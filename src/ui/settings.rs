//! The settings page.

use egui::{CornerRadius, Frame, Margin};

use crate::app::App;
use crate::model::{Action, Dialog, Page};
use crate::settings::ThemeChoice;
use crate::theme::{self, Icon};

use super::widgets;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    super::standalone_header(app, ui);
    if theme::macos_chrome(ui.ctx()) {
        super::banner(app, ui);
    }
    let palette = app.palette;
    let mut filter = Filter::new(&app.settings_search);
    egui::ScrollArea::vertical()
        .id_salt("settings")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            Frame::new()
                .inner_margin(Margin::symmetric(32, 24))
                .show(ui, |ui| {
                    ui.set_max_width(640.0);
                    ui.horizontal(|ui| {
                        if theme::icon_button(
                            ui,
                            Icon::ArrowLeft,
                            20.0,
                            palette.secondary,
                            palette.text,
                            "Back (Esc)",
                        )
                        .clicked()
                        {
                            app.actions.push(Action::Open(Page::Chats));
                        }
                        theme::text(ui, "Settings", theme::bold(24.0), palette.text);
                    });
                    ui.add_space(12.0);
                    let search = ui.add(
                        egui::TextEdit::singleline(&mut app.settings_search)
                            .id(egui::Id::new("settings-search"))
                            .hint_text("Search settings")
                            .desired_width(f32::INFINITY),
                    );
                    if std::mem::take(&mut app.focus_settings_search) {
                        search.request_focus();
                    }
                    filter.query = app.settings_search.trim().to_lowercase();
                    ui.add_space(12.0);

                    filter.section("Appearance");
                    filter.row(ui, &palette, "Theme", "", |ui| {
                        for choice in ThemeChoice::ALL.iter().rev() {
                            let active = app.settings.theme == *choice;
                            if theme::soft_button(ui, &palette, None, choice.label(), active).clicked()
                                && !active
                            {
                                app.settings.theme = *choice;
                                app.actions.push(Action::SettingsChanged);
                            }
                        }
                    });
                    filter.row(
                        ui,
                        &palette,
                        "Zoom",
                        &super::keys::label("You can also use Ctrl+plus and Ctrl+minus."),
                        |ui| {
                            if theme::icon_button(ui, Icon::Plus, 16.0, palette.secondary, palette.text, "Larger").clicked() {
                                app.actions.push(Action::ZoomBy(0.1));
                            }
                            theme::text(
                                ui,
                                format!("{:.0}%", app.settings.zoom * 100.0),
                                theme::medium(13.5),
                                palette.text,
                            );
                            if theme::icon_button(ui, Icon::Minus, 16.0, palette.secondary, palette.text, "Smaller").clicked() {
                                app.actions.push(Action::ZoomBy(-0.1));
                            }
                        },
                    );

                    filter.section("Chats");
                    toggle(ui, app, &mut filter, "Enter sends", &super::keys::label("When off, Enter adds a line and Ctrl+Enter sends."), |settings| &mut settings.enter_sends);
                    let receipts_note = if app.account_receipts_off {
                        "Read receipts are disabled for your WhatsApp account. Direct chats will not send them. When this switch is on, groups still do. Read state syncs between your devices either way."
                    } else {
                        "Let people see when you read messages or play voice messages. Your WhatsApp privacy setting still applies. Read state syncs between your devices either way."
                    };
                    toggle(ui, app, &mut filter, "Send read receipts", receipts_note, |settings| &mut settings.send_read_receipts);
                    toggle(ui, app, &mut filter, "Show when you are typing", "", |settings| &mut settings.send_typing);
                    toggle(ui, app, &mut filter, "Download attachments automatically", "Download pictures, videos, voice messages, and documents up to 64 MB when they enter view. When off, click a file to download it.", |settings| &mut settings.auto_download);
                    toggle(ui, app, &mut filter, "Show sender pictures in every chat", "WhatsApp shows them in groups only.", |settings| &mut settings.show_sender_pictures);
                    toggle(ui, app, &mut filter, "Names from your address book", "Prefer saved contact names. When off, prefer public WhatsApp profile names. This applies throughout the app.", |settings| &mut settings.names_from_contacts);
                    toggle(ui, app, &mut filter, "Save contacts to the phone's address book", "Also add contacts saved here to your phone's address book. When off, they remain WhatsApp contacts. Names sync to linked devices either way.", |settings| &mut settings.save_contacts_to_phone);
                    toggle(ui, app, &mut filter, "Show shortcut hints", "Show keyboard tips when no chat is open.", |settings| &mut settings.show_shortcut_hints);

                    filter.section("Window");
                    toggle(ui, app, &mut filter, "Keep running when the window closes", "Keep Whatsapp linked in the menu bar. Quit from the menu or with ⌘Q.", |settings| &mut settings.keep_running_in_background);
                    toggle(ui, app, &mut filter, "Notify about new messages", "Show desktop notifications when the window is hidden, in the background, or showing another chat. Muted chats do not notify you.", |settings| &mut settings.notifications);
                    toggle(ui, app, &mut filter, "Check for updates", "Ask GitHub once a day whether a newer Whatsapp release exists. The request identifies only Whatsapp and its version.", |settings| &mut settings.check_for_updates);

                    filter.row(
                        ui,
                        &palette,
                        "GIPHY API key",
                        if crate::settings::BUILT_IN_GIPHY_KEY.is_some() {
                            "Used for GIF search. This build includes a key. Enter a key from developers.giphy.com to replace it."
                        } else {
                            "Required for GIF search. Get a free key from developers.giphy.com."
                        },
                        |ui| {
                            let response = ui.add(
                                egui::TextEdit::singleline(&mut app.settings.giphy_key)
                                    .font(theme::regular(13.0))
                                    .text_color(palette.text)
                                    .desired_width(220.0),
                            );
                            if response.changed() {
                                app.actions.push(Action::SettingsChanged);
                            }
                        },
                    );

                    filter.section("Account");
                    let name = app.me_name.clone().unwrap_or_default();
                    let me = app.me.clone().unwrap_or_default();
                    let phone = crate::model::phone_of(&me)
                        .map(crate::util::phone)
                        .unwrap_or_else(|| me.clone());
                    let description = match &app.me_about {
                        Some(about) => format!("{phone} · {about}"),
                        None => phone,
                    };
                    if filter.matches(&name, &description) {
                        if std::mem::take(&mut filter.heading_pending) {
                            section(ui, &palette, filter.section);
                        }
                        ui.horizontal(|ui| {
                            let picture = app.avatar_full(&me).or_else(|| app.avatar(&me));
                            widgets::avatar(ui, &palette, &name, &me, 56.0, picture.as_deref());
                        });
                        ui.add_space(6.0);
                    }
                    filter.row(
                        ui,
                        &palette,
                        if name.is_empty() { "Linked device" } else { &name },
                        &description,
                        |ui| {
                            if theme::soft_button(ui, &palette, Some(Icon::LogOut), "Unlink this computer", false).clicked() {
                                app.actions.push(Action::ShowDialog(Dialog::ConfirmUnlink));
                            }
                        },
                    );

                    filter.section("Files");
                    let archive = app.dirs.archive_db();
                    filter.row(
                        ui,
                        &palette,
                        "Message archive",
                        &archive.display().to_string(),
                        |ui| {
                            if theme::soft_button(ui, &palette, Some(Icon::ExternalLink), "Open folder", false).clicked() {
                                app.actions.push(Action::OpenFile(app.dirs.state.clone()));
                            }
                        },
                    );
                    let media = app.dirs.media_cache_dir();
                    filter.row(
                        ui,
                        &palette,
                        "Downloaded attachments",
                        &media.display().to_string(),
                        |ui| {
                            if theme::soft_button(ui, &palette, Some(Icon::ExternalLink), "Open folder", false).clicked() {
                                let _ = std::fs::create_dir_all(&media);
                                app.actions.push(Action::OpenFile(media.clone()));
                            }
                        },
                    );
                    let log = app.dirs.log_file();
                    filter.row(ui, &palette, "Log of this run", &log.display().to_string(), |ui| {
                        if theme::soft_button(ui, &palette, Some(Icon::FileText), "Open", false).clicked() {
                            app.actions.push(Action::OpenFile(log.clone()));
                        }
                    });

                    filter.section("About");
                    filter.row(
                        ui,
                        &palette,
                        &format!("Whatsapp {}", env!("CARGO_PKG_VERSION")),
                        "A native WhatsApp client built with Rust, egui, and whatsapp-rust.",
                        |ui| {
                            if theme::soft_button(ui, &palette, Some(Icon::Info), "About", false).clicked() {
                                app.actions.push(Action::ShowDialog(Dialog::About));
                            }
                            if theme::soft_button(ui, &palette, Some(Icon::Keyboard), "Shortcuts", false).clicked() {
                                app.actions.push(Action::ShowDialog(Dialog::Shortcuts));
                            }
                        },
                    );
                    if !filter.found {
                        widgets::rich_text(ui, "No settings match your search.", theme::regular(14.0), palette.secondary);
                    }
                });
        });
}

