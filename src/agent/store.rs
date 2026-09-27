//! Short-lived SQLite snapshots. Never call Archive::open from this module.

use std::time::{Duration, Instant};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, params, params_from_iter, types::Value as SqlValue,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{ChatQuery, Failure, MessageQuery, Result, bounded, content, identifier};
use crate::{
    model::{Content, MentionRef, Quoted, Reaction},
    paths::AppDirs,
};

const PAGE_BYTES: usize = 256 * 1024;
const MESSAGE_SELECT: &str = "SELECT m.chat, m.id, coalesce(l.pn, m.sender),
    CASE WHEN m.from_me THEN 'You' ELSE coalesce(nullif(n.full_name,''), nullif(m.sender_name,''), nullif(n.push_name,''), m.sender) END,
    m.from_me, m.timestamp,
    CASE WHEN length(CAST(m.content AS BLOB)) <= 1048576 THEN m.content END,
    CASE WHEN m.quoted IS NULL THEN NULL WHEN length(CAST(m.quoted AS BLOB)) <= 1048576 THEN m.quoted ELSE '' END,
    CASE WHEN length(CAST(m.reactions AS BLOB)) <= 1048576 THEN m.reactions END,
    CASE WHEN length(CAST(m.mentions AS BLOB)) <= 1048576 THEN m.mentions END,
    m.edited, m.forwarded, coalesce(c.name, m.chat)
    FROM messages m
    LEFT JOIN chats c ON c.id = m.chat
    LEFT JOIN lids l ON l.lid = m.sender
    LEFT JOIN contacts n ON n.id = coalesce(l.pn, m.sender)";

pub(super) struct Store {
    connection: Connection,
    dirs: AppDirs,
}

