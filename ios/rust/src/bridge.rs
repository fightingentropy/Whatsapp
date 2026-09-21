//! A small, versioned C boundary. Only UI commands cross it, never protobufs or keys.

use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use serde::Deserialize;
use serde_json::{Value, json};

use crate::backend::{Backend, Command, Event, LinkStatus, Waker};
use crate::model::{Chat, Content, MediaState, Message};
use crate::paths::AppDirs;

static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);
static SESSIONS: OnceLock<Mutex<HashMap<u64, Backend>>> = OnceLock::new();

fn sessions() -> &'static Mutex<HashMap<u64, Backend>> {
    SESSIONS.get_or_init(Mutex::default)
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Input {
    Pair {
        phone: String,
    },
    Reconnect {},
    Load {
        chat: String,
        before: Option<(i64, String)>,
    },
    Older {
        chat: String,
    },
    Send {
        chat: String,
        text: String,
        quoting: Option<String>,
    },
    Read {
        chat: String,
        receipts: bool,
    },
    Download {
        chat: String,
        message: String,
    },
    Avatar {
        id: String,
    },
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

fn valid_chat(value: &str) -> bool {
    valid_id(value)
        && value.split_once('@').is_some_and(|(user, server)| {
            !user.is_empty() && !server.is_empty() && !value.chars().any(char::is_whitespace)
        })
}

fn parse_command(input: &str) -> Option<Command> {
    // Bound allocation at the interface. WhatsApp text is capped at 65,536 characters.
    if input.len() > 512 * 1024 {
        return None;
    }
    match serde_json::from_str::<Input>(input).ok()? {
        Input::Pair { phone }
            if (7..=15).contains(&phone.len()) && phone.bytes().all(|c| c.is_ascii_digit()) =>
        {
            Some(Command::PairWithPhone(phone))
        }
        Input::Reconnect {} => Some(Command::Reconnect),
        Input::Load { chat, before }
            if valid_chat(&chat)
                && before
                    .as_ref()
                    .is_none_or(|(time, id)| *time >= 0 && valid_id(id)) =>
        {
            Some(Command::LoadChat { chat, before })
        }
        Input::Older { chat } if valid_chat(&chat) => Some(Command::FetchOlder(chat)),
        Input::Send {
            chat,
            text,
            quoting,
        } if valid_chat(&chat)
            && !text.trim().is_empty()
            && text.chars().count() <= 65_536
            && quoting.as_ref().is_none_or(|id| valid_id(id)) =>
        {
            Some(Command::SendText {
                chat,
                text,
                quoting,
                mentions: Vec::new(),
            })
        }
        Input::Read { chat, receipts } if valid_chat(&chat) => {
            Some(Command::MarkRead { chat, receipts })
        }
        Input::Download { chat, message } if valid_chat(&chat) && valid_id(&message) => {
            Some(Command::Download { chat, message })
        }
        Input::Avatar { id } if valid_chat(&id) => Some(Command::FetchAvatar { id, full: false }),
        _ => None,
    }
}

fn chat_json(chat: &Chat) -> Value {
    json!({
        "id": chat.id, "name": chat.name, "kind": chat.kind,
        "timestamp": chat.last_activity, "unread": chat.unread,
        "archived": chat.archived, "pinned": chat.pinned,
        "readOnly": chat.read_only || chat.kind == crate::model::ChatKind::Broadcast,
        "preview": chat.last.as_ref().map_or("", |last| last.summary.as_str()),
    })
}

fn message_json(message: &Message) -> Value {
    let (kind, text) = match &message.content {
        Content::Text { text, .. } => ("text", text.clone()),
        Content::Image { caption, .. } => ("image", caption.clone().unwrap_or_default()),
        Content::Video { caption, .. } => ("video", caption.clone().unwrap_or_default()),
        Content::Audio { .. } => ("audio", message.summary()),
        Content::Document {
            caption, file_name, ..
        } => (
            "document",
            caption.clone().unwrap_or_else(|| file_name.clone()),
        ),
        Content::Sticker { .. } => ("sticker", String::new()),
        Content::Revoked => ("revoked", message.summary()),
        _ => ("other", message.summary()),
    };
    let media = message.content.media();
    json!({
        "id": message.id, "chat": message.chat, "sender": message.sender,
        "senderName": message.sender_name, "fromMe": message.from_me,
        "timestamp": message.timestamp, "kind": kind, "text": text,
        "status": message.status, "edited": message.edited,
        "mediaPath": media.and_then(|m| m.path.as_ref()),
        "hasMedia": media.is_some(),
        "mediaState": match media.map(|m| &m.state) {
            Some(MediaState::Downloading) => "downloading",
            Some(MediaState::Failed(_)) => "failed",
            _ => "idle",
        },
        "mediaError": media.and_then(|m| if let MediaState::Failed(error) = &m.state { Some(error) } else { None }),
        "quote": message.quoted.as_ref().map(|q| json!({"id":q.id,"sender":q.sender_name.as_deref().unwrap_or(&q.sender),"text":q.summary})),
        "reactions": message.reactions.iter().map(|r| &r.emoji).collect::<Vec<_>>(),
    })
}

fn event_json(event: Event) -> Option<Value> {
    Some(match event {
        Event::Link(status) => match status {
            LinkStatus::Starting => json!({"type":"link","status":"starting"}),
            LinkStatus::Unlinked { qr, pair_code, .. } => {
                json!({"type":"link","status":"unlinked","qr":qr,"code":pair_code})
            }
            LinkStatus::Connecting => json!({"type":"link","status":"connecting"}),
            LinkStatus::Connected => json!({"type":"link","status":"connected"}),
            LinkStatus::Disconnected { reason } => {
                json!({"type":"link","status":"disconnected","detail":reason})
            }
            LinkStatus::LoggedOut => json!({"type":"link","status":"logged_out"}),
            LinkStatus::Failed(detail) => json!({"type":"link","status":"failed","detail":detail}),
        },
        Event::Me { id, name, .. } => json!({"type":"me","id":id,"name":name}),
        Event::Chats(chats) => {
            json!({"type":"chats","chats":chats.iter().map(chat_json).collect::<Vec<_>>()})
        }
        Event::ChatUpdated(chat) => json!({"type":"chats","chats":[chat_json(&chat)]}),
        Event::Messages {
            chat,
            messages,
            older,
            complete,
            requested,
        } => {
            json!({"type":"messages","chat":chat,"messages":messages.iter().map(message_json).collect::<Vec<_>>(),"older":older,"complete":complete,"requested":requested})
        }
        Event::MessageUpdated(message) => {
            json!({"type":"message","message":message_json(&message)})
        }
        Event::ChatLoadFailed { chat, error, .. } => {
            json!({"type":"load_failed","chat":chat,"detail":error})
        }
        Event::ChatMerged { from, into } => json!({"type":"merged","from":from,"into":into}),
        Event::MessageDeleted { chat, id } => json!({"type":"deleted","chat":chat,"id":id}),
        Event::Contacts(contacts) => {
            json!({"type":"contacts","contacts":contacts.iter().map(|c| json!({"id":c.id,"name":c.display_name()})).collect::<Vec<_>>()})
        }
        Event::Avatar {
            id,
            full: false,
            path,
        } => json!({"type":"avatar","id":id,"path":path}),
        Event::Media {
            chat,
            message,
            result,
        } => match result {
            Ok(path) => json!({"type":"media","chat":chat,"id":message,"path":path}),
            Err(error) => json!({"type":"media","chat":chat,"id":message,"detail":error}),
        },
        Event::Syncing(active) => json!({"type":"sync","active":active}),
        Event::SyncProgress(progress) => json!({"type":"progress","progress":progress}),
        Event::OlderFetched { chat, more } => json!({"type":"older","chat":chat,"more":more}),
        Event::ReceiptsPrivacy { disabled } => json!({"type":"privacy","disabled":disabled}),
        Event::Info(detail) => json!({"type":"info","detail":detail}),
        Event::Error(detail) => json!({"type":"error","detail":detail}),
        // Native iOS does not expose the desktop picker, updater or notifications.
        _ => return None,
    })
}

/// Starts one independent iOS session. Returns zero for invalid paths or a second session.
///
/// # Safety
/// `root` must be a valid, NUL-terminated UTF-8 C string for this call. `wake`
/// must remain callable from any thread until `wa_stop` returns. It must enqueue
/// UI work rather than synchronously re-entering this API.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wa_start(root: *const c_char, wake: Option<extern "C" fn()>) -> u64 {
    if root.is_null() {
        return 0;
    }
    let Ok(root) = (unsafe { CStr::from_ptr(root) }).to_str() else {
        return 0;
    };
    let root = Path::new(root);
    if !root.is_absolute() || root.parent().is_none() {
        return 0;
    }
    let mut sessions = sessions().lock().unwrap_or_else(|p| p.into_inner());
    if !sessions.is_empty() {
        return 0;
    }
    let dirs = AppDirs::under(root);
    if dirs.ensure().is_err() {
        return 0;
    }
    #[cfg(target_os = "ios")]
    let waker = Waker::new(move || {
        if let Some(wake) = wake {
            wake();
        }
    });
    #[cfg(not(target_os = "ios"))]
    let waker = {
        let _ = wake;
        Waker::default()
    };
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    sessions.insert(handle, Backend::spawn(dirs, waker));
    handle
}