fn section(ui: &mut egui::Ui, palette: &theme::Palette, label: &str) {
    ui.add_space(10.0);
    Frame::new()
        .fill(palette.panel)
        .corner_radius(CornerRadius::same(theme::RADIUS))
        .inner_margin(Margin::symmetric(14, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::text(ui, label, theme::semibold(12.5), palette.accent);
        });
    ui.add_space(8.0);
}

fn toggle(
    ui: &mut egui::Ui,
    app: &mut App,
    filter: &mut Filter,
    label: &str,
    description: &str,
    field: impl Fn(&mut crate::settings::Settings) -> &mut bool,
) {
    let palette = app.palette;
    let mut value = *field(&mut app.settings);
    let mut changed = false;
    filter.row(ui, &palette, label, description, |ui| {
        changed = widgets::switch(ui, &palette, &mut value).changed();
    });
    if changed {
        *field(&mut app.settings) = value;
        app.actions.push(Action::SettingsChanged);
    }
}

struct Filter {
    query: String,
    section: &'static str,
    heading_pending: bool,
    found: bool,
}

impl Filter {
    fn new(query: &str) -> Self {
        Self {
            query: query.trim().to_lowercase(),
            section: "",
            heading_pending: false,
            found: false,
        }
    }

    fn section(&mut self, section: &'static str) {
        self.section = section;
        self.heading_pending = true;
    }

