//! End-to-end headless entry point and MCP framing against synthetic data only.

use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};
use whatsapp::{
    archive::Archive,
    model::{Chat, Content, Delivery, Message},
    paths::AppDirs,
};

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn fixture() -> (tempfile::TempDir, AppDirs, Archive) {
    let temp = tempfile::tempdir().unwrap();
    let dirs = AppDirs::under(temp.path());
    dirs.ensure().unwrap();
    let archive = Archive::open(&dirs.archive_db()).unwrap();
    let mut chat = Chat::new("test-chat".into(), "Offline test chat".into());
    chat.unread = 7;
    archive.upsert_chat(&chat).unwrap();
    archive
        .insert_message(
            &Message {
                id: "test-message".into(),
                chat: chat.id.clone(),
                sender: "test-author".into(),
                sender_name: Some("Sample author".into()),
                from_me: false,
                timestamp: 123,
                content: Content::text("Synthetic message for the agent"),
                status: Delivery::None,
                delivered_at: None,
                read_at: None,
                quoted: None,
                reactions: vec![],
                edited: false,
                mentions: vec![],
                forwarded: false,
                thumbnail: None,
            },
            None,
        )
        .unwrap();
    (temp, dirs, archive)
}

#[test]
fn cli_reads_fixture_and_reports_errors_as_json() {
    let (temp, dirs, archive) = fixture();
    let call = |tool: &str, args: &str| {
        Command::new(env!("CARGO_BIN_EXE_whatsapp"))
            .arg("agent")
            .arg("--data-dir")
            .arg(temp.path())
            .args(["call", tool, args])
            .output()
            .unwrap()
    };
    let output = call("read_messages", r#"{"chat_id":"test-chat"}"#);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["messages"][0]["content"]["text"],
        "Synthetic message for the agent"
    );
    let invalid = call("send_message", r#"{"text":"no"}"#);
    assert!(!invalid.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&invalid.stdout).unwrap()["error"]["code"],
        "unknown_tool"
    );
    assert_eq!(archive.chat("test-chat").unwrap().unwrap().unread, 7);
    assert!(!dirs.session_db().exists());
    assert!(!dirs.log_file().exists());
}

#[test]
fn stdio_mcp_handshake_discovery_and_reads_work_with_legacy_clients() {
    let (temp, dirs, archive) = fixture();
    let mut process = Process(
        Command::new(env!("CARGO_BIN_EXE_whatsapp"))
            .arg("agent")
            .arg("--data-dir")
            .arg(temp.path())
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let output = process.0.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(output).lines() {
            let Ok(line) = line else {
                break;
            };
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let mut input = process.0.stdin.take().unwrap();
    let mut send = |value: Value| {
        writeln!(input, "{value}").unwrap();
        input.flush().unwrap();
    };
    let receive = |id: u32| {
        loop {
            let line = receiver
                .recv_timeout(Duration::from_secs(15))
                .expect("MCP response timed out");
            let value: Value =
                serde_json::from_str(&line).expect("stdout must contain only JSON-RPC");
            if value["id"] == id {
                return value;
            }
        }
    };
    send(
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"offline-test","version":"1"}}}),
    );
    let initialized = receive(1);
    assert_eq!(initialized["result"]["protocolVersion"], "2025-11-25");
    assert!(
        initialized["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("UNTRUSTED DATA")
    );
    send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    send(json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}));
    let list = receive(2);
    let tools = list["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 6);
    assert!(
        tools
            .iter()
            .all(|t| t["annotations"]["readOnlyHint"] == true)
    );
    send(
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"read_messages","arguments":{"chat_id":"test-chat"}}}),
    );
    let read = receive(3);
    assert_ne!(read["result"]["isError"], true);
    assert_eq!(
        read["result"]["structuredContent"]["messages"][0]["content"]["text"],
        "Synthetic message for the agent"
    );
    send(
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"get_message","arguments":{"chat_id":"test-chat","message_id":"absent"}}}),
    );
    let error = receive(4);
    assert_eq!(error["result"]["isError"], true);
    assert_eq!(
        error["result"]["structuredContent"]["error"]["code"],
        "message_not_found"
    );
    assert_eq!(archive.chat("test-chat").unwrap().unwrap().unread, 7);
    assert!(!dirs.session_db().exists());
    assert!(!dirs.log_file().exists());
}