impl Store {
    pub(super) fn open(dirs: &AppDirs) -> Result<Self> {
        if !dirs.archive_db().is_file() {
            return Err(Failure::new(
                "archive_unavailable",
                "No local archive exists. Open Whatsapp and link/sync the account first.",
            ));
        }
        let connection = Connection::open_with_flags(
            dirs.archive_db(),
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        connection.busy_timeout(Duration::from_millis(500))?;
        connection.execute_batch("PRAGMA query_only = ON; BEGIN DEFERRED;")?;
        let started = Instant::now();
        connection.progress_handler(
            10_000,
            Some(move || started.elapsed() > Duration::from_secs(5)),
        )?;
        Ok(Self {
            connection,
            dirs: dirs.clone(),
        })
    }

    fn canonical(&self, id: &str) -> Result<String> {
        identifier(id)?;
        Ok(self
            .connection
            .query_row("SELECT pn FROM lids WHERE lid = ?1", [id], |r| r.get(0))
            .optional()?
            .unwrap_or_else(|| id.to_owned()))
    }

    fn require_chat(&self, id: &str) -> Result<String> {
        let id = self.canonical(id)?;
        let found: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM chats WHERE id = ?1)",
            [&id],
            |r| r.get(0),
        )?;
        if !found {
            return Err(Failure::new(
                "chat_not_found",
                "Chat not found in the local archive. Use list_chats to find its ID.",
            ));
        }
        Ok(id)
    }

    pub(super) fn chats(&self, args: ChatQuery) -> Result<Value> {
        let limit = bounded(args.limit, 30, 100)?;
        check_query(&args.query, true)?;
        let scope = json!(["chats", args.query]);
        let cursor = Cursor::decode(args.cursor.as_deref(), &scope)?;
        let mut sql = String::from("SELECT c.id, coalesce(nullif(n.full_name,''), nullif(c.name,''), nullif(n.push_name,''), c.id), c.kind, c.last_activity, c.unread, c.archived, c.pinned,
            CASE WHEN json_valid(c.participants) THEN json_array_length(c.participants) ELSE 0 END
            FROM chats c LEFT JOIN contacts n ON n.id = c.id WHERE 1=1");
        let mut values: Vec<SqlValue> = vec![];
        if !args.query.is_empty() {
            sql.push_str(" AND (c.name LIKE ? ESCAPE '\\' OR c.id LIKE ? ESCAPE '\\' OR n.full_name LIKE ? ESCAPE '\\' OR n.push_name LIKE ? ESCAPE '\\')");
            values.extend(std::iter::repeat_n(SqlValue::Text(like(&args.query)), 4));
        }
        if let Some(cursor) = cursor {
            sql.push_str(" AND (c.last_activity, c.id) < (?, ?)");
            values.extend([
                SqlValue::Integer(cursor.timestamp),
                SqlValue::Text(cursor.id),
            ]);
        }
        sql.push_str(" ORDER BY c.last_activity DESC, c.id DESC LIMIT ?");
        values.push(SqlValue::Integer(limit as i64 + 1));
        let mut statement = self.connection.prepare(&sql)?;
        let mut rows = statement.query(params_from_iter(values))?;
        let mut chats = Vec::new();
        let mut next = None;
        let mut last = None;
        let mut bytes = 0;
        while let Some(row) = rows.next()? {
            let chat = json!({
                "id": row.get::<_, String>(0)?, "name": row.get::<_, String>(1)?,
                "kind": row.get::<_, String>(2)?, "last_activity": row.get::<_, i64>(3)?,
                "unread_count": row.get::<_, i64>(4)?, "archived": row.get::<_, bool>(5)?,
                "pinned": row.get::<_, bool>(6)?, "participant_count": row.get::<_, i64>(7)?
            });
            let size = chat.to_string().len();
            if chats.len() == limit || (!chats.is_empty() && bytes + size > PAGE_BYTES) {
                next = last;
                break;
            }
            last = Some(Cursor::from_chat(&chat, scope.clone()).encode()?);
            chats.push(chat);
            bytes += size;
        }
        Ok(json!({"chats": chats, "next_cursor": next, "order": "newest_activity_first"}))
    }

    pub(super) fn messages(&self, mut args: MessageQuery, search: bool) -> Result<Value> {
        let limit = bounded(args.limit, 30, 100)?;
        if args.since.zip(args.before).is_some_and(|(a, b)| a >= b)
            || (!search && (args.chat_id.is_none() || args.query.is_some()))
        {
            return Err(Failure::invalid());
        }
        if search {
            check_query(args.query.as_deref().ok_or_else(Failure::invalid)?, false)?;
        }
        if let Some(chat) = &args.chat_id {
            args.chat_id = Some(self.require_chat(chat)?);
        }
        let scope = json!([
            "messages",
            search,
            args.chat_id,
            args.query,
            args.since,
            args.before
        ]);
        let cursor = Cursor::decode(args.cursor.as_deref(), &scope)?;
        let mut sql = format!("{MESSAGE_SELECT} WHERE 1=1");
        let mut values: Vec<SqlValue> = vec![];
        if let Some(chat) = &args.chat_id {
            sql.push_str(" AND m.chat = ?");
            values.push(chat.clone().into());
        }
        if let Some(since) = args.since {
            sql.push_str(" AND m.timestamp >= ?");
            values.push(since.into());
        }
        if let Some(before) = args.before {
            sql.push_str(" AND m.timestamp < ?");
            values.push(before.into());
        }
        if let Some(query) = args.query.as_deref() {
            // The archive index folds ASCII only. Do not lowercase Unicode here:
            // that would make accented characters differ from the indexed text.
            let query = query.to_ascii_lowercase();
            if query.chars().count() >= 3 {
                sql.push_str(" AND m.rowid IN (SELECT rowid FROM message_search WHERE message_search MATCH ?)");
                values.push(format!("\"{}\"", query.replace('"', "\"\"")).into());
            }
            sql.push_str(" AND m.search_text LIKE ? ESCAPE '\\'");
            values.push(like(&query).into());
        }
        if let Some(cursor) = cursor {
            sql.push_str(" AND (m.timestamp, m.chat, m.id) < (?, ?, ?)");
            values.extend([
                cursor.timestamp.into(),
                cursor.chat.into(),
                cursor.id.into(),
            ]);
        }
        sql.push_str(" ORDER BY m.timestamp DESC, m.chat DESC, m.id DESC LIMIT ?");
        values.push(SqlValue::Integer(limit as i64 + 1));
        let mut statement = self.connection.prepare(&sql)?;
        let mut rows = statement.query(params_from_iter(values))?;
        let mut messages = Vec::new();
        let mut next = None;
        let mut last = None;
        let mut bytes = 0;
        while let Some(row) = rows.next()? {
            let message = self.decode_message(row)?;
            let size = message.to_string().len();
            if messages.len() == limit || (!messages.is_empty() && bytes + size > PAGE_BYTES) {
                next = last;
                break;
            }
            last = Some(Cursor::from_message(&message, scope.clone()).encode()?);
            bytes += size;
            messages.push(message);
        }
        if !search {
            messages.reverse();
        }
        Ok(
            json!({"chat_id": args.chat_id, "messages": messages, "next_cursor": next,
            "order": if search {"newest_first"} else {"chronological_within_page"},
            "pagination": "next_cursor reads older matching messages in the local archive"}),
        )
    }

    pub(super) fn message(&self, chat: &str, id: &str) -> Result<Value> {
        let chat = self.require_chat(chat)?;
        identifier(id)?;
        let mut statement = self
            .connection
            .prepare(&format!("{MESSAGE_SELECT} WHERE m.chat = ?1 AND m.id = ?2"))?;
        let mut rows = statement.query(params![chat, id])?;
        let row = rows.next()?.ok_or_else(|| {
            Failure::new(
                "message_not_found",
                "This message is not in the local archive. A quoted message may not have synced.",
            )
        })?;
        self.decode_message(row)
    }

    fn decode_message(&self, row: &rusqlite::Row<'_>) -> Result<Value> {
        let raw: Option<String> = row.get(6)?;
        let content = raw
            .as_deref()
            .and_then(|s| serde_json::from_str::<Content>(s).ok());
        let mut omitted = Vec::new();
        if content.is_none() {
            omitted.push("content (unreadable or larger than 1 MiB)");
        }
        let content = content
            .map(|c| serde_json::to_value(c).unwrap_or(Value::Null))
            .unwrap_or(Value::Null);
        let quoted = read_json::<Quoted>(row, 7, "quoted", &mut omitted)?;
        let reactions = read_json::<Vec<Reaction>>(row, 8, "reactions", &mut omitted)?;
        let mentions = read_json::<Vec<MentionRef>>(row, 9, "mentions", &mut omitted)?;
        let timestamp: i64 = row.get(5)?;
        Ok(json!({
            "chat_id": row.get::<_, String>(0)?, "id": row.get::<_, String>(1)?,
            "sender_id": row.get::<_, String>(2)?, "sender_name": row.get::<_, String>(3)?,
            "from_me": row.get::<_, bool>(4)?, "timestamp": timestamp,
            "timestamp_utc": jiff::Timestamp::from_second(timestamp).ok().map(|t| t.to_string()),
            "content": content::describe(&self.dirs, content), "quoted": quoted,
            "reactions": reactions, "mentions": mentions, "edited": row.get::<_, bool>(10)?,
            "forwarded": row.get::<_, bool>(11)?, "chat_name": row.get::<_, String>(12)?,
            "omitted_fields": omitted
        }))
    }
}

