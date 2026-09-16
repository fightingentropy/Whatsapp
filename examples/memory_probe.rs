//! Offline conversation-cache soak. No fonts, native window, network or account.
//! RSS is whole-process resident memory; payload bytes count text and thumbnails
//! only, not allocator overhead, model fields or renderer/codec allocations.

use zapfast::{
    app::{App, Conversation},
    backend::LinkStatus,
    model::{Action, Chat, Content, Delivery, Media, MediaState, Message},
    paths::AppDirs,
    settings::Settings,
};

fn rss_kib() -> anyhow::Result<u64> {
    let output = std::process::Command::new("/bin/ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()?;
    anyhow::ensure!(output.status.success(), "ps failed");
    Ok(std::str::from_utf8(&output.stdout)?.trim().parse()?)
}

fn snapshot(app: &App, visited: usize) -> anyhow::Result<serde_json::Value> {
    let messages: usize = app
        .conversations
        .values()
        .map(|chat| chat.messages.len())
        .sum();
    let bytes: usize = app
        .conversations
        .values()
        .flat_map(|chat| &chat.messages)
        .map(|message| {
            message.thumbnail.as_ref().map_or(0, Vec::len)
                + match &message.content {
                    Content::Text { text, .. } => text.len(),
                    Content::Image { caption, .. } => caption.as_ref().map_or(0, String::len),
                    _ => 0,
                }
        })
        .sum();
    Ok(serde_json::json!({
        "visited_chats": visited, "resident_kib": rss_kib()?,
        "cached_conversations": app.conversations.values().filter(|chat| !chat.messages.is_empty()).count(),
        "cached_messages": messages, "retained_text_thumbnail_bytes": bytes,
    }))
}

fn main() -> anyhow::Result<()> {
    let root = std::env::temp_dir().join(format!("zapfast-memory-probe-{}", std::process::id()));
    std::fs::create_dir(&root)?;
    let (mut app, _events) = App::headless(AppDirs::under(&root), Settings::default());
    app.link = LinkStatus::Connected;
    let ctx = egui::Context::default();
    let mut samples = vec![snapshot(&app, 0)?];
    for index in 0..96 {
        let chat = format!("synthetic-{index}@g.us");
        app.chats
            .push(Chat::new(chat.clone(), format!("Synthetic group {index}")));
        let messages = (0..300)
            .map(|row| {
                let picture = row % 4 == 0;
                let text = "x".repeat(1024);
                Message {
                    id: format!("synthetic-{index}-{row}"),
                    chat: chat.clone(),
                    sender: "synthetic-sender@lid".into(),
                    sender_name: Some("Synthetic sender".into()),
                    from_me: false,
                    timestamp: 1_750_000_000 + row,
                    content: if picture {
                        Content::Image {
                            caption: Some(text),
                            media: Media {
                                mime: "image/jpeg".into(),
                                size: 8192,
                                width: Some(640),
                                height: Some(480),
                                path: None,
                                state: MediaState::Idle,
                            },
                        }
                    } else {
                        Content::text(text)
                    },
                    status: Delivery::None,
                    delivered_at: None,
                    read_at: None,
                    quoted: None,
                    reactions: Vec::new(),
                    edited: false,
                    mentions: Vec::new(),
                    forwarded: false,
                    thumbnail: picture.then(|| vec![42; 8192]),
                }
            })
            .collect();
        // Model an archived history page arriving before opening the next chat.
        app.conversations.insert(
            chat.clone(),
            Conversation {
                messages,
                requested: true,
                complete: true,
                ..Default::default()
            },
        );
        app.actions.push(Action::OpenChat(chat));
        app.background_frame(&ctx);
        if (index + 1) % 12 == 0 {
            samples.push(snapshot(&app, index + 1)?);
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "workload": "visit 96 synthetic conversations with 300 messages each; 1024-byte text/caption per message and an 8192-byte thumbnail every fourth message",
            "note": "headless application state only; no renderer, fonts, real account or archive; cache reload correctness is covered separately by SQLite regression tests",
            "samples": samples,
        }))?
    );
    drop(app);
    std::fs::remove_dir_all(root)?;
    Ok(())
}
