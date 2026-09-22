//! Regenerate the native picker's catalogue from the desktop's emoji database.
fn main() {
    let entries: Vec<_> = emojis::iter().map(|emoji| serde_json::json!({
        "emoji": emoji.as_str(), "name": emoji.name(), "shortcodes": emoji.shortcodes().collect::<Vec<_>>()
    })).collect();
    println!(
        "{}",
        serde_json::to_string(&entries).expect("emoji catalogue")
    );
}
