//! Bound reloadable inactive history without discarding phone-history state.

use std::collections::HashSet;
use std::mem::size_of;
use std::path::Path;

use super::{App, Conversation, Pending};
use crate::model::{Content, Dialog, Media, MediaState, MentionRef, Message};

const INACTIVE_CHATS: usize = 8;
const INACTIVE_BYTES: usize = 32 * 1024 * 1024;

impl App {
    pub(super) fn trim_conversations(&mut self, ctx: &egui::Context) {
        if !std::mem::take(&mut self.conversations_dirty) {
            return;
        }
        let dialog_chat = match &self.dialog {
            Some(Dialog::Forward { chat, .. }) | Some(Dialog::ChatInfo(chat)) => {
                Some(chat.as_str())
            }
            _ => None,
        };
        let mut candidates = Vec::new();
        let mut total = 0;
        for (id, conversation) in &mut self.conversations {
            if self.open_chat.as_deref() == Some(id.as_str())
                || dialog_chat == Some(id.as_str())
                || conversation.busy()
                || (!conversation.requested && conversation.messages.is_empty())
            {
                continue;
            }
            let bytes = conversation.estimated_bytes();
            total += bytes;
            candidates.push((conversation.last_viewed, id.clone(), bytes));
        }
        let mut count = candidates.len();
        if count <= INACTIVE_CHATS && total <= INACTIVE_BYTES {
            return;
        }
        candidates.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        let mut evicted = Vec::new();
        for (_, id, bytes) in candidates {
            if count <= INACTIVE_CHATS && total <= INACTIVE_BYTES {
                break;
            }
            let conversation = self.conversations.get_mut(&id).expect("candidate exists");
            let messages = std::mem::take(&mut conversation.messages);
            conversation.row_heights = Default::default();
            conversation.cached_bytes = None;
            conversation.cached_busy = None;
            conversation.requested = false;
            conversation.complete = false;
            // Keep phone_exhausted, cooldown and empty-response backoff. Clearing
            // those would ask the phone again just because RAM was reclaimed.
            evicted.push((id, messages));
            count -= 1;
            total = total.saturating_sub(bytes);
        }

        // Do not invalidate a shared file still used by another chat or picker.
        let mut retained_paths: HashSet<&Path> = self
            .conversations
            .values()
            .flat_map(|conversation| &conversation.messages)
            .filter_map(|message| message.content.media()?.path.as_deref())
            .collect();
        retained_paths.extend(self.pending.iter().filter_map(|pending| match pending {
            Pending::File(path) => Some(path.as_path()),
            Pending::Picture { .. } => None,
        }));
        retained_paths.extend(
            self.gif_results
                .iter()
                .filter_map(|gif| gif.still.as_deref()),
        );
        retained_paths.extend(
            self.stickers
                .iter()
                .chain(&self.stickers_saved)
                .map(|path| path.as_path()),
        );
        retained_paths.extend(
            self.sticker_packs
                .iter()
                .flat_map(|pack| &pack.stickers)
                .map(|path| path.as_path()),
        );
        for (chat, messages) in evicted {
            crate::ui::conversation::forget_cached_messages(ctx, &chat, &messages, &retained_paths);
        }
    }
}

impl Conversation {
    fn busy(&mut self) -> bool {
        if self.loading_initial || self.loading_older || self.fetching_phone {
            return true;
        }
        *self.cached_busy.get_or_insert_with(|| {
            self.messages.iter().any(|message| {
                message.status.in_flight()
                    || message
                        .content
                        .media()
                        .is_some_and(|media| matches!(media.state, MediaState::Downloading))
            })
        })
    }

    fn estimated_bytes(&mut self) -> usize {
        if let Some(bytes) = self.cached_bytes {
            return bytes;
        }
        let bytes = self.messages.capacity() * size_of::<Message>()
            + self.messages.iter().map(message_heap_bytes).sum::<usize>()
            + self.row_heights.estimated_bytes();
        self.cached_bytes = Some(bytes);
        bytes
    }
}

fn optional_text(text: &Option<String>) -> usize {
    text.as_ref().map_or(0, String::capacity)
}

fn mentions_bytes(mentions: &Vec<MentionRef>) -> usize {
    mentions.capacity() * size_of::<MentionRef>()
        + mentions
            .iter()
            .map(|mention| mention.user.capacity() + mention.id.capacity())
            .sum::<usize>()
}