fn read_json<T: for<'de> Deserialize<'de> + Serialize>(
    row: &rusqlite::Row<'_>,
    column: usize,
    field: &'static str,
    omitted: &mut Vec<&'static str>,
) -> Result<Value> {
    let text: Option<String> = row.get(column)?;
    match text {
        None if field == "quoted" => Ok(Value::Null),
        Some(text) => match serde_json::from_str::<T>(&text) {
            Ok(value) => Ok(serde_json::to_value(value).unwrap_or(Value::Null)),
            Err(_) => {
                omitted.push(field);
                Ok(Value::Null)
            }
        },
        None => {
            omitted.push(field);
            Ok(Value::Null)
        }
    }
}

fn check_query(query: &str, empty_ok: bool) -> Result<()> {
    if query.len() > 512 || query.contains('\0') || (!empty_ok && query.trim().is_empty()) {
        return Err(Failure::invalid());
    }
    Ok(())
}

fn like(query: &str) -> String {
    format!(
        "%{}%",
        query
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    )
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    version: u8,
    scope: Value,
    timestamp: i64,
    chat: String,
    id: String,
}

impl Cursor {
    fn from_message(message: &Value, scope: Value) -> Self {
        Self {
            version: 1,
            scope,
            timestamp: message["timestamp"].as_i64().unwrap_or(0),
            chat: message["chat_id"].as_str().unwrap_or_default().to_owned(),
            id: message["id"].as_str().unwrap_or_default().to_owned(),
        }
    }