/// Enqueues a validated command. 1 means queued, 0 invalid input, -1 stale handle.
///
/// # Safety
/// `input` must point to a valid NUL-terminated UTF-8 string for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wa_command(handle: u64, input: *const c_char) -> i32 {
    if input.is_null() {
        return 0;
    }
    let Ok(input) = (unsafe { CStr::from_ptr(input) }).to_str() else {
        return 0;
    };
    let Some(command) = parse_command(input) else {
        return 0;
    };
    let sessions = sessions().lock().unwrap_or_else(|p| p.into_inner());
    let Some(backend) = sessions.get(&handle) else {
        return -1;
    };
    backend.send(command);
    1
}

/// Drains queued events. Caller owns the result and must call `wa_free_string` once.
/// Returns null for a stale handle. Event JSON must never be logged.
#[unsafe(no_mangle)]
pub extern "C" fn wa_poll(handle: u64) -> *mut c_char {
    let events = {
        let sessions = sessions().lock().unwrap_or_else(|p| p.into_inner());
        let Some(backend) = sessions.get(&handle) else {
            return std::ptr::null_mut();
        };
        backend.poll()
    };
    let values: Vec<_> = events.into_iter().filter_map(event_json).collect();
    CString::new(json!({"version":1,"events":values}).to_string())
        .expect("JSON escapes NUL bytes")
        .into_raw()
}

