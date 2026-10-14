//! Persist chat timers separately from display metadata, including explicit off.

use super::{Archive, Delivery, OptionalExtension, Result, params, status_rank};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisappearingTimer {
    /// Zero explicitly disables the timer; absence means not learned yet.
    pub seconds: u32,
    /// Setting timestamp in Unix seconds, or zero for an undated snapshot.
    pub updated_at: i64,
}

impl Archive {
    pub fn chat_timer(&self, chat: &str) -> Result<Option<DisappearingTimer>> {
        self.connection
            .query_row(
                "SELECT seconds, updated_at FROM chat_timers WHERE chat = ?1",
                [chat],
                |row| {
                    Ok(DisappearingTimer {
                        seconds: row.get(0)?,
                        updated_at: row.get(1)?,
                    })
                },
            )
            .optional()
    }

    /// History can arrive after a live timer change. Never roll it backwards.
    pub fn set_chat_timer(&self, chat: &str, timer: DisappearingTimer) -> Result<()> {
        self.connection.execute(
            "INSERT INTO chat_timers (chat, seconds, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(chat) DO UPDATE SET seconds = excluded.seconds,
                 updated_at = excluded.updated_at
             WHERE excluded.updated_at >= chat_timers.updated_at",
            params![chat, timer.seconds, timer.updated_at],
        )?;
        Ok(())
    }

    /// Upgrade fallback for a direct chat whose timer older builds discarded.
    /// Only the latest non-forwarded received body, capped at 256 KiB. Returning
    /// no body for a large/missing source prevents reaching past it to stale data.
    pub fn latest_timer_source(&self, chat: &str) -> Result<Option<(i64, Option<Vec<u8>>)>> {
        self.connection
            .query_row(
                "SELECT timestamp, CASE WHEN length(raw) <= 262144 THEN raw END
                 FROM messages WHERE chat = ?1 AND from_me = 0 AND forwarded = 0
                 ORDER BY timestamp DESC, rowid DESC LIMIT 1",
                [chat],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
    }

    /// A send cannot leave the worker until its final body is durable for retry.
    pub fn prepare_send_raw(&self, chat: &str, id: &str, raw: &[u8]) -> Result<bool> {
        Ok(self.connection.execute(
            "UPDATE messages SET raw = ?3 WHERE chat = ?1 AND id = ?2
                 AND from_me = 1 AND status = ?4",
            params![chat, id, raw, status_rank(Delivery::Pending)],
        )? == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timers_survive_reopening_disable_replays_and_identity_merges() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("archive.db");
        let lid = "123@lid";
        let pn = "15550001111@s.whatsapp.net";
        let enabled = DisappearingTimer {
            seconds: 86400,
            updated_at: 100,
        };
        let disabled = DisappearingTimer {
            seconds: 0,
            updated_at: 200,
        };
        {
            let archive = Archive::open(&path).unwrap();
            archive.set_chat_timer(lid, enabled).unwrap();
            archive.set_chat_timer(pn, disabled).unwrap();
            archive.put_lid("123", "15550001111").unwrap();
            assert!(archive.chat_timer(lid).unwrap().is_none());
        }
        let archive = Archive::open(&path).unwrap();
        assert_eq!(archive.chat_timer(pn).unwrap(), Some(disabled));
        archive.set_chat_timer(pn, enabled).unwrap();
        assert_eq!(archive.chat_timer(pn).unwrap(), Some(disabled));
        archive.clear().unwrap();
        assert!(archive.chat_timer(pn).unwrap().is_none());
    }
}
