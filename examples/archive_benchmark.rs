//! Reproducible synthetic archive benchmark. Never reads a linked account.
//! Run with `cargo run --locked --example archive_benchmark`.
use std::hint::black_box;
use std::path::PathBuf;
use std::time::Instant;

use rusqlite::{Connection, params};
use zapfast::archive::Archive;
use zapfast::model::{Content, Delivery, Message};

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn message(i: usize) -> Message {
    Message {
        chat: format!("synthetic-chat-{}", i % 100),
        id: format!("synthetic-{i}"),
        sender: "synthetic-sender".into(),
        sender_name: None,
        from_me: false,
        timestamp: i as i64,
        content: Content::text(format!(
            "Ordinary project conversation number {i}. Meeting notes and progress on the plan. {}",
            if i.is_multiple_of(1000) {
                "parcelreference"
            } else {
                ""
            }
        )),
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

fn query(connection: &Connection, sql: &str, pattern: &str, phrase: &str) -> Vec<String> {
    connection
        .prepare_cached(sql)
        .unwrap()
        .query_map(params![pattern, phrase], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

fn median_ms(mut times: Vec<f64>) -> f64 {
    times.sort_by(f64::total_cmp);
    times[times.len() / 2]
}

fn main() {
    let root = Scratch(std::env::temp_dir().join(format!(
        "zapfast-silicon-benchmark-{}-{}",
        std::process::id(),
        jiff::Timestamp::now().as_millisecond()
    )));
    std::fs::create_dir(&root.0).unwrap();
    let rows: Vec<_> = (0..100_000).map(message).collect();
    let raw = vec![42u8; 256];
    let archive = Archive::open(&root.0.join("search.db")).unwrap();
    for chunk in rows.chunks(256) {
        archive
            .insert_messages(chunk.iter().map(|row| (row, Some(raw.as_slice()))))
            .unwrap();
    }
    let connection = Connection::open(root.0.join("search.db")).unwrap();
    // Upstream had no global timestamp index; NOT INDEXED reproduces its scan.
    // Both queries return identical IDs, with identical sorting and limits.
    let old_sql = "SELECT id FROM messages NOT INDEXED WHERE json_valid(content) AND lower(
        coalesce(json_extract(content, '$.text'), '') || char(10) ||
        coalesce(json_extract(content, '$.caption'), '') || char(10) ||
        coalesce(json_extract(content, '$.file_name'), '') || char(10) ||
        coalesce(json_extract(content, '$.question'), '') || char(10) ||
        coalesce(json_extract(content, '$.display_name'), '') || char(10) ||
        coalesce(json_extract(content, '$.name'), '')) LIKE ?1 ESCAPE '\\'
        AND ?2 IS NOT NULL ORDER BY timestamp DESC, rowid DESC LIMIT 50";
    let new_sql =
        "SELECT id FROM messages WHERE json_valid(content) AND search_text LIKE ?1 ESCAPE '\\'
        AND rowid IN (SELECT rowid FROM message_search WHERE message_search MATCH ?2)
        ORDER BY timestamp DESC, rowid DESC LIMIT 50";
    let short_sql =
        "SELECT id FROM messages WHERE json_valid(content) AND search_text LIKE ?1 ESCAPE '\\'
        AND ?2 IS NOT NULL ORDER BY timestamp DESC, rowid DESC LIMIT 50";
    let recent_sql =
        "SELECT id FROM messages WHERE json_valid(content) AND search_text LIKE ?1 ESCAPE '\\'
        AND rowid IN (SELECT rowid FROM messages ORDER BY timestamp DESC, rowid DESC LIMIT 256)
        AND ?2 IS NOT NULL ORDER BY timestamp DESC, rowid DESC LIMIT 50";
    let mut searches = Vec::new();
    for term in ["parcelreference", "no-such-phrase", "ordinary", "or"] {
        let pattern = format!("%{term}%");
        let phrase = format!("\"{term}\"");
        let indexed = if term.chars().count() >= 3 {
            new_sql
        } else {
            short_sql
        };
        let current = || {
            if term.chars().count() >= 3 {
                let recent = query(&connection, recent_sql, &pattern, &phrase);
                if recent.len() == 50 {
                    return recent;
                }
            }
            query(&connection, indexed, &pattern, &phrase)
        };
        let expected = query(&connection, old_sql, &pattern, &phrase);
        assert_eq!(current(), expected);
        assert_eq!(
            archive
                .search_messages(term, 50)
                .unwrap()
                .into_iter()
                .map(|row| row.id)
                .collect::<Vec<_>>(),
            expected
        );
        let mut old = Vec::new();
        let mut new = Vec::new();
        // Alternate order after warm-up; time SQL only, not UI or network.
        for i in 0..21 {
            for (baseline, times) in if i % 2 == 0 {
                [(true, &mut old), (false, &mut new)]
            } else {
                [(false, &mut new), (true, &mut old)]
            } {
                let started = Instant::now();
                black_box(if baseline {
                    query(&connection, old_sql, &pattern, &phrase)
                } else {
                    current()
                });
                times.push(started.elapsed().as_secs_f64() * 1000.0);
            }
        }
        searches.push(serde_json::json!({"query": term, "hits": expected.len(),
            "old_sql_median_ms": median_ms(old), "new_sql_median_ms": median_ms(new)}));
    }
    let mut single = Vec::new();
    let mut batched = Vec::new();
    for trial in 0..3 {
        for batch in [false, true] {
            let archive = Archive::open(&root.0.join(format!("write-{trial}-{batch}.db"))).unwrap();
            for i in 0..100 {
                archive
                    .ensure_chat(&format!("synthetic-chat-{i}"), "Synthetic")
                    .unwrap();
            }
            let started = Instant::now();
            if batch {
                for chunk in rows[..10_000].chunks(256) {
                    archive
                        .insert_messages(chunk.iter().map(|row| (row, Some(raw.as_slice()))))
                        .unwrap();
                }
            } else {
                for row in &rows[..10_000] {
                    archive.insert_message(row, Some(&raw)).unwrap();
                }
            }
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            if batch {
                batched.push(elapsed);
            } else {
                single.push(elapsed);
            }
            assert!(
                archive
                    .message("synthetic-chat-99", "synthetic-9999")
                    .unwrap()
                    .is_some()
            );
        }
    }
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "dataset": "100000 synthetic text messages, 100 chats, 256 raw bytes per message",
        "sqlite_version": rusqlite::version(),
        "search_trials": 21, "searches": searches,
        "write_rows": 10000, "write_trials": 3, "batch_size": 256,
        "single_insert_median_ms": median_ms(single),
        "batch_insert_median_ms": median_ms(batched),
        "write_note": "Same current indexed schema and public Archive API on both sides; file-backed WAL, synchronous NORMAL"
    })).unwrap());
}