/// # Safety
/// `value` must be null or a pointer returned by `wa_poll` that has not yet been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wa_free_string(value: *mut c_char) {
    if !value.is_null() {
        drop(unsafe { CString::from_raw(value) });
    }
}

/// Finishes database/network work. Call off the main thread; never deletes the session.
/// All API calls, including stop/start, must be serialized by the owning UI engine.
#[unsafe(no_mangle)]
pub extern "C" fn wa_stop(handle: u64) {
    let backend = sessions()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(&handle);
    if let Some(mut backend) = backend {
        backend.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Delivery;

    #[test]
    fn rejects_internal_commands_and_invalid_recipients() {
        for input in [
            r#"{"type":"shutdown"}"#,
            r#"{"type":"send","chat":"","text":"hi"}"#,
            r#"{"type":"send","chat":"a@b","text":"  "}"#,
            r#"{"type":"pair","phone":"+44"}"#,
            r#"{"type":"reconnect","unexpected":true}"#,
        ] {
            assert!(
                parse_command(input).is_none(),
                "unexpected fixture accepted: {input}"
            );
        }
        assert!(
            matches!(parse_command(r#"{"type":"send","chat":"fixture@g.us","text":"Hello\n👋","quoting":"quoted"}"#), Some(Command::SendText { text, quoting: Some(id), .. }) if text == "Hello\n👋" && id == "quoted")
        );
    }

    #[test]
    fn bridge_preserves_paging_flags_and_full_text() {
        let message = Message {
            id: "m".into(),
            chat: "fixture@g.us".into(),
            sender: "fixture@lid".into(),
            sender_name: None,
            from_me: false,
            timestamp: 1,
            content: Content::text("First\nSecond 👋"),
            status: Delivery::None,
            delivered_at: None,
            read_at: None,
            quoted: None,
            reactions: vec![],
            edited: false,
            mentions: vec![],
            forwarded: false,
            thumbnail: None,
        };
        let event = event_json(Event::Messages {
            chat: message.chat.clone(),
            messages: vec![message],
            older: false,
            complete: false,
            requested: false,
        })
        .unwrap();
        assert_eq!(event["messages"][0]["text"], "First\nSecond 👋");
        assert_eq!(event["requested"], false);
        assert_eq!(event["complete"], false);
    }

    #[test]
    fn stale_handles_are_safe_and_commands_are_recorded_without_network() {
        let (mut backend, mut commands) = Backend::recording();
        backend.send(
            parse_command(r#"{"type":"read","chat":"fixture@g.us","receipts":false}"#).unwrap(),
        );
        assert!(matches!(
            commands.try_recv().unwrap(),
            Command::MarkRead {
                receipts: false,
                ..
            }
        ));
        backend.set_offline(true);
        backend.record_demo_commands();
        backend.send(Command::Reconnect);
        assert!(matches!(
            backend.take_demo_commands()[0],
            Command::Reconnect
        ));
        assert!(wa_poll(u64::MAX).is_null());
        wa_stop(u64::MAX);
        let command = CString::new(r#"{"type":"reconnect"}"#).unwrap();
        assert_eq!(unsafe { wa_command(u64::MAX, command.as_ptr()) }, -1);
        assert_eq!(unsafe { wa_command(1, std::ptr::null()) }, 0);
    }
}
