//! A derived substring index. The message archive remains the source of truth.

use rusqlite::Connection;

/// Match the six fields and ASCII case folding used by the original search.
const SEARCH_TEXT: &str = "CASE WHEN json_valid(content) THEN lower(
    coalesce(json_extract(content, '$.text'), '') || char(10) ||
    coalesce(json_extract(content, '$.caption'), '') || char(10) ||
    coalesce(json_extract(content, '$.file_name'), '') || char(10) ||
    coalesce(json_extract(content, '$.question'), '') || char(10) ||
    coalesce(json_extract(content, '$.display_name'), '') || char(10) ||
    coalesce(json_extract(content, '$.name'), '')) ELSE '' END";

pub(super) fn install(connection: &Connection) -> rusqlite::Result<()> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'message_search')",
        [],
        |row| row.get(0),
    )?;
    if exists {
        return Ok(());
    }
    // Build the column, triggers and index in one transaction. An interrupted
    // first launch can safely retry, including for an existing message archive.
    let transaction = connection.unchecked_transaction()?;
    transaction.execute_batch(&format!(
        "ALTER TABLE messages ADD COLUMN search_text TEXT GENERATED ALWAYS AS ({SEARCH_TEXT}) VIRTUAL;
         CREATE INDEX messages_by_global_time ON messages(timestamp DESC);
         CREATE VIRTUAL TABLE message_search USING fts5(
             search_text, content='messages', content_rowid='rowid',
             tokenize='trigram case_sensitive 1'
         );
         CREATE TRIGGER message_search_insert AFTER INSERT ON messages BEGIN
             INSERT INTO message_search(rowid, search_text) VALUES (new.rowid, new.search_text);
         END;
         CREATE TRIGGER message_search_delete AFTER DELETE ON messages BEGIN
             INSERT INTO message_search(message_search, rowid, search_text)
                 VALUES ('delete', old.rowid, old.search_text);
         END;
         CREATE TRIGGER message_search_update AFTER UPDATE OF content ON messages
         WHEN old.content IS NOT new.content BEGIN
             INSERT INTO message_search(message_search, rowid, search_text)
                 VALUES ('delete', old.rowid, old.search_text);
             INSERT INTO message_search(rowid, search_text) VALUES (new.rowid, new.search_text);
         END;
         INSERT INTO message_search(message_search) VALUES ('rebuild');"
    ))?;
    transaction.commit()
}

/// Quote the entire string as a literal FTS phrase, never as query syntax.
/// Trigrams need three Unicode characters. NUL follows SQLite LIKE's existing
/// behavior through the fallback path instead of being interpreted by FTS.
pub(super) fn phrase(needle: &str) -> Option<String> {
    (needle.chars().count() >= 3 && !needle.contains('\0'))
        .then(|| format!("\"{}\"", needle.replace('"', "\"\"")))
}
