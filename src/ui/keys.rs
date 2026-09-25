//! Keyboard shortcuts.

use egui::{Key, Modifiers};

use crate::app::App;
use crate::model::{Action, Dialog, Page};

pub fn handle(app: &mut App, ctx: &egui::Context) {
    if app.image_preview.is_some() {
        preview_keys(app, ctx);
        return;
    }
    let mut actions = Vec::new();
    ctx.input_mut(|input| {
        let mut key = |modifiers: Modifiers, key: Key, action: Action| {
            if input.consume_key(modifiers, key) {
                actions.push(action);
            }
        };
        key(
            Modifiers::COMMAND | Modifiers::SHIFT,
            Key::F,
            Action::FocusSearch,
        );
        key(Modifiers::COMMAND, Key::F, Action::Find);
        key(Modifiers::COMMAND, Key::K, Action::FocusSearch);
        key(Modifiers::COMMAND, Key::B, Action::ToggleSidebar);
        key(Modifiers::COMMAND, Key::Comma, Action::Open(Page::Settings));
        key(Modifiers::COMMAND, Key::Q, Action::Quit);
        key(Modifiers::COMMAND, Key::W, Action::CloseWindow);
        key(
            Modifiers::COMMAND,
            Key::Slash,
            Action::ShowDialog(Dialog::Shortcuts),
        );
        key(Modifiers::COMMAND, Key::Plus, Action::ZoomBy(0.1));
        key(Modifiers::COMMAND, Key::Equals, Action::ZoomBy(0.1));
        key(Modifiers::COMMAND, Key::Minus, Action::ZoomBy(-0.1));
        key(Modifiers::COMMAND, Key::Num0, Action::ResetZoom);
        key(Modifiers::COMMAND, Key::End, Action::ScrollToBottom);
    });
    if ctx.memory(|memory| memory.has_focus(egui::Id::new("conversation-search"))) {
        let step = ctx.input_mut(|input| {
            if input.consume_key(Modifiers::SHIFT, Key::Enter)
                || input.consume_key(Modifiers::NONE, Key::ArrowUp)
            {
                -1
            } else if input.consume_key(Modifiers::NONE, Key::Enter)
                || input.consume_key(Modifiers::NONE, Key::ArrowDown)
            {
                1
            } else {
                0
            }
        });
        if step != 0 {
            actions.push(Action::StepChatSearch(step));
        }
    }
    // Escape cancels the topmost state. Menus handle Escape themselves.
    let menu_open = egui::Popup::is_any_open(ctx);
    let search_focused = ctx.memory(|memory| memory.has_focus(egui::Id::new("chat-search")));
    let escape =
        !menu_open && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));
    if escape {
        if app.dialog.is_some() {
            actions.push(Action::CloseDialog);
        } else if app.recording.is_some() {
            actions.push(Action::CancelRecording);
        } else if app.picker.is_some() {
            actions.push(Action::ClosePicker);
        } else if app.emoji_start.is_some() {
            actions.push(Action::CloseEmojiSuggestions);
        } else if app.mention_start.is_some() {
            actions.push(Action::CloseMentions);
        } else if !app.pending.is_empty() {
            actions.push(Action::ClearPending);
        } else if app.editing.is_some() {
            actions.push(Action::CancelEdit);
        } else if app.reply_to.is_some() {
            actions.push(Action::CancelReply);
        } else if app.page == Page::Settings {
            if app.settings_search.is_empty() {
                actions.push(Action::Open(Page::Chats));
            } else {
                app.settings_search.clear();
                app.focus_settings_search = true;
            }
        } else if app.chat_search.chat.is_some() {
            actions.push(Action::CloseChatSearch);
        } else if search_focused || !app.search.is_empty() {
            if !app.search.is_empty() {
                actions.push(Action::Search(String::new()));
            }
            if app.open_chat.is_some() {
                actions.push(Action::FocusComposer);
            }
        }
    }
    // Enter sends a recording because the text field is hidden.
    if app.recording.is_some()
        && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter))
    {
        actions.push(Action::SendRecording);
    }
    // Alt+Up/Down switches chats without leaving the composer.
    let step = ctx.input_mut(|input| {
        if input.consume_key(Modifiers::ALT, Key::ArrowDown) {
            1
        } else if input.consume_key(Modifiers::ALT, Key::ArrowUp) {
            -1
        } else {
            0
        }
    });
    if step != 0 {
        let visible = app.visible_chats();
        if !visible.is_empty() {
            let current = app
                .open_chat
                .as_ref()
                .and_then(|open| visible.iter().position(|chat| chat.id == *open));
            let next = match current {
                Some(index) => (index as i64 + step).rem_euclid(visible.len() as i64) as usize,
                None => 0,
            };
            let next = visible[next].id.clone();
            app.scroll_chat_into_view = Some(next.clone());
            actions.push(Action::OpenChat(next));
        }
    }
    app.actions.extend(actions);
}

/// Handles keys while the image preview is open. No chat shortcut runs, and
/// typing and clipboard input are swallowed; Tab, Enter, Space and the arrows
/// stay for the preview's own controls.
fn preview_keys(app: &mut App, ctx: &egui::Context) {
    let mut actions = Vec::new();
    ctx.input_mut(|input| {
        if input.consume_key(Modifiers::NONE, Key::Escape) {
            actions.push(Action::CloseImagePreview);
        }
        let mut event_actions = Vec::new();
        for event in &input.events {
            if let egui::Event::Key {
                key,
                modifiers,
                pressed: true,
                ..
            } = event
            {
                event_actions.extend(crate::image_preview::preview_action(*key, *modifiers));
            }
        }
        actions.extend(event_actions);
        input
            .events
            .retain(|event| !crate::image_preview::consumes_key(event));
    });
    app.actions.extend(actions);
}

/// Shortcuts shown in the help dialog.
pub const SHORTCUTS: &[(&str, &str)] = &[
    ("Ctrl+F", "Search this chat or Settings"),
    ("Ctrl+K / Ctrl+Shift+F", "Search all chats"),
    ("Alt+↑ / Alt+↓", "Previous / next chat"),
    (
        "Escape",
        "Dismiss suggestions, cancel the current action, or return from search",
    ),
    ("Ctrl+V", "Paste text or attach copied files or a picture"),
    ("Ctrl+B", "Show or hide the chat list"),
    ("Ctrl+End", "Jump to the newest message"),
    ("Ctrl+,", "Settings"),
    ("Ctrl++ / Ctrl+-", "Zoom in / out"),
    ("Ctrl+0", "Reset zoom"),
    ("Ctrl+/", "This list"),
    ("Ctrl+W", "Close the window (Whatsapp remains in the tray)"),
    ("Ctrl+Q", "Quit"),
];

/// Uses Command and Option labels on macOS.
pub fn label(keys: &str) -> String {
    if cfg!(target_os = "macos") {
        keys.replace("Ctrl", "⌘").replace("Alt", "⌥")
    } else {
        keys.to_owned()
    }
}