fn media_bytes(media: &Media) -> usize {
    media.mime.capacity()
        + media.path.as_ref().map_or(0, |path| path.capacity())
        + match &media.state {
            MediaState::Failed(error) => error.capacity(),
            _ => 0,
        }
}

/// Owned allocations, including capacities and nested content. Allocator,
/// hash-table and egui/codec/GPU overhead are outside this estimated budget.
fn message_heap_bytes(message: &Message) -> usize {
    let content = match &message.content {
        Content::Text { text, preview } => {
            text.capacity()
                + preview.as_ref().map_or(0, |preview| {
                    preview.url.capacity()
                        + optional_text(&preview.title)
                        + optional_text(&preview.description)
                })
        }
        Content::Image { caption, media } | Content::Video { caption, media, .. } => {
            optional_text(caption) + media_bytes(media)
        }
        Content::Audio {
            media, waveform, ..
        } => media_bytes(media) + waveform.capacity(),
        Content::Document {
            media,
            file_name,
            caption,
            ..
        } => media_bytes(media) + file_name.capacity() + optional_text(caption),
        Content::Sticker { media, .. } => media_bytes(media),
        Content::Location { name, address, .. } => optional_text(name) + optional_text(address),
        Content::Contact {
            display_name,
            vcard,
        } => display_name.capacity() + vcard.capacity(),
        Content::Poll { question, options } => {
            question.capacity()
                + options.capacity() * size_of::<String>()
                + options.iter().map(String::capacity).sum::<usize>()
        }
        Content::Revoked => 0,
        Content::Unsupported { what } => what.capacity(),
    };
    message.id.capacity()
        + message.chat.capacity()
        + message.sender.capacity()
        + optional_text(&message.sender_name)
        + content
        + message.thumbnail.as_ref().map_or(0, Vec::capacity)
        + mentions_bytes(&message.mentions)
        + message.reactions.capacity() * size_of::<crate::model::Reaction>()
        + message
            .reactions
            .iter()
            .map(|reaction| reaction.sender.capacity() + reaction.emoji.capacity())
            .sum::<usize>()
        + message.quoted.as_ref().map_or(0, |quoted| {
            quoted.id.capacity()
                + quoted.sender.capacity()
                + optional_text(&quoted.sender_name)
                + quoted.summary.capacity()
                + mentions_bytes(&quoted.mentions)
        })
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::archive::Archive;
    use crate::backend::{Command, Event, LinkStatus};
    use crate::model::{Chat, Delivery};
    use crate::paths::AppDirs;
    use crate::settings::Settings;

    fn app() -> (App, std::sync::mpsc::Sender<Event>) {
        let root = std::env::temp_dir().join(format!(
            "zapfast-cache-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let (mut app, events) = App::headless(AppDirs::under(&root), Settings::default());
        app.backend.record_demo_commands();
        app.link = LinkStatus::Connected;
        (app, events)
    }

    fn message(chat: &str, id: &str) -> Message {
        Message {
            id: id.into(),
            chat: chat.into(),
            sender: "synthetic@lid".into(),
            sender_name: None,
            from_me: false,
            timestamp: 100,
            content: Content::text("cached text"),
            status: Delivery::None,
            delivered_at: None,
            read_at: None,
            quoted: None,
            reactions: Vec::new(),
            edited: false,
            mentions: Vec::new(),
            forwarded: false,
            thumbnail: None,
        }
    }

    fn loaded(app: &mut App, chat: &str, age: u64) {
        app.chats.push(Chat::new(chat.into(), chat.into()));
        app.conversations.insert(
            chat.into(),
            Conversation {
                messages: vec![message(chat, "m")],
                requested: true,
                complete: true,
                last_viewed: Some(Instant::now() - Duration::from_secs(age)),
                ..Default::default()
            },
        );
        app.conversations_dirty = true;
    }

    fn fill(app: &mut App) {
        for index in 0..INACTIVE_CHATS {
            loaded(app, &format!("recent-{index}"), 1);
        }
    }

    #[test]
    fn oldest_inactive_history_is_evicted_but_phone_state_and_drafts_survive() {
        let (mut app, _) = app();
        let ctx = egui::Context::default();
        loaded(&mut app, "old", 100);
        app.open_chat("old".into());
        app.composer = "unfinished draft".into();
        fill(&mut app);
        app.open_chat("recent-0".into());
        let answered = Instant::now();
        let old = app.conversations.get_mut("old").unwrap();
        old.last_viewed = Some(answered - Duration::from_secs(100));
        old.phone_exhausted = true;
        old.phone_misses = 5;
        old.phone_answered = Some(answered);
        old.row_heights.set("m", None, 100.0);
        loaded(&mut app, "newest", 0);
        app.trim_conversations(&ctx);
        let old = &app.conversations["old"];
        assert!(old.messages.is_empty());
        assert_eq!(old.messages.capacity(), 0, "release the backing allocation");
        assert_eq!(old.row_heights.estimated_bytes(), 0);
        assert!(!old.requested);
        assert!(old.phone_exhausted);
        assert_eq!(old.phone_misses, 5);
        assert_eq!(old.phone_answered, Some(answered));
        assert_eq!(app.drafts["old"], "unfinished draft");
        assert_eq!(
            app.conversations
                .values()
                .filter(|chat| !chat.messages.is_empty())
                .count(),
            INACTIVE_CHATS + 1
        );
        app.open_chat("old".into());
        assert_eq!(app.composer, "unfinished draft");
        app.backend.take_demo_commands();
        app.fetch_older("old");
        assert!(
            app.backend.take_demo_commands().is_empty(),
            "eviction must not reset phone exhaustion"
        );
    }

    #[test]
    fn byte_budget_recounts_a_growing_thumbnail_and_protects_the_open_chat() {
        let (mut app, _) = app();
        let ctx = egui::Context::default();
        loaded(&mut app, "large", 10);
        app.trim_conversations(&ctx);
        assert!(app.conversations["large"].cached_bytes.is_some());
        app.conversations
            .get_mut("large")
            .unwrap()
            .message_mut("m")
            .unwrap()
            .thumbnail = Some(vec![0; INACTIVE_BYTES + 1]);
        app.open_chat("large".into());
        app.trim_conversations(&ctx);
        assert_eq!(
            app.conversations["large"].messages.len(),
            1,
            "current history is never truncated"
        );
        loaded(&mut app, "other", 0);
        app.open_chat("other".into());
        app.trim_conversations(&ctx);
        assert!(
            app.conversations["large"].messages.is_empty(),
            "byte limit works even below the chat-count limit"
        );
    }

    #[test]
    fn active_loads_sends_and_downloads_are_protected_then_become_evictable() {
        let (mut app, _) = app();
        let ctx = egui::Context::default();
        for chat in ["initial", "older", "phone", "send", "download"] {
            loaded(&mut app, chat, 100);
        }
        app.conversations
            .get_mut("initial")
            .unwrap()
            .loading_initial = true;
        app.conversations.get_mut("older").unwrap().loading_older = true;
        app.conversations.get_mut("phone").unwrap().fetching_phone = true;
        app.conversations
            .get_mut("send")
            .unwrap()
            .message_mut("m")
            .unwrap()
            .status = Delivery::Pending;
        app.conversations
            .get_mut("download")
            .unwrap()
            .message_mut("m")
            .unwrap()
            .content = Content::Image {
            caption: None,
            media: Media {
                mime: "image/jpeg".into(),
                size: 20,
                width: None,
                height: None,
                path: None,
                state: MediaState::Downloading,
            },
        };
        fill(&mut app);
        app.trim_conversations(&ctx);
        for chat in ["initial", "older", "phone", "send", "download"] {
            assert!(
                !app.conversations[chat].messages.is_empty(),
                "{chat} is protected"
            );
        }
        app.conversations
            .get_mut("initial")
            .unwrap()
            .loading_initial = false;
        app.conversations.get_mut("older").unwrap().loading_older = false;
        app.conversations.get_mut("phone").unwrap().fetching_phone = false;
        app.conversations
            .get_mut("send")
            .unwrap()
            .message_mut("m")
            .unwrap()
            .status = Delivery::Sent;
        app.conversations
            .get_mut("download")
            .unwrap()
            .message_mut("m")
            .unwrap()
            .content
            .media_mut()
            .unwrap()
            .state = MediaState::Idle;
        app.conversations_dirty = true;
        app.trim_conversations(&ctx);
        for chat in ["initial", "older", "phone", "send", "download"] {
            assert!(
                app.conversations[chat].messages.is_empty(),
                "{chat} can be reclaimed after completion"
            );
        }
    }

    #[test]
    fn live_updates_do_not_finish_a_pending_page_or_populate_unopened_chats() {
        let (mut app, events) = app();
        let ctx = egui::Context::default();
        app.open_chat("loading".into());
        for chat in ["loading", "unopened"] {
            events
                .send(Event::Messages {
                    chat: chat.into(),
                    messages: vec![message(chat, "live")],
                    older: false,
                    complete: false,
                    requested: false,
                })
                .unwrap();
        }
        app.handle_events();
        assert!(app.conversations["loading"].loading_initial);
        assert!(!app.conversations.contains_key("unopened"));
        fill(&mut app);
        app.open_chat("recent-0".into());
        app.conversations.get_mut("loading").unwrap().last_viewed = None;
        loaded(&mut app, "extra", 0);
        app.trim_conversations(&ctx);
        assert!(!app.conversations["loading"].messages.is_empty());
        events
            .send(Event::Messages {
                chat: "loading".into(),
                messages: vec![message("loading", "stored")],
                older: false,
                complete: true,
                requested: true,
            })
            .unwrap();
        app.handle_events();
        assert!(!app.conversations["loading"].loading_initial);
        assert!(
            app.conversations["loading"].complete,
            "the page's completeness wins over the earlier live update"
        );
        app.trim_conversations(&ctx);
        assert!(app.conversations["loading"].messages.is_empty());
    }

    #[test]
    fn reopening_an_evicted_chat_reads_current_sqlite_state_and_preserves_raw_data() {
        let (mut app, events) = app();
        let archive = Archive::in_memory().unwrap();
        let ctx = egui::Context::default();
        archive.ensure_chat("old", "Old").unwrap();
        let original = message("old", "m");
        archive.insert_message(&original, Some(&[1, 2, 3])).unwrap();
        archive
            .insert_message(&message("old", "deleted"), None)
            .unwrap();
        loaded(&mut app, "old", 100);
        fill(&mut app);
        app.trim_conversations(&ctx);
        assert!(app.conversations["old"].messages.is_empty());
        archive
            .set_content("old", "m", &Content::text("edited while uncached"), true)
            .unwrap();
        archive.delete_message("old", "deleted").unwrap();
        archive
            .insert_message(&message("old", "new"), None)
            .unwrap();
        events
            .send(Event::MessageUpdated(Box::new(
                archive.message("old", "m").unwrap().unwrap(),
            )))
            .unwrap();
        events
            .send(Event::Messages {
                chat: "old".into(),
                messages: vec![message("old", "new")],
                older: false,
                complete: false,
                requested: false,
            })
            .unwrap();
        events
            .send(Event::MessageDeleted {
                chat: "old".into(),
                id: "deleted".into(),
            })
            .unwrap();
        app.handle_events();
        assert!(
            app.conversations["old"].messages.is_empty(),
            "background updates must not refill the cache"
        );
        app.open_chat("old".into());
        assert!(app.backend.take_demo_commands().iter().any(
            |command| matches!(command, Command::LoadChat { chat, before: None } if chat == "old")
        ));
        let current = archive.messages("old", None, super::super::PAGE).unwrap();
        events
            .send(Event::Messages {
                chat: "old".into(),
                messages: current.clone(),
                older: false,
                complete: true,
                requested: true,
            })
            .unwrap();
        app.handle_events();
        assert_eq!(app.conversations["old"].messages, current);
        assert_eq!(archive.raw("old", "m").unwrap(), Some(vec![1, 2, 3]));
    }

    #[test]
    fn failed_local_queries_release_loading_state_and_retry_on_reopen_or_reconnect() {
        let (mut app, events) = app();
        app.open_chat("chat".into());
        app.backend.take_demo_commands();
        events
            .send(Event::ChatLoadFailed {
                chat: "chat".into(),
                initial: true,
                error: "synthetic read failure".into(),
            })
            .unwrap();
        app.handle_events();
        assert!(!app.conversations["chat"].loading_initial);
        assert!(!app.conversations["chat"].requested);
        app.handle_link(LinkStatus::Disconnected {
            reason: "synthetic disconnect".into(),
        });
        app.handle_link(LinkStatus::Connected);
        assert!(app.conversations["chat"].loading_initial);
        assert!(app.backend.take_demo_commands().iter().any(
            |command| matches!(command, Command::LoadChat { chat, before: None } if chat == "chat")
        ));
        app.conversations.get_mut("chat").unwrap().loading_older = true;
        events
            .send(Event::ChatLoadFailed {
                chat: "chat".into(),
                initial: false,
                error: "synthetic older-page failure".into(),
            })
            .unwrap();
        app.handle_events();
        assert!(!app.conversations["chat"].loading_older);
    }
}
