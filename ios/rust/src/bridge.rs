//! A small, versioned C boundary. Only UI commands cross it, never protobufs or keys.

use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use serde::Deserialize;
use serde_json::{Value, json};

use crate::backend::{Backend, Command, Event, LinkStatus, Waker};
use crate::model::{Chat, Content, Gif, MediaState, Message};
use crate::paths::AppDirs;

static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);
struct Session {
    backend: Backend,
    dirs: AppDirs,
}

static SESSIONS: OnceLock<Mutex<HashMap<u64, Session>>> = OnceLock::new();

fn sessions() -> &'static Mutex<HashMap<u64, Session>> {
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
    Around {
        chat: String,
        id: String,
    },
    Newer {
        chat: String,
        after: (i64, String),
    },
    Older {
        chat: String,
    },
    Send {
        chat: String,
        text: String,
        quoting: Option<String>,
        #[serde(default)]
        mentions: Vec<String>,
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
        #[serde(default)]
        full: bool,
    },
    Search {
        query: String,
    },
    Until {
        chat: String,
        id: String,
        before: (i64, String),
    },
    Ensure {
        chat: String,
        name: String,
    },
    Compose {
        chat: String,
        composing: bool,
    },
    Edit {
        chat: String,
        id: String,
        text: String,
        #[serde(default)]
        mentions: Vec<String>,
    },
    React {
        chat: String,
        message: String,
        emoji: String,
    },
    Forward {
        chat: String,
        message: String,
        to: String,
    },
    Delete {
        chat: String,
        id: String,
        everyone: bool,
    },
    Archive {
        chat: String,
        value: bool,
    },
    Pin {
        chat: String,
        value: bool,
    },
    Mute {
        chat: String,
        until: Option<i64>,
    },
    Files {
        chat: String,
        paths: Vec<PathBuf>,
        caption: Option<String>,
        #[serde(default)]
        mentions: Vec<String>,
    },
    Voice {
        chat: String,
        path: PathBuf,
        quoting: Option<String>,
    },
    Played {
        chat: String,
        message: String,
        sender: String,
        receipts: bool,
    },
    NewContact {
        phone: String,
        name: Option<String>,
        to_phone: bool,
    },
    SaveContact {
        id: String,
        name: String,
        to_phone: bool,
    },
    Stickers {},
    Sticker {
        chat: String,
        path: PathBuf,
    },
    SaveSticker {
        path: PathBuf,
    },
    ForgetSticker {
        path: PathBuf,
    },
    ImportStickerUrl {
        url: String,
    },
    ImportStickerArchive {
        path: PathBuf,
    },
    DeleteStickerPack {
        path: PathBuf,
    },
    Gifs {
        query: String,
        key: String,
    },
    Gif {
        chat: String,
        id: String,
        mp4: String,
        width: u32,
        height: u32,
    },
    Unlink {},
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

fn scoped_file(path: &Path, root: &Path) -> Option<PathBuf> {
    let path = path.canonicalize().ok()?;
    let root = root.canonicalize().ok()?;
    (path.starts_with(root) && path.is_file()).then_some(path)
}

fn attachment(path: &Path, dirs: Option<&AppDirs>) -> Option<PathBuf> {
    let dirs = dirs?;
    scoped_file(path, &dirs.cache).or_else(|| scoped_file(path, &dirs.saved_sticker_dir()))
}

fn valid_text(text: &str) -> bool {
    text.chars().count() <= 65_536
}
fn valid_mentions(ids: &[String]) -> bool {
    ids.len() <= 512 && ids.iter().all(|id| valid_chat(id))
}

fn valid_giphy_url(value: &str) -> bool {
    let Ok(uri) = value.parse::<ureq::http::Uri>() else {
        return false;
    };
    uri.scheme_str() == Some("https")
        && uri.authority().is_some_and(|a| !a.as_str().contains('@'))
        && uri.port_u16().is_none_or(|port| port == 443)
        && uri
            .host()
            .is_some_and(|host| host == "giphy.com" || host.ends_with(".giphy.com"))
}

