//! Reconcile phone and privacy ids without discarding either conversation.

use super::{Archive, Result, params};

impl Archive {
    /// Older builds remembered mappings but left existing rows under the alias.
    /// Only visit identities with stale rows, keeping normal startup inexpensive.
    pub fn pending_lid_merges(&self) -> Result<Vec<(String, String)>> {
        self.connection
            .prepare(
                "SELECT lid, pn FROM lids WHERE
                 EXISTS (SELECT 1 FROM chats WHERE id = lid || '@lid') OR
                 EXISTS (SELECT 1 FROM contacts WHERE id = lid || '@lid') OR
                 EXISTS (SELECT 1 FROM messages WHERE chat = lid || '@lid')",
            )?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect()
    }

    /// Called only for a protocol-provided mapping, inside put_lid's transaction.
    /// Move unique rows in place so their order, raw keys and search rowids survive.
    pub(super) fn merge_chat_identity(&self, from: &str, into: &str) -> Result<()> {
        let had_chat: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM chats WHERE id = ?1)",
            [from],
            |row| row.get(0),
        )?;
        self.connection.execute(
            "INSERT INTO chats (id, name, kind, last_activity, unread, archived, pinned,
                 muted_until, participants, read_only, read_through, pending_read)
             SELECT ?2, name, kind, last_activity, unread, archived, pinned,
                 muted_until, participants, read_only, read_through, pending_read
             FROM chats WHERE id = ?1
             ON CONFLICT(id) DO UPDATE SET
                 last_activity = MAX(last_activity, excluded.last_activity),
                 unread = MAX(unread, excluded.unread),
                 archived = MIN(archived, excluded.archived),
                 pinned = MAX(pinned, excluded.pinned),
                 muted_until = CASE WHEN muted_until = 0 OR excluded.muted_until = 0 THEN 0
                     ELSE COALESCE(MAX(muted_until, excluded.muted_until), muted_until, excluded.muted_until) END,
                 read_through = COALESCE(MAX(read_through, excluded.read_through), read_through, excluded.read_through),
                 pending_read = COALESCE(MAX(pending_read, excluded.pending_read), pending_read, excluded.pending_read)",
            params![from, into],
        )?;
        self.connection.execute(
            "INSERT OR IGNORE INTO chats (id, name, kind)
             SELECT ?2, ?2, 'direct' WHERE EXISTS (SELECT 1 FROM messages WHERE chat = ?1)",
            params![from, into],
        )?;
        self.connection.execute(
            "UPDATE messages SET chat = ?2 WHERE chat = ?1
             AND NOT EXISTS (SELECT 1 FROM messages destination
                 WHERE destination.chat = ?2 AND destination.id = messages.id)",
            params![from, into],
        )?;
        // Remaining rows are the same message replayed under both identities.
        // Keep edits/revocations, downloaded files, attachment keys and receipts.
        // An edited replay may not carry the original copy's local media path.
        self.connection.execute(
            "UPDATE messages SET content = json_set(content, '$.media.path',
                 (SELECT json_extract(destination.content, '$.media.path') FROM messages destination
                  WHERE destination.chat = ?2 AND destination.id = messages.id))
             WHERE chat = ?1 AND json_valid(content)
                 AND json_extract(content, '$.media') IS NOT NULL
                 AND json_extract(content, '$.media.path') IS NULL
                 AND EXISTS (SELECT 1 FROM messages destination
                     WHERE destination.chat = ?2 AND destination.id = messages.id
                     AND json_valid(destination.content)
                     AND json_extract(destination.content, '$.media.path') IS NOT NULL)",
            params![from, into],
        )?;
        self.connection.execute(
            "INSERT INTO messages (chat, id, sender, sender_name, from_me, timestamp, content,
                 status, quoted, reactions, edited, raw, thumbnail, mentions, forwarded, delivered_at, read_at)
             SELECT ?2, id, sender, sender_name, from_me, timestamp, content,
                 status, quoted, reactions, edited, raw, thumbnail, mentions, forwarded, delivered_at, read_at
             FROM messages WHERE chat = ?1
             ON CONFLICT(chat, id) DO UPDATE SET
                 sender_name = COALESCE(messages.sender_name, excluded.sender_name),
                 content = CASE WHEN json_valid(messages.content) AND json_valid(excluded.content) THEN
                     CASE WHEN json_extract(messages.content, '$.kind') = 'revoked' THEN messages.content
                          WHEN json_extract(excluded.content, '$.kind') = 'revoked'
                              OR json_extract(messages.content, '$.kind') = 'unsupported'
                              OR excluded.edited > messages.edited THEN excluded.content
                          WHEN json_extract(messages.content, '$.media') IS NOT NULL
                              AND json_extract(messages.content, '$.media.path') IS NULL
                              AND json_extract(excluded.content, '$.media.path') IS NOT NULL
                          THEN json_set(messages.content, '$.media.path', json_extract(excluded.content, '$.media.path'))
                          ELSE messages.content END
                     ELSE messages.content END,
                 status = CASE WHEN messages.status BETWEEN 2 AND 5 OR excluded.status BETWEEN 2 AND 5
                     THEN MAX(CASE WHEN messages.status = 6 THEN 0 ELSE messages.status END,
                              CASE WHEN excluded.status = 6 THEN 0 ELSE excluded.status END)
                     ELSE MAX(messages.status, excluded.status) END,
                 quoted = COALESCE(messages.quoted, excluded.quoted),
                 reactions = (SELECT json_group_array(json(value)) FROM (
                     SELECT value FROM json_each(messages.reactions)
                     UNION ALL SELECT value FROM json_each(excluded.reactions) source
                     WHERE NOT EXISTS (SELECT 1 FROM json_each(messages.reactions) destination
                         WHERE json_extract(destination.value, '$.sender') = json_extract(source.value, '$.sender')))),
                 edited = MAX(messages.edited, excluded.edited),
                 raw = CASE WHEN excluded.edited > messages.edited
                     THEN COALESCE(excluded.raw, messages.raw) ELSE COALESCE(messages.raw, excluded.raw) END,
                 thumbnail = COALESCE(messages.thumbnail, excluded.thumbnail),
                 mentions = CASE WHEN messages.mentions = '[]' THEN excluded.mentions ELSE messages.mentions END,
                 forwarded = MAX(messages.forwarded, excluded.forwarded),
                 delivered_at = COALESCE(MIN(messages.delivered_at, excluded.delivered_at), messages.delivered_at, excluded.delivered_at),
                 read_at = COALESCE(MIN(messages.read_at, excluded.read_at), messages.read_at, excluded.read_at)",
            params![from, into],
        )?;
        self.connection
            .execute("DELETE FROM messages WHERE chat = ?1", [from])?;
        self.connection
            .execute("DELETE FROM chats WHERE id = ?1", [from])?;
        self.connection.execute(
            "UPDATE messages SET sender = ?2 WHERE sender = ?1",
            params![from, into],
        )?;
        // A read position from either identity covers the merged conversation.
        self.connection.execute(
            "UPDATE chats SET last_activity = MAX(last_activity,
                 COALESCE((SELECT MAX(timestamp) FROM messages WHERE chat = ?1), 0)) WHERE id = ?1",
            [into],
        )?;
        if had_chat {
            self.connection.execute(
                "UPDATE chats SET unread = MIN(unread, (SELECT COUNT(*) FROM messages
                 WHERE chat = ?1 AND from_me = 0 AND timestamp > read_through))
             WHERE id = ?1 AND read_through IS NOT NULL",
                [into],
            )?;
        }
        self.connection.execute(
            "INSERT INTO contacts (id, full_name, push_name)
             SELECT ?2, full_name, push_name FROM contacts WHERE id = ?1
             ON CONFLICT(id) DO UPDATE SET
                 full_name = COALESCE(NULLIF(contacts.full_name, ''), excluded.full_name),
                 push_name = COALESCE(NULLIF(contacts.push_name, ''), excluded.push_name)",
            params![from, into],
        )?;
        self.connection
            .execute("DELETE FROM contacts WHERE id = ?1", [from])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::tests::message;
    use crate::model::{Contact, Content, Delivery, Media, MediaState, Reaction};

    const LID: &str = "123456@lid";
    const PN: &str = "15550002222@s.whatsapp.net";

    #[test]
    fn late_mapping_unites_history_contacts_keys_downloads_and_search() {
        let archive = Archive::in_memory().unwrap();
        archive.ensure_chat(LID, "~Peer").unwrap();
        archive.ensure_chat(PN, "Phone").unwrap();
        archive
            .ensure_chat("other@s.whatsapp.net", "Other")
            .unwrap();
        archive
            .upsert_contact(&Contact {
                id: LID.into(),
                full_name: None,
                push_name: Some("Peer".into()),
            })
            .unwrap();
        archive
            .upsert_contact(&Contact {
                id: PN.into(),
                full_name: Some("Saved name".into()),
                push_name: None,
            })
            .unwrap();
        let first = message(LID, "first", 10, false);
        archive
            .insert_message(&first, Some(b"first attachment keys"))
            .unwrap();
        archive
            .insert_message(
                &message(PN, "second", 20, true),
                Some(b"second attachment keys"),
            )
            .unwrap();
        archive
            .insert_message(&message("other@s.whatsapp.net", "first", 15, false), None)
            .unwrap();
        let mut duplicate = message(LID, "shared", 30, true);
        duplicate.content = Content::Image {
            caption: Some("searchable photograph".into()),
            media: Media {
                mime: "image/jpeg".into(),
                size: 10,
                width: None,
                height: None,
                path: Some("/tmp/synthetic-photo.jpg".into()),
                state: MediaState::Idle,
            },
        };
        duplicate.status = Delivery::Read;
        duplicate.delivered_at = Some(31);
        duplicate.read_at = Some(32);
        duplicate.reactions.push(Reaction {
            sender: "one@s.whatsapp.net".into(),
            from_me: false,
            emoji: "👍".into(),
        });
        archive
            .insert_message(&duplicate, Some(b"shared attachment keys"))
            .unwrap();
        duplicate.chat = PN.into();
        duplicate.content.media_mut().unwrap().path = None;
        duplicate.status = Delivery::Failed;
        duplicate.delivered_at = None;
        duplicate.read_at = None;
        duplicate.reactions[0].sender = "two@s.whatsapp.net".into();
        archive.insert_message(&duplicate, None).unwrap();
        archive.set_pinned(LID, true).unwrap();
        archive.set_muted(PN, Some(0)).unwrap();
        archive.put_lid("123456", "15550002222").unwrap();
        assert!(archive.chat(LID).unwrap().is_none());
        assert!(archive.messages(LID, None, 100).unwrap().is_empty());
        assert_eq!(archive.messages(PN, None, 100).unwrap().len(), 3);
        assert!(archive.chat(PN).unwrap().unwrap().pinned);
        assert_eq!(archive.chat(PN).unwrap().unwrap().muted_until, Some(0));
        let row = archive.message(PN, "shared").unwrap().unwrap();
        assert_eq!(row.status, Delivery::Read);
        assert_eq!((row.delivered_at, row.read_at), (Some(31), Some(32)));
        assert_eq!(
            row.content.media().unwrap().path.as_deref(),
            Some(std::path::Path::new("/tmp/synthetic-photo.jpg"))
        );
        assert_eq!(row.reactions.len(), 2);
        assert_eq!(
            archive.raw(PN, "first").unwrap().unwrap(),
            b"first attachment keys"
        );
        assert_eq!(
            archive.raw(PN, "shared").unwrap().unwrap(),
            b"shared attachment keys"
        );
        assert_eq!(archive.message(PN, "first").unwrap().unwrap().sender, PN);
        let contact = archive.contact(PN).unwrap().unwrap();
        assert_eq!(contact.full_name.as_deref(), Some("Saved name"));
        assert_eq!(contact.push_name.as_deref(), Some("Peer"));
        assert!(archive.contact(LID).unwrap().is_none());
        assert_eq!(archive.search_messages("searchable", 10).unwrap().len(), 1);
        assert_eq!(
            archive.search_messages("searchable", 10).unwrap()[0].chat,
            PN
        );
        assert!(
            archive
                .message("other@s.whatsapp.net", "first")
                .unwrap()
                .is_some()
        );
        // Reapplying the same mapping cannot erase, duplicate or regress rows.
        archive.put_lid("123456", "15550002222").unwrap();
        assert_eq!(archive.messages(PN, None, 100).unwrap().len(), 3);
        assert!(archive.pending_lid_merges().unwrap().is_empty());
    }

    #[test]
    fn identity_merge_is_atomic_if_a_write_fails() {
        let archive = Archive::in_memory().unwrap();
        for chat in [LID, PN] {
            archive.ensure_chat(chat, "Peer").unwrap();
            archive
                .insert_message(&message(chat, chat, 10, false), Some(b"keys"))
                .unwrap();
        }
        archive.connection.execute_batch("CREATE TRIGGER reject_merge BEFORE DELETE ON chats BEGIN SELECT RAISE(ABORT, 'injected failure'); END;").unwrap();
        assert!(archive.put_lid("123456", "15550002222").is_err());
        assert!(archive.lids().unwrap().is_empty());
        for chat in [LID, PN] {
            assert!(archive.chat(chat).unwrap().is_some());
            assert_eq!(archive.messages(chat, None, 10).unwrap().len(), 1);
            assert_eq!(archive.raw(chat, chat).unwrap().unwrap(), b"keys");
        }
    }

    #[test]
    fn merge_keeps_edits_revocations_and_the_furthest_read_position() {
        let archive = Archive::in_memory().unwrap();
        for chat in [LID, PN] {
            archive.ensure_chat(chat, "Peer").unwrap();
            for id in ["edited", "revoked"] {
                archive
                    .insert_message(&message(chat, id, 10, false), None)
                    .unwrap();
            }
        }
        archive
            .set_content(LID, "edited", &Content::text("corrected wording"), true)
            .unwrap();
        archive
            .set_content(PN, "revoked", &Content::Revoked, false)
            .unwrap();
        archive.mark_read_through(LID, 10).unwrap();
        archive.put_lid("123456", "15550002222").unwrap();
        assert_eq!(
            archive.message(PN, "edited").unwrap().unwrap().content,
            Content::text("corrected wording")
        );
        assert_eq!(
            archive.message(PN, "revoked").unwrap().unwrap().content,
            Content::Revoked
        );
        assert_eq!(archive.read_through(PN).unwrap(), Some(10));
        assert_eq!(archive.chat(PN).unwrap().unwrap().unread, 0);
        assert!(
            archive
                .search_messages("corrected", 10)
                .unwrap()
                .iter()
                .all(|row| row.chat == PN)
        );
    }
}