    fn from_chat(chat: &Value, scope: Value) -> Self {
        Self {
            version: 1,
            scope,
            timestamp: chat["last_activity"].as_i64().unwrap_or(0),
            chat: String::new(),
            id: chat["id"].as_str().unwrap_or_default().to_owned(),
        }
    }

    fn encode(&self) -> Result<String> {
        Ok(URL_SAFE_NO_PAD.encode(serde_json::to_vec(self).map_err(|_| Failure::invalid())?))
    }

    fn decode(encoded: Option<&str>, scope: &Value) -> Result<Option<Self>> {
        let Some(encoded) = encoded else {
            return Ok(None);
        };
        if encoded.len() > 4096 {
            return Err(Failure::invalid());
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| Failure::invalid())?;
        let cursor: Self = serde_json::from_slice(&bytes).map_err(|_| Failure::invalid())?;
        if cursor.version != 1
            || cursor.scope != *scope
            || cursor.id.len() > 256
            || cursor.chat.len() > 256
        {
            return Err(Failure::invalid());
        }
        Ok(Some(cursor))
    }
}

pub(super) fn status(dirs: &AppDirs) -> Result<Value> {
    if !dirs.archive_db().is_file() {
        return Ok(
            json!({"archive_available": false, "hint": "Open Whatsapp and link/sync your account first."}),
        );
    }
    let store = Store::open(dirs)?;
    let chats: i64 = store
        .connection
        .query_row("SELECT count(*) FROM chats", [], |r| r.get(0))?;
    let newest: Option<i64> =
        store
            .connection
            .query_row("SELECT max(timestamp) FROM messages", [], |r| r.get(0))?;
    Ok(
        json!({"archive_available": true, "chat_count": chats, "newest_message_timestamp": newest,
        "live_connection_status": "unknown", "history_completeness": "unknown",
        "hint": "Open Whatsapp for new messages to sync. Only locally archived messages and downloaded attachments are available. Agent reads do not mark chats as read."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_reader_cannot_write_even_if_query_only_is_disabled() {
        let temp = tempfile::tempdir().unwrap();
        let dirs = AppDirs::under(temp.path());
        dirs.ensure().unwrap();
        let _archive = crate::archive::Archive::open(&dirs.archive_db()).unwrap();
        let reader = Store::open(&dirs).unwrap();
        reader
            .connection
            .execute_batch("PRAGMA query_only = OFF;")
            .unwrap();
        let error = reader
            .connection
            .execute(
                "INSERT INTO meta(key,value) VALUES ('agent-test','forbidden')",
                [],
            )
            .unwrap_err();
        assert_eq!(
            error.sqlite_error_code(),
            Some(rusqlite::ErrorCode::ReadOnly)
        );
        assert_eq!(
            reader
                .connection
                .query_row(
                    "SELECT count(*) FROM meta WHERE key = 'agent-test'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
    }
}