fn parse_command(input: &str, dirs: Option<&AppDirs>) -> Option<Command> {
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
            Some(Command::LoadWindow { chat, before })
        }
        Input::Around { chat, id } if valid_chat(&chat) && valid_id(&id) => {
            Some(Command::LoadWindowAround { chat, id })
        }
        Input::Newer { chat, after } if valid_chat(&chat) && after.0 >= 0 && valid_id(&after.1) => {
            Some(Command::LoadNewer { chat, after })
        }
        Input::Older { chat } if valid_chat(&chat) => Some(Command::FetchOlder(chat)),
        Input::Send {
            chat,
            text,
            quoting,
            mentions,
        } if valid_chat(&chat)
            && !text.trim().is_empty()
            && text.chars().count() <= 65_536
            && quoting.as_ref().is_none_or(|id| valid_id(id)) =>
        {
            Some(Command::SendText {
                chat,
                text,
                quoting,
                mentions: valid_mentions(&mentions).then_some(mentions)?,
            })
        }
        Input::Read { chat, receipts } if valid_chat(&chat) => {
            Some(Command::MarkRead { chat, receipts })
        }
        Input::Download { chat, message } if valid_chat(&chat) && valid_id(&message) => {
            Some(Command::Download { chat, message })
        }
        Input::Avatar { id, full } if valid_chat(&id) => Some(Command::FetchAvatar { id, full }),
        Input::Search { query } if query.len() <= 1024 => Some(Command::SearchMessages { query }),
        Input::Until { chat, id, before }
            if valid_chat(&chat) && valid_id(&id) && before.0 >= 0 && valid_id(&before.1) =>
        {
            Some(Command::LoadUntil { chat, id, before })
        }
        Input::Ensure { chat, name } if valid_chat(&chat) && name.len() <= 512 => {
            Some(Command::EnsureChat { chat, name })
        }
        Input::Compose { chat, composing } if valid_chat(&chat) => {
            Some(Command::Composing { chat, composing })
        }
        Input::Edit {
            chat,
            id,
            text,
            mentions,
        } if valid_chat(&chat)
            && valid_id(&id)
            && valid_text(&text)
            && !text.trim().is_empty()
            && valid_mentions(&mentions) =>
        {
            Some(Command::EditText {
                chat,
                id,
                text,
                mentions,
            })
        }
        Input::React {
            chat,
            message,
            emoji,
        } if valid_chat(&chat)
            && valid_id(&message)
            && emoji.len() <= 128
            && !emoji.chars().any(char::is_control) =>
        {
            Some(Command::React {
                chat,
                message,
                emoji,
            })
        }
        Input::Forward { chat, message, to }
            if valid_chat(&chat) && valid_id(&message) && valid_chat(&to) =>
        {
            Some(Command::Forward {
                from_chat: chat,
                message,
                to_chat: to,
            })
        }
        Input::Delete { chat, id, everyone } if valid_chat(&chat) && valid_id(&id) => {
            Some(if everyone {
                Command::Revoke { chat, id }
            } else {
                Command::DeleteLocal { chat, id }
            })
        }
        Input::Archive { chat, value } if valid_chat(&chat) => {
            Some(Command::SetArchived(chat, value))
        }
        Input::Pin { chat, value } if valid_chat(&chat) => Some(Command::SetPinned(chat, value)),
        Input::Mute { chat, until } if valid_chat(&chat) && until.is_none_or(|time| time >= 0) => {
            Some(Command::SetMuted(chat, until))
        }
        Input::Files {
            chat,
            paths,
            caption,
            mentions,
        } if valid_chat(&chat)
            && !paths.is_empty()
            && paths.len() <= 30
            && caption.as_deref().is_none_or(valid_text)
            && valid_mentions(&mentions) =>
        {
            Some(Command::SendFiles {
                chat,
                paths: paths
                    .iter()
                    .map(|p| {
                        let path = scoped_file(p, &dirs?.cache.join("outgoing"))?;
                        (std::fs::metadata(&path).ok()?.len() <= 100 * 1024 * 1024).then_some(path)
                    })
                    .collect::<Option<_>>()?,
                caption,
                mentions,
            })
        }
        Input::Voice {
            chat,
            path,
            quoting,
        } if valid_chat(&chat) && quoting.as_ref().is_none_or(|id| valid_id(id)) => {
            let path = scoped_file(&path, &dirs?.cache.join("outgoing"))?;
            let size = std::fs::metadata(&path).ok()?.len();
            if size == 0 || size > 48_000 * 4 * 600 || size % 4 != 0 {
                return None;
            }
            let bytes = std::fs::read(path).ok()?;
            let samples: Vec<f32> = bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|chunk| f32::from_le_bytes(*chunk))
                .collect();
            if samples.iter().any(|s| !s.is_finite() || s.abs() > 1.0) {
                return None;
            }
            Some(Command::SendVoice {
                chat,
                samples,
                quoting,
            })
        }
        Input::Played {
            chat,
            message,
            sender,
            receipts,
        } if valid_chat(&chat) && valid_id(&message) && valid_chat(&sender) => {
            Some(Command::MarkPlayed {
                chat,
                message,
                sender,
                receipts,
            })
        }
        Input::NewContact {
            phone,
            name,
            to_phone,
        } if (7..=15).contains(&phone.len())
            && phone.bytes().all(|c| c.is_ascii_digit())
            && name.as_ref().is_none_or(|s| s.len() <= 512) =>
        {
            Some(Command::NewContact {
                phone,
                full_name: name,
                first_name: None,
                to_phone,
            })
        }
        Input::SaveContact { id, name, to_phone }
            if valid_chat(&id) && !name.trim().is_empty() && name.len() <= 512 =>
        {
            Some(Command::SaveContact {
                id,
                full_name: name,
                first_name: None,
                to_phone,
            })
        }
        Input::Stickers {} => Some(Command::RecentStickers),
        Input::Sticker { chat, path } if valid_chat(&chat) => Some(Command::SendSticker {
            chat,
            path: attachment(&path, dirs)?,
        }),
        Input::SaveSticker { path } => Some(Command::SaveSticker {
            path: attachment(&path, dirs)?,
        }),
        Input::ForgetSticker { path } => {
            let root = dirs?.saved_sticker_dir().canonicalize().ok()?;
            let path = scoped_file(&path, &root)?;
            (path.parent() == Some(root.as_path())).then_some(Command::ForgetSticker { path })
        }
        Input::ImportStickerUrl { url }
            if crate::backend::sticker_import::parse_signal_url(&url).is_ok() =>
        {
            Some(Command::ImportStickerUrl { url })
        }
        Input::ImportStickerArchive { path } => Some(Command::ImportStickerArchive {
            path: scoped_file(&path, &dirs?.cache.join("outgoing"))?,
        }),
        Input::DeleteStickerPack { path } => {
            let root = dirs?
                .saved_sticker_dir()
                .join("packs")
                .canonicalize()
                .ok()?;
            let path = path.canonicalize().ok()?;
            (path.parent() == Some(root.as_path()) && path.is_dir())
                .then_some(Command::DeleteStickerPack { dir: path })
        }
        Input::Gifs { query, key } if query.len() <= 1024 && key.len() <= 512 => {
            Some(Command::SearchGifs { query, key })
        }
        Input::Gif {
            chat,
            id,
            mp4,
            width,
            height,
        } if valid_chat(&chat)
            && valid_id(&id)
            && mp4.len() <= 4096
            && valid_giphy_url(&mp4)
            && (1..=4096).contains(&width)
            && (1..=4096).contains(&height) =>
        {
            Some(Command::SendGif {
                chat,
                gif: Gif {
                    id,
                    still: None,
                    mp4,
                    width,
                    height,
                },
            })
        }
        Input::Unlink {} => Some(Command::Unlink),
        _ => None,
    }
}