    fn matches(&self, label: &str, description: &str) -> bool {
        self.query.is_empty()
            || [self.section, label, description]
                .iter()
                .any(|text| text.to_lowercase().contains(&self.query))
    }

    fn row(
        &mut self,
        ui: &mut egui::Ui,
        palette: &theme::Palette,
        label: &str,
        description: &str,
        controls: impl FnOnce(&mut egui::Ui),
    ) {
        if !self.matches(label, description) {
            return;
        }
        if std::mem::take(&mut self.heading_pending) {
            section(ui, palette, self.section);
        }
        self.found = true;
        widgets::setting_row(ui, palette, label, description, controls);
    }
}

#[cfg(test)]
mod tests {
    use super::Filter;

    #[test]
    fn settings_search_matches_titles_descriptions_and_whole_sections() {
        let mut filter = Filter::new(" READ receipts ");
        filter.section("Chats");
        assert!(filter.matches("Send read receipts", "Privacy"));
        assert!(!filter.matches("Download attachments automatically", "Download pictures"));
        filter.query = "privacy".into();
        assert!(filter.matches("Send read receipts", "Your privacy setting applies"));
        filter.query = "chats".into();
        assert!(filter.matches("Enter sends", ""));
        filter.section("Window");
        assert!(!filter.matches("Check for updates", ""));
    }
}
