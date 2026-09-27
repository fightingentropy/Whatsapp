use super::*;
use crate::{
    archive::Archive,
    model::{Chat, Contact, Content, Delivery, Media, MediaState, Message, Quoted},
};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Fixture {
    root: PathBuf,
    dirs: AppDirs,
    archive: Archive,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "whatsapp-agent-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let dirs = AppDirs::under(&root);
        dirs.ensure().unwrap();
        let archive = Archive::open(&dirs.archive_db()).unwrap();
        for id in ["alpha", "beta"] {
            let mut chat = Chat::new(id.into(), format!("Test {id}"));
            chat.unread = 3;
            chat.last_activity = 10;
            archive.upsert_chat(&chat).unwrap();
        }
        Self {
            root,
            dirs,
            archive,
        }
    }

    fn call(&self, name: &str, args: Value) -> Value {
        execute(&self.dirs, name, args).unwrap().value
    }

    fn insert(&self, chat: &str, id: &str, text: &str) {
        self.archive
            .insert_message(
                &message(chat, id, Content::text(text)),
                Some(b"secret raw protobuf must never be returned"),
            )
            .unwrap();
    }

    fn attach(&self, id: &str, path: &std::path::Path, mime: &str) {
        let media = Media {
            mime: mime.into(),
            size: 123,
            width: None,
            height: None,
            path: Some(path.into()),
            state: MediaState::default(),
        };
        self.archive
            .insert_message(
                &message(
                    "alpha",
                    id,
                    Content::Document {
                        media,
                        file_name: "fixture.pdf".into(),
                        caption: Some("Test attachment".into()),
                        pages: None,
                    },
                ),
                None,
            )
            .unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn message(chat: &str, id: &str, content: Content) -> Message {
    Message {
        id: id.into(),
        chat: chat.into(),
        sender: "author@lid".into(),
        sender_name: Some("Original name".into()),
        from_me: false,
        timestamp: 10,
        content,
        status: Delivery::None,
        delivered_at: None,
        read_at: None,
        quoted: None,
        reactions: vec![],
        edited: false,
        mentions: vec![],
        forwarded: false,
        thumbnail: Some(vec![1, 2, 3]),
    }
}

#[test]
fn missing_profile_is_not_created() {
    let fixture = Fixture::new();
    let absent = fixture.root.join("does-not-exist");
    let dirs = AppDirs::under(&absent);
    assert_eq!(
        execute(&dirs, "status", json!({})).unwrap().value["archive_available"],
        false
    );
    assert!(execute(&dirs, "list_chats", json!({})).is_err());
    assert!(!absent.exists());
}

#[test]
fn reads_committed_wal_without_changing_unread_or_exposing_raw_data() {
    let f = Fixture::new();
    f.insert("alpha", "a", "first");
    let first = f.call("read_messages", json!({"chat_id":"alpha"}));
    assert_eq!(first["messages"].as_array().unwrap().len(), 1);
    f.insert("alpha", "b", "second");
    let second = f.call("read_messages", json!({"chat_id":"alpha"}));
    assert_eq!(second["messages"].as_array().unwrap().len(), 2);
    assert_eq!(f.archive.chat("alpha").unwrap().unwrap().unread, 3);
    assert!(!second.to_string().contains("secret raw"));
    assert!(second["messages"][0].get("thumbnail").is_none());
    assert!(!f.dirs.session_db().exists());
    assert!(!f.dirs.log_file().exists());
}

#[test]
fn chat_and_message_pagination_preserve_timestamp_ties_and_scope() {
    let f = Fixture::new();
    for chat in ["alpha", "beta"] {
        for id in ["a", "b", "c"] {
            f.insert(chat, id, "matching 🍋");
        }
    }
    let chats = f.call("list_chats", json!({"limit":1}));
    assert_eq!(chats["chats"][0]["id"], "beta");
    let next = f.call(
        "list_chats",
        json!({"limit":1,"cursor":chats["next_cursor"]}),
    );
    assert_eq!(next["chats"][0]["id"], "alpha");
    assert!(next["next_cursor"].is_null());
    let first = f.call("read_messages", json!({"chat_id":"alpha","limit":2}));
    assert_eq!(first["messages"][0]["id"], "b");
    assert_eq!(first["messages"][1]["id"], "c");
    let older = f.call(
        "read_messages",
        json!({"chat_id":"alpha","limit":2,"cursor":first["next_cursor"]}),
    );
    assert_eq!(older["messages"][0]["id"], "a");
    assert!(older["next_cursor"].is_null());
    assert!(
        execute(
            &f.dirs,
            "read_messages",
            json!({"chat_id":"beta","cursor":first["next_cursor"]})
        )
        .is_err()
    );
    let mut cursor = Value::Null;
    let mut ids = Vec::new();
    loop {
        let page = f.call(
            "search_messages",
            json!({"query":"matching", "limit":2,"cursor":cursor}),
        );
        for message in page["messages"].as_array().unwrap() {
            ids.push((message["chat_id"].clone(), message["id"].clone()));
        }
        cursor = page["next_cursor"].clone();
        if cursor.is_null() {
            break;
        }
    }
    assert_eq!(ids.len(), 6);
    assert_eq!(
        ids.iter().collect::<std::collections::HashSet<_>>().len(),
        6
    );
}

#[test]
fn search_is_literal_and_filters_before_paging() {
    let f = Fixture::new();
    for (id, text) in [
        ("a", "100%_done \\\" quoted"),
        ("b", "100000done"),
        ("c", "ÉTÉ 🍋"),
    ] {
        f.insert("alpha", id, text);
    }
    f.insert("beta", "z", "100%_done");
    for query in ["100%_", "\\", "\"", "ÉTÉ", "🍋"] {
        let page = f.call(
            "search_messages",
            json!({"chat_id":"alpha", "query":query, "since":10, "before":11}),
        );
        assert_eq!(page["messages"].as_array().unwrap().len(), 1, "{query}");
    }
    let page = f.call("search_messages", json!({"query":"100%_", "before":10}));
    assert!(page["messages"].as_array().unwrap().is_empty());
}

#[test]
fn names_aliases_replies_and_links_are_structured() {
    let f = Fixture::new();
    f.archive
        .put_lid("author@lid", "author@s.whatsapp.net")
        .unwrap();
    f.archive.put_lid("alias@lid", "alpha").unwrap();
    f.archive
        .upsert_contact(&Contact {
            id: "author@s.whatsapp.net".into(),
            full_name: Some("Known Author".into()),
            push_name: None,
        })
        .unwrap();
    let mut m = message(
        "alpha",
        "reply",
        Content::Text {
            text: "Read https://example.com/a".into(),
            preview: Some(crate::model::LinkPreview {
                url: "https://example.com/a".into(),
                title: Some("Article".into()),
                description: Some("Description".into()),
            }),
        },
    );
    m.quoted = Some(Quoted {
        id: "original".into(),
        sender: "author@lid".into(),
        sender_name: None,
        summary: "Original text".into(),
        mentions: vec![],
    });
    f.archive.insert_message(&m, None).unwrap();
    let result = f.call(
        "get_message",
        json!({"chat_id":"alias@lid","message_id":"reply"}),
    );
    assert_eq!(result["chat_id"], "alpha");
    assert_eq!(result["sender_id"], "author@s.whatsapp.net");
    assert_eq!(result["sender_name"], "Known Author");
    assert_eq!(result["quoted"]["id"], "original");
    assert_eq!(result["content"]["preview"]["url"], "https://example.com/a");
    assert_eq!(result["untrusted_content"], true);
}

#[test]
fn input_bounds_and_unknown_fields_fail_closed() {
    let f = Fixture::new();
    for (tool, args) in [
        ("status", json!({"send":true})),
        ("list_chats", json!({"limit":0})),
        ("list_chats", json!({"limit":101})),
        ("read_messages", json!({})),
        ("read_messages", json!({"chat_id":"alpha","query":"x"})),
        ("search_messages", json!({"query":""})),
        ("search_messages", json!({"query":"x\0y"})),
        (
            "search_messages",
            json!({"query":"x","since":20,"before":10}),
        ),
        (
            "read_messages",
            json!({"chat_id":"alpha","cursor":"garbage"}),
        ),
        ("send_message", json!({"text":"never"})),
    ] {
        assert!(execute(&f.dirs, tool, args).is_err(), "{tool}");
    }
    for tool in tools() {
        assert_eq!(tool.annotations.unwrap().read_only_hint, Some(true));
        assert_eq!(tool.input_schema["additionalProperties"], false);
    }
}

#[test]
fn response_byte_budget_does_not_skip_large_messages() {
    let f = Fixture::new();
    for id in ["a", "b", "c"] {
        f.insert("alpha", id, &"x".repeat(150_000));
    }
    let first = f.call("read_messages", json!({"chat_id":"alpha"}));
    assert_eq!(first["messages"].as_array().unwrap().len(), 1);
    let second = f.call(
        "read_messages",
        json!({"chat_id":"alpha","cursor":first["next_cursor"]}),
    );
    assert_eq!(second["messages"][0]["id"], "b");
}

#[test]
fn oversized_and_corrupt_fields_are_explicitly_omitted() {
    let f = Fixture::new();
    f.insert("alpha", "huge", &"x".repeat(1_048_577));
    let huge = f.call(
        "get_message",
        json!({"chat_id":"alpha","message_id":"huge"}),
    );
    assert!(huge["content"].is_null());
    assert!(!huge["omitted_fields"].as_array().unwrap().is_empty());
    let connection = rusqlite::Connection::open(f.dirs.archive_db()).unwrap();
    connection
        .execute(
            "UPDATE messages SET content = 'invalid', quoted = ? WHERE id = 'huge'",
            ["x".repeat(1_048_577)],
        )
        .unwrap();
    let corrupt = f.call(
        "get_message",
        json!({"chat_id":"alpha","message_id":"huge"}),
    );
    assert!(
        corrupt["omitted_fields"]
            .as_array()
            .unwrap()
            .contains(&json!("quoted"))
    );
    assert!(corrupt["content"].is_null());
}

#[test]
fn attachments_are_confined_repaired_and_chunked_without_archive_writes() {
    let f = Fixture::new();
    std::fs::create_dir_all(f.dirs.media_cache_dir()).unwrap();
    let text = f.dirs.media_cache_dir().join("note.txt");
    std::fs::write(&text, "A🍋éZ").unwrap();
    let old = f.root.join("old-cache/note.txt");
    f.attach("text", &old, "text/plain");
    let result = f.call(
        "read_attachment",
        json!({"chat_id":"alpha", "message_id":"text", "offset":1,"max_chars":2}),
    );
    assert_eq!(result["text"], "🍋é");
    assert_eq!(result["next_offset"], 3);
    assert_eq!(
        f.archive
            .message("alpha", "text")
            .unwrap()
            .unwrap()
            .content
            .media()
            .unwrap()
            .path
            .as_ref(),
        Some(&old)
    );
    let secret = f.root.join("secret.txt");
    std::fs::write(&secret, "must not read").unwrap();
    f.attach("outside", &secret, "text/plain");
    assert!(
        execute(
            &f.dirs,
            "read_attachment",
            json!({"chat_id":"alpha","message_id":"outside"})
        )
        .is_err()
    );
    #[cfg(unix)]
    {
        let symlink = f.dirs.media_cache_dir().join("escape.txt");
        std::os::unix::fs::symlink(&secret, &symlink).unwrap();
        f.attach("symlink", &symlink, "text/plain");
        assert!(
            execute(
                &f.dirs,
                "read_attachment",
                json!({"chat_id":"alpha","message_id":"symlink"})
            )
            .is_err()
        );
    }
    let image = f.dirs.media_cache_dir().join("test.png");
    image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 100, 200, 255]))
        .save(&image)
        .unwrap();
    f.attach("image", &image, "image/png");
    let reply = execute(
        &f.dirs,
        "read_attachment",
        json!({"chat_id":"alpha","message_id":"image"}),
    )
    .unwrap();
    assert_eq!(reply.value["width"], 4);
    assert_eq!(reply.image.unwrap().1, "image/png");
}