fn chat_json(chat: &Chat) -> Value {
    json!({
        "id": chat.id, "name": chat.name, "kind": chat.kind,
        "timestamp": chat.last_activity, "unread": chat.unread,
        "archived": chat.archived, "pinned": chat.pinned,
        "mutedUntil": chat.muted_until, "participants": chat.participants,
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
        "deliveredAt": message.delivered_at, "readAt": message.read_at,
        "forwarded": message.forwarded, "mentions": message.mentions,
        "content": message.content, "reactionDetails": message.reactions,
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
        Event::Me { id, name, about } => json!({"type":"me","id":id,"name":name,"about":about}),
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
        Event::WindowAround {
            chat,
            messages,
            older_complete,
            newer_complete,
        } => {
            json!({"type":"around","chat":chat,"messages":messages.iter().map(message_json).collect::<Vec<_>>(),"complete":older_complete,"more":!newer_complete})
        }
        Event::NewerMessages {
            chat,
            messages,
            complete,
        } => {
            json!({"type":"newer","chat":chat,"messages":messages.iter().map(message_json).collect::<Vec<_>>(),"complete":complete})
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
            json!({"type":"contacts","contacts":contacts.iter().map(|c| json!({"id":c.id,"name":c.display_name(),"fullName":c.full_name,"pushName":c.push_name})).collect::<Vec<_>>()})
        }
        Event::Avatar { id, full, path } => {
            json!({"type":"avatar","id":id,"path":path,"full":full})
        }
        Event::SearchHits { query, messages } => {
            json!({"type":"search","query":query,"messages":messages.iter().map(message_json).collect::<Vec<_>>()})
        }
        Event::ChatSearchHits {
            chat,
            request,
            result,
            truncated,
        } => match result {
            Ok(messages) => {
                json!({"type":"chat_search","chat":chat,"request":request,"messages":messages.iter().map(message_json).collect::<Vec<_>>(),"truncated":truncated})
            }
            Err(error) => json!({"type":"chat_search","chat":chat,"request":request,"error":error}),
        },
        Event::Typing {
            chat,
            sender,
            composing,
        } => json!({"type":"typing","chat":chat,"id":sender,"composing":composing}),
        Event::Presence {
            id,
            online,
            last_seen,
        } => json!({"type":"presence","id":id,"online":online,"lastSeen":last_seen}),
        Event::ContactReady { id, name } => json!({"type":"contact_ready","id":id,"name":name}),
        Event::Incoming { chat, message } => {
            json!({"type":"incoming","chat":chat,"message":message_json(&message)})
        }
        Event::Gifs { query, results } => match results {
            Ok(gifs) => {
                json!({"type":"gifs","query":query,"gifs":gifs.iter().map(|g| json!({"id":g.id,"still":g.still,"mp4":g.mp4,"width":g.width,"height":g.height})).collect::<Vec<_>>()})
            }
            Err(error) => json!({"type":"gifs","query":query,"detail":error.message}),
        },
        Event::Stickers {
            saved,
            packs,
            recent,
        } => {
            json!({"type":"stickers","saved":saved,"recent":recent,"packs":packs.iter().map(|p| json!({"name":p.name,"dir":p.dir,"stickers":p.stickers})).collect::<Vec<_>>()})
        }
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
    let backend = Backend::spawn(dirs.clone(), waker);
    sessions.insert(handle, Session { backend, dirs });
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
    let sessions = sessions().lock().unwrap_or_else(|p| p.into_inner());
    let Some(session) = sessions.get(&handle) else {
        return -1;
    };
    let Some(command) = parse_command(input, Some(&session.dirs)) else {
        return 0;
    };
    session.backend.send(command);
    1
}

/// Drains queued events. Caller owns the result and must call `wa_free_string` once.
/// Returns null for a stale handle. Event JSON must never be logged.
#[unsafe(no_mangle)]
pub extern "C" fn wa_poll(handle: u64) -> *mut c_char {
    let events = {
        let sessions = sessions().lock().unwrap_or_else(|p| p.into_inner());
        let Some(session) = sessions.get(&handle) else {
            return std::ptr::null_mut();
        };
        session.backend.poll()
    };
    let values: Vec<_> = events.into_iter().filter_map(event_json).collect();
    CString::new(json!({"version":1,"events":values}).to_string())
        .expect("JSON escapes NUL bytes")
        .into_raw()
}

/// # Safety
/// `value` must be null or an unfreed pointer returned by `wa_poll` or `wa_prepare_audio`.
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
    if let Some(mut session) = backend {
        session.backend.shutdown();
    }
}

/// Converts an archived OGG/Opus file to a cached WAV for native iOS playback.
/// Returns a path owned by the caller, freed with `wa_free_string`, or null.
/// # Safety
/// `source` must be a valid NUL-terminated UTF-8 string. Serialize with other API calls.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wa_prepare_audio(handle: u64, source: *const c_char) -> *mut c_char {
    if source.is_null() {
        return std::ptr::null_mut();
    }
    let result = (|| -> Option<PathBuf> {
        let source = unsafe { CStr::from_ptr(source) }.to_str().ok()?;
        let dirs = sessions()
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&handle)?
            .dirs
            .clone();
        let path = scoped_file(Path::new(source), &dirs.media_cache_dir())?;
        if std::fs::metadata(&path).ok()?.len() > 64 * 1024 * 1024 {
            return None;
        }
        let bytes = std::fs::read(path).ok()?;
        use sha2::{Digest, Sha256};
        let output = dirs.cache.join("audio-preview").join(format!(
            "{}.wav",
            Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        ));
        if output.is_file() {
            return Some(output);
        }
        // An Opus packet can pad a recording by one 20 ms frame.
        let samples = crate::voice::decode_limited(&bytes, 48_000 * 600 + 960).ok()?;
        let length = u32::try_from(samples.len() * 2).ok()?;
        let mut wave = Vec::with_capacity(length as usize + 44);
        wave.extend_from_slice(b"RIFF");
        wave.extend_from_slice(&(length + 36).to_le_bytes());
        wave.extend_from_slice(b"WAVEfmt ");
        wave.extend_from_slice(&16_u32.to_le_bytes());
        wave.extend_from_slice(&1_u16.to_le_bytes());
        wave.extend_from_slice(&1_u16.to_le_bytes());
        wave.extend_from_slice(&48_000_u32.to_le_bytes());
        wave.extend_from_slice(&96_000_u32.to_le_bytes());
        wave.extend_from_slice(&2_u16.to_le_bytes());
        wave.extend_from_slice(&16_u16.to_le_bytes());
        wave.extend_from_slice(b"data");
        wave.extend_from_slice(&length.to_le_bytes());
        for sample in samples {
            wave.extend_from_slice(&((sample.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
        }
        std::fs::create_dir_all(output.parent()?).ok()?;
        let temporary = output.with_extension("wav.tmp");
        std::fs::write(&temporary, wave).ok()?;
        std::fs::rename(temporary, &output).ok()?;
        Some(output)
    })();
    result
        .and_then(|p| CString::new(p.to_string_lossy().as_bytes()).ok())
        .map_or(std::ptr::null_mut(), CString::into_raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Delivery;

    struct FilesFixture {
        root: PathBuf,
        dirs: AppDirs,
    }
    impl FilesFixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "whatsapp-ios-bridge-{}-{}",
                std::process::id(),
                NEXT_HANDLE.fetch_add(1, Ordering::Relaxed)
            ));
            let dirs = AppDirs::under(&root);
            for path in [
                dirs.cache.join("outgoing"),
                dirs.media_cache_dir(),
                dirs.saved_sticker_dir().join("packs/fixture"),
                dirs.config.clone(),
            ] {
                std::fs::create_dir_all(path).unwrap();
            }
            Self { root, dirs }
        }
        fn command(&self, value: Value) -> Option<Command> {
            parse_command(&value.to_string(), Some(&self.dirs))
        }
    }
    impl Drop for FilesFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn attachment_commands_cannot_read_keys_or_follow_outgoing_symlinks() {
        let f = FilesFixture::new();
        let photo = f.dirs.cache.join("outgoing/photo.jpg");
        let secret = f.dirs.session_db();
        std::fs::write(&photo, b"fixture").unwrap();
        std::fs::write(&secret, b"fixture-private-key").unwrap();
        let send = |path: &Path| json!({"type":"files", "chat":"fixture@g.us", "paths":[path], "caption":"Caption"});
        assert!(
            matches!(f.command(send(&photo)), Some(Command::SendFiles { caption: Some(caption), .. }) if caption == "Caption")
        );
        assert!(f.command(send(&secret)).is_none());
        let escape = f.dirs.cache.join("outgoing/escape.jpg");
        std::os::unix::fs::symlink(&secret, &escape).unwrap();
        assert!(f.command(send(&escape)).is_none());
        assert!(
            f.command(json!({"type":"files","chat":"fixture@g.us","paths":vec![&photo;31]}))
                .is_none()
        );
        assert!(
            f.command(json!({"type":"files","chat":"fixture@g.us","paths":[]}))
                .is_none()
        );
    }

    #[test]
    fn sticker_removal_is_scoped_to_the_selected_saved_file_or_pack() {
        let f = FilesFixture::new();
        let saved = f.dirs.saved_sticker_dir().join("saved.webp");
        let pack = f.dirs.saved_sticker_dir().join("packs/fixture");
        let packed = pack.join("sticker.webp");
        std::fs::write(&saved, b"fixture").unwrap();
        std::fs::write(&packed, b"fixture").unwrap();
        assert!(matches!(
            f.command(json!({"type":"forget_sticker","path":saved})),
            Some(Command::ForgetSticker { .. })
        ));
        assert!(
            f.command(json!({"type":"forget_sticker","path":packed}))
                .is_none()
        );
        assert!(matches!(
            f.command(json!({"type":"delete_sticker_pack","path":pack})),
            Some(Command::DeleteStickerPack { .. })
        ));
        assert!(
            f.command(json!({"type":"delete_sticker_pack","path":f.dirs.saved_sticker_dir()}))
                .is_none()
        );
        assert!(
            f.command(json!({"type":"delete_sticker_pack","path":f.dirs.state}))
                .is_none()
        );
    }

    #[test]
    fn voice_samples_are_bounded_finite_and_owned_by_the_command() {
        let f = FilesFixture::new();
        let path = f.dirs.cache.join("outgoing/recording.f32");
        let command =
            || json!({"type":"voice","chat":"fixture@g.us","path":path,"quoting":"reply"});
        for sample in [f32::NAN, f32::INFINITY, 1.1] {
            std::fs::write(&path, sample.to_le_bytes()).unwrap();
            assert!(f.command(command()).is_none());
        }
        std::fs::write(&path, [0, 1, 2]).unwrap();
        assert!(f.command(command()).is_none());
        std::fs::write(&path, 0.5_f32.to_le_bytes()).unwrap();
        let parsed = f.command(command());
        std::fs::remove_file(&path).unwrap();
        assert!(
            matches!(parsed, Some(Command::SendVoice { samples, quoting: Some(id), .. }) if samples == [0.5] && id == "reply")
        );
    }

    #[test]
    fn native_audio_conversion_produces_a_scoped_reusable_wave_file() {
        let f = FilesFixture::new();
        let source = f.dirs.media_cache_dir().join("fixture.ogg");
        let encoded = crate::voice::encode(&vec![0.125; 9_600]).unwrap();
        std::fs::write(&source, encoded).unwrap();
        let (backend, _) = Backend::recording();
        let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
        sessions().lock().unwrap().insert(
            handle,
            Session {
                backend,
                dirs: f.dirs.clone(),
            },
        );
        let input = CString::new(source.to_str().unwrap()).unwrap();
        let pointer = unsafe { wa_prepare_audio(handle, input.as_ptr()) };
        assert!(!pointer.is_null());
        let output = PathBuf::from(unsafe { CStr::from_ptr(pointer) }.to_str().unwrap());
        unsafe { wa_free_string(pointer) };
        assert!(output.starts_with(f.dirs.cache.join("audio-preview")));
        let wave = std::fs::read(&output).unwrap();
        assert_eq!(&wave[..4], b"RIFF");
        assert_eq!(&wave[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(wave[24..28].try_into().unwrap()), 48_000);
        assert_eq!(
            u32::from_le_bytes(wave[40..44].try_into().unwrap()) as usize,
            wave.len() - 44
        );
        let cached = unsafe { wa_prepare_audio(handle, input.as_ptr()) };
        assert_eq!(
            unsafe { CStr::from_ptr(cached) }.to_str().unwrap(),
            output.to_str().unwrap()
        );
        unsafe { wa_free_string(cached) };
        let private = CString::new(f.dirs.session_db().to_str().unwrap()).unwrap();
        assert!(unsafe { wa_prepare_audio(handle, private.as_ptr()) }.is_null());
        assert!(unsafe { wa_prepare_audio(handle, std::ptr::null()) }.is_null());
        wa_stop(handle);
        assert!(unsafe { wa_prepare_audio(handle, input.as_ptr()) }.is_null());
    }

    #[test]
    fn gif_urls_cannot_redirect_the_native_sender_to_arbitrary_hosts() {
        assert!(valid_giphy_url(
            "https://media3.giphy.com/media/fixture/giphy.mp4?cid=fixture"
        ));
        for url in [
            "http://media.giphy.com/a.mp4",
            "https://giphy.com.evil.example/a",
            "https://evil.example/?x=.giphy.com",
            "https://evil.example#x.giphy.com",
            "https://giphy.com@evil.example/a",
            "https://evil.example@giphy.com/a",
            "https://media.giphy.com:9999/a",
            "file:///a.mp4",
        ] {
            assert!(!valid_giphy_url(url), "accepted fixture {url}");
        }
    }

    #[test]
    fn parity_commands_keep_recipients_mentions_and_privacy() {
        assert!(
            matches!(parse_command(r#"{"type":"edit","chat":"a@g.us","id":"m","text":"Hi @b","mentions":["b@lid"]}"#, None), Some(Command::EditText { mentions, .. }) if mentions == ["b@lid"])
        );
        assert!(
            matches!(parse_command(r#"{"type":"forward","chat":"a@g.us","message":"m","to":"b@lid"}"#, None), Some(Command::Forward { from_chat, to_chat, .. }) if from_chat == "a@g.us" && to_chat == "b@lid")
        );
        assert!(matches!(
            parse_command(
                r#"{"type":"played","chat":"a@lid","message":"m","sender":"a@lid","receipts":false}"#,
                None
            ),
            Some(Command::MarkPlayed {
                receipts: false,
                ..
            })
        ));
        assert!(matches!(
            parse_command(
                r#"{"type":"delete","chat":"a@lid","id":"m","everyone":false}"#,
                None
            ),
            Some(Command::DeleteLocal { .. })
        ));
        assert!(
            parse_command(
                r#"{"type":"edit","chat":"a@g.us","id":"m","text":"Hi","mentions":["invalid"]}"#,
                None
            )
            .is_none()
        );
        assert!(parse_command(r#"{"type":"mute","chat":"a@g.us","until":-1}"#, None).is_none());
    }

    #[test]
    fn validates_bounded_history_commands() {
        assert!(matches!(
            parse_command(r#"{"type":"load","chat":"fixture@lid"}"#, None),
            Some(Command::LoadWindow { before: None, .. })
        ));
        assert!(matches!(
            parse_command(
                r#"{"type":"newer","chat":"fixture@lid","after":[10,"message"]}"#,
                None
            ),
            Some(Command::LoadNewer { .. })
        ));
        assert!(matches!(
            parse_command(
                r#"{"type":"around","chat":"fixture@lid","id":"message"}"#,
                None
            ),
            Some(Command::LoadWindowAround { .. })
        ));
        assert!(
            parse_command(
                r#"{"type":"newer","chat":"fixture@lid","after":[-1,"message"]}"#,
                None
            )
            .is_none()
        );
        assert!(parse_command(r#"{"type":"around","chat":"fixture@lid","id":""}"#, None).is_none());
    }

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
                parse_command(input, None).is_none(),
                "unexpected fixture accepted: {input}"
            );
        }
        assert!(
            matches!(parse_command(r#"{"type":"send","chat":"fixture@g.us","text":"Hello\n👋","quoting":"quoted"}"#, None), Some(Command::SendText { text, quoting: Some(id), .. }) if text == "Hello\n👋" && id == "quoted")
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
            parse_command(
                r#"{"type":"read","chat":"fixture@g.us","receipts":false}"#,
                None,
            )
            .unwrap(),
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
