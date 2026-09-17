//! Keep an open conversation and unfinished work attached to its canonical id.

use super::*;

fn append_draft(destination: &mut String, source: String) {
    if source.is_empty() || *destination == source {
        return;
    }
    if !destination.is_empty() {
        destination.push('\n');
    }
    destination.push_str(&source);
}

fn remap_message(message: &mut Message, from: &str, into: &str) {
    let replace = |id: &mut String| {
        if id == from {
            *id = into.to_owned();
        }
    };
    replace(&mut message.chat);
    replace(&mut message.sender);
    for reaction in &mut message.reactions {
        replace(&mut reaction.sender);
    }
    for mention in &mut message.mentions {
        replace(&mut mention.id);
    }
    if let Some(quoted) = &mut message.quoted {
        replace(&mut quoted.sender);
        for mention in &mut quoted.mentions {
            replace(&mut mention.id);
        }
    }
}

impl App {
    pub(super) fn merge_chat_identity(&mut self, from: ChatId, into: ChatId) {
        if self.chat_aliases.get(&from) == Some(&into) {
            return;
        }
        self.chat_aliases.insert(from.clone(), into.clone());
        if let Some(index) = self.chats.iter().position(|chat| chat.id == from) {
            let mut source = self.chats.remove(index);
            if self.chat(&into).is_none() {
                source.id = into.clone();
                self.chats.push(source);
            }
        }
        self.contacts.remove(&from);
        self.notifications.clear(&from);
        if let Some(mut source) = self.conversations.remove(&from) {
            for message in &mut source.messages {
                remap_message(message, &from, &into);
            }
            let destination = self.conversations.entry(into.clone()).or_default();
            destination.fetching_phone |= source.fetching_phone;
            destination.merge(source.messages, true);
        }
        for conversation in self.conversations.values_mut() {
            for message in &mut conversation.messages {
                remap_message(message, &from, &into);
            }
        }
        // Reload from the committed union. Keep visible/pending rows until it
        // arrives, but never reuse an alias's exhausted local page boundary.
        if let Some(conversation) = self.conversations.get_mut(&into) {
            conversation.complete = false;
            conversation.requested = false;
            conversation.loading_initial = false;
            conversation.phone_exhausted = false;
        }
        for message in &mut self.search_hits {
            remap_message(message, &from, &into);
        }
        if let Some(draft) = self.drafts.remove(&from) {
            append_draft(self.drafts.entry(into.clone()).or_default(), draft);
        }
        if let Some(mentions) = self.draft_mentions.remove(&from) {
            self.draft_mentions
                .entry(into.clone())
                .or_default()
                .extend(mentions);
        }
        let is_open = self
            .open_chat
            .as_deref()
            .is_some_and(|id| id == from || id == into);
        if is_open {
            self.open_chat = Some(into.clone());
            if self.editing.is_none() {
                if let Some(draft) = self.drafts.remove(&into) {
                    append_draft(&mut self.composer, draft);
                }
                if let Some(mentions) = self.draft_mentions.remove(&into) {
                    self.composer_mentions.extend(mentions);
                }
            }
            self.ensure_loaded(&into);
        }
        if self.scroll_chat_into_view.as_deref() == Some(&from) {
            self.scroll_chat_into_view = Some(into.clone());
        }
        if self.settings.last_chat.as_deref() == Some(&from) {
            self.settings.last_chat = Some(into.clone());
            self.mark_settings_dirty();
        }
        match &mut self.dialog {
            Some(Dialog::ChatInfo(id) | Dialog::Forward { chat: id, .. }) if *id == from => {
                *id = into.clone();
            }
            _ => {}
        }
        if let Some((id, _)) = &mut self.contact_edit
            && *id == from
        {
            *id = into.clone();
        }
        if let Some(typing) = self.typing.remove(&from) {
            self.typing.entry(into.clone()).or_default().extend(typing);
        }
        if let Some(presence) = self.presence.remove(&from) {
            self.presence.entry(into.clone()).or_insert(presence);
        }
        for avatars in [&mut self.avatars, &mut self.avatars_full] {
            if let Some(picture) = avatars.remove(&from) {
                avatars.entry(into.clone()).or_insert(picture);
            }
        }
        self.avatar_requests.remove(&from);
        self.avatar_full_requests.remove(&from);
        self.invalidate_message_layouts();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_keeps_the_open_composer_reply_downloads_and_both_drafts() {
        let dirs = AppDirs::under(&std::env::temp_dir().join("whatsapp-identity-ui"));
        let (mut app, events) = App::headless(dirs, Settings::default());
        let from = "123456@lid";
        let into = "15550002222@s.whatsapp.net";
        app.chats = vec![
            Chat::new(from.into(), "Unknown".into()),
            Chat::new(into.into(), "~Peer".into()),
        ];
        app.open_chat = Some(from.into());
        app.settings.last_chat = Some(from.into());
        app.composer = "unsent current draft".into();
        app.drafts.insert(into.into(), "unsent other draft".into());
        app.reply_to = Some("old".into());
        app.pending
            .push(Pending::File("/tmp/unsent-file.pdf".into()));
        app.dialog = Some(Dialog::ChatInfo(from.into()));
        let message = |chat: &str, id: &str, timestamp: i64| Message {
            chat: chat.into(),
            id: id.into(),
            sender: chat.into(),
            sender_name: None,
            from_me: false,
            timestamp,
            content: Content::text("synthetic message"),
            status: Delivery::None,
            delivered_at: None,
            read_at: None,
            quoted: None,
            reactions: Vec::new(),
            edited: false,
            mentions: Vec::new(),
            forwarded: false,
            thumbnail: None,
        };
        app.conversations
            .entry(from.into())
            .or_default()
            .merge(vec![message(from, "old", 10)], false);
        app.conversations
            .entry(into.into())
            .or_default()
            .merge(vec![message(into, "new", 20)], false);
        events
            .send(Event::ChatMerged {
                from: from.into(),
                into: into.into(),
            })
            .unwrap();
        app.handle_events();
        assert_eq!(app.chats.len(), 1);
        assert_eq!(app.open_chat.as_deref(), Some(into));
        assert_eq!(app.settings.last_chat.as_deref(), Some(into));
        assert_eq!(app.composer, "unsent current draft\nunsent other draft");
        assert_eq!(app.reply_to.as_deref(), Some("old"));
        assert_eq!(app.pending.len(), 1);
        assert!(matches!(app.dialog.as_ref(), Some(Dialog::ChatInfo(id)) if id == into));
        assert!(!app.conversations.contains_key(from));
        assert_eq!(app.conversations[into].messages.len(), 2);
        assert!(
            app.conversations[into]
                .messages
                .iter()
                .all(|row| row.chat == into && row.sender == into)
        );
        assert!(app.conversations[into].loading_initial);
        // Reopening an old notification stays on the same canonical chat.
        app.open_chat(from.into());
        assert_eq!(app.open_chat.as_deref(), Some(into));
        assert_eq!(app.composer, "unsent current draft\nunsent other draft");
    }
}