#[test]
#[cfg(target_os = "macos")]
fn pdf_text_is_extracted_with_page_and_character_paging() {
    let f = Fixture::new();
    std::fs::create_dir_all(f.dirs.media_cache_dir()).unwrap();
    let path = f.dirs.media_cache_dir().join("fixture.pdf");
    std::fs::write(&path, synthetic_pdf()).unwrap();
    f.attach("pdf", &path, "application/pdf");
    let result = f.call(
        "read_attachment",
        json!({"chat_id":"alpha","message_id":"pdf","max_chars":5}),
    );
    assert_eq!(result["representation"], "pdf_text");
    assert_eq!(result["page_count"], 2);
    assert_eq!(result["next_page"], 2);
    assert_eq!(result["text"], "Hello");
    assert_eq!(result["next_offset"], 5);
    let second = f.call(
        "read_attachment",
        json!({"chat_id":"alpha","message_id":"pdf","page":2}),
    );
    assert!(second["text"].as_str().unwrap().contains("Second page"));
    assert!(second["next_page"].is_null());
    assert!(
        execute(
            &f.dirs,
            "read_attachment",
            json!({"chat_id":"alpha","message_id":"pdf","page":3})
        )
        .is_err()
    );
}

#[cfg(target_os = "macos")]
fn synthetic_pdf() -> Vec<u8> {
    let stream = |text: &str| {
        let body = format!("BT /F1 16 Tf 40 100 Td ({text}) Tj ET\n");
        format!("<< /Length {} >>\nstream\n{body}endstream", body.len())
    };
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        stream("Hello agent PDF"),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Resources << /Font << /F1 4 0 R >> >> /Contents 7 0 R >>".into(),
        stream("Second page"),
    ];
    let mut pdf = String::from("%PDF-1.4\n");
    let mut offsets = vec![0];
    for (i, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{object}\nendobj\n", i + 1));
    }
    let xref = pdf.len();
    pdf.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()));
    for offset in &offsets[1..] {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        offsets.len()
    ));
    pdf.into_bytes()
}
