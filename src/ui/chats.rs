//! The left panel: the chat list.

mod archive_pull;

use egui::{Align, Frame, Layout, Margin, Rect, Sense, Vec2, pos2, vec2};

use self::archive_pull::ArchivePull;
use crate::app::App;
use crate::model::{Action, Chat, Contact, Dialog, Message, Page};
use crate::theme::{self, Icon, Palette};

use super::widgets;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let panel = egui::Panel::left("chats")
        .resizable(true)
        .default_size(app.settings.sidebar_width)
        .size_range(if theme::macos_chrome(ui.ctx()) {
            (theme::traffic_light_inset(ui.ctx()) + 210.0).max(280.0)..=520.0
        } else {
            260.0..=520.0
        })
        .show_separator_line(false)
        .frame(Frame::new().fill(palette.panel).inner_margin(Margin::ZERO));
    let response = panel.show(ui, |ui| {
        header(app, ui);
        list(app, ui);
    });
    let width = response.response.rect.width();
    if (width - app.settings.sidebar_width).abs() > 1.0 {
        app.settings.sidebar_width = width;
        app.actions.push(Action::SettingsChanged);
    }
    // Separate the panel from the conversation.
    let rect = response.response.rect;
    ui.painter().vline(
        rect.right(),
        rect.y_range(),
        egui::Stroke::new(1.0, palette.outline),
    );
}

fn header(app: &mut App, ui: &mut egui::Ui) {
    if theme::macos_chrome(ui.ctx()) {
        macos_header(app, ui);
        return;
    }
    let palette = app.palette;
    Frame::new()
        .inner_margin(Margin {
            left: 14,
            right: 10,
            top: 12,
            bottom: 8,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if app.show_archived {
                    if theme::icon_button(
                        ui,
                        Icon::ArrowLeft,
                        18.0,
                        palette.secondary,
                        palette.text,
                        "Back to chats",
                    )
                    .clicked()
                    {
                        app.show_archived = false;
                    }
                    theme::text(ui, "Archived", theme::bold(20.0), palette.text);
                } else {
                    let me = app.me.clone().unwrap_or_default();
                    let name = app.me_name.clone().unwrap_or_else(|| "You".to_owned());
                    let picture = app.avatar(&me);
                    let tooltip = match &app.me_about {
                        Some(about) => format!("{name}\n{about}"),
                        None => name.clone(),
                    };
                    let response =
                        widgets::avatar(ui, &palette, &name, &me, 34.0, picture.as_deref())
                            .interact(Sense::click())
                            .on_hover_text(tooltip)
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if response.clicked() {
                        app.actions.push(Action::Open(Page::Settings));
                    }
                    ui.add_space(2.0);
                    theme::text(ui, "Chats", theme::bold(20.0), palette.text);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if theme::icon_button(
                        ui,
                        Icon::Settings,
                        18.0,
                        palette.secondary,
                        palette.text,
                        "Settings (Ctrl+,)",
                    )
                    .clicked()
                    {
                        app.actions.push(Action::Open(Page::Settings));
                    }
                    if theme::icon_button(
                        ui,
                        Icon::SquarePen,
                        18.0,
                        palette.secondary,
                        palette.text,
                        "New contact",
                    )
                    .clicked()
                    {
                        app.actions
                            .push(Action::ShowDialog(crate::model::Dialog::NewContact));
                    }
                    if theme::icon_button(
                        ui,
                        Icon::PanelLeft,
                        18.0,
                        palette.secondary,
                        palette.text,
                        "Hide the chat list (Ctrl+B)",
                    )
                    .clicked()
                    {
                        app.actions.push(Action::ToggleSidebar);
                    }
                });
            });
            ui.add_space(6.0);
            let id = egui::Id::new("chat-search");
            let width = ui.available_width();
            let mut text = app.search.clone();
            let response = widgets::search_field(ui, &palette, id, &mut text, "Search", width);
            if text != app.search {
                app.actions.push(Action::Search(text));
            }
            if app.focus_search {
                app.focus_search = false;
                response.request_focus();
            }
        });
}

fn macos_header(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let inset = theme::traffic_light_inset(ui.ctx());
    let mut drag = ui.max_rect();
    drag.min.x += inset;
    drag.max.y = drag.min.y + 60.0;
    super::titlebar_drag(ui, drag);
    Frame::new()
        .inner_margin(Margin::symmetric(14, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_min_height(44.0);
                ui.add_space((inset - 14.0).max(0.0));
                if app.show_archived {
                    if theme::icon_button(
                        ui,
                        Icon::ArrowLeft,
                        18.0,
                        palette.secondary,
                        palette.text,
                        "Back to chats",
                    )
                    .clicked()
                    {
                        app.show_archived = false;
                    }
                    theme::text(ui, "Archived", theme::bold(16.0), palette.text);
                } else {
                    theme::text(ui, "Chats", theme::bold(20.0), palette.text);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if theme::icon_button(
                        ui,
                        Icon::SquarePen,
                        18.0,
                        palette.secondary,
                        palette.text,
                        "New contact (⌘N)",
                    )
                    .clicked()
                    {
                        app.actions.push(Action::ShowDialog(Dialog::NewContact));
                    }
                    if theme::icon_button(
                        ui,
                        Icon::PanelLeft,
                        18.0,
                        palette.secondary,
                        palette.text,
                        "Hide the chat list (⌘B)",
                    )
                    .clicked()
                    {
                        app.actions.push(Action::ToggleSidebar);
                    }
                });
            });
            ui.add_space(6.0);
            let mut text = app.search.clone();
            let response = widgets::search_field(
                ui,
                &palette,
                egui::Id::new("chat-search"),
                &mut text,
                "Search",
                ui.available_width(),
            );
            if text != app.search {
                app.actions.push(Action::Search(text));
            }
            if app.focus_search {
                app.focus_search = false;
                response.request_focus();
            }
        });
}

fn list(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    if !app.search.trim().is_empty() {
        results(app, ui);
        return;
    }
    let chats = app.visible_chat_indices();
    let archived = app.archived_count();
    let show_archive_row = !app.show_archived && archived > 0;
    // Keep the archive folder's scroll position separate from the main list.
    let scroll_salt = if app.show_archived {
        "archived-chat-list"
    } else {
        "chat-list"
    };
    let scroll_id = ui.make_persistent_id(egui::IdSalt::new(scroll_salt));
    let archive_row_id = scroll_id.with("archive-row-present");
    let pull_id = scroll_id.with("archive-pull");
    if chats.is_empty() && !show_archive_row {
        ui.ctx().data_mut(|data| {
            data.remove::<bool>(archive_row_id);
            data.remove::<ArchivePull>(pull_id);
        });
        let (title, body) = if app.show_archived {
            ("Nothing archived", "Archived chats appear here.")
        } else if app.syncing {
            ("Loading your chats", "Receiving history from your phone.")
        } else {
            (
                "No chats yet",
                "New chats appear here. You can start one from your phone.",
            )
        };
        widgets::empty_state(ui, &palette, Icon::MessageCircle, title, body);
        return;
    }
    let row_height = theme::ROW_HEIGHT;
    let spacing = ui.spacing().item_spacing.y;
    let row_stride = row_height + spacing;
    let total = chats.len() + usize::from(show_archive_row);
    let viewport_height = ui.available_height();
    let mut current = egui::scroll_area::State::load(ui.ctx(), scroll_id)
        .unwrap_or_default()
        .offset
        .y;
    let previous_archive_row = ui.ctx().data_mut(|data| {
        let previous = data.get_temp::<bool>(archive_row_id);
        data.insert_temp(archive_row_id, show_archive_row);
        previous
    });
    let mut pull = ui
        .ctx()
        .data(|data| data.get_temp::<ArchivePull>(pull_id))
        .unwrap_or_default();
    let mut scroll_area = egui::ScrollArea::vertical()
        .id_salt(scroll_salt)
        .scroll_source(egui::scroll_area::ScrollSource::ALL)
        .auto_shrink([false, false]);
    if previous_archive_row != Some(show_archive_row) {
        pull = ArchivePull::default();
        // The archive row starts just above the viewport. Adjust by one row
        // when the first chat is archived or the last one is unarchived, too,
        // so the visible conversations keep their positions.
        current = match previous_archive_row {
            None if show_archive_row => row_stride,
            Some(false) if show_archive_row => current + row_stride,
            Some(true) => (current - row_stride).max(0.0),
            _ => current,
        };
        scroll_area = scroll_area.vertical_scroll_offset(current);
    }
    if show_archive_row && chats.len() as f32 * row_stride - spacing <= viewport_height {
        // A short list still needs room to pull the archive into view, but
        // that extra room alone should not introduce a scrollbar.
        scroll_area =
            scroll_area.scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
    }
    let target_row = app.scroll_chat_into_view.as_ref().and_then(|target| {
        chats
            .iter()
            .position(|index| app.chats[*index].id == *target)
            .map(|index| index + usize::from(show_archive_row))
    });
    if let Some(target_row) = target_row {
        pull.hide();
        let offset = row_scroll_offset(current, viewport_height, target_row, row_height, spacing);
        scroll_area = scroll_area.vertical_scroll_offset(offset);
        current = offset;
        app.scroll_chat_into_view = None;
    }
    let mut pull_offset = None;
    if show_archive_row {
        let was_moving = pull.is_moving();
        pull_offset = pull.update(ui, scroll_id, current, row_stride);
        if let Some(offset) = pull_offset {
            if !was_moving {
                // Drop any earlier native momentum when the pull takes over.
                let mut state = egui::scroll_area::State::default();
                state.offset.y = offset;
                state.store(ui.ctx(), scroll_id);
            }
            // Preserve the list's drag identity and click suppression while
            // the resisted gesture owns movement instead of native scrolling.
            ui.interact(
                ui.available_rect_before_wrap(),
                scroll_id.with("area"),
                Sense::drag(),
            );
            scroll_area = scroll_area
                .scroll_source(egui::scroll_area::ScrollSource::NONE)
                .vertical_scroll_offset(offset);
        } else if !pull.revealed {
            scroll_area = scroll_area.vertical_scroll_offset(current.max(row_stride));
        }
    }
    let mut output = scroll_area.show_viewport(ui, |ui, viewport| {
        // Keep the usual visible-row virtualization, with at least one hidden
        // row of scrollable space even when the conversations fit on screen.
        let content_height = (total as f32 * row_stride - spacing).max(0.0);
        let minimum_height = if show_archive_row {
            viewport.height() + row_stride
        } else {
            0.0
        };
        ui.set_height(content_height.max(minimum_height));
        let first = ((viewport.min.y / row_stride).floor() as usize).min(total);
        let end = ((viewport.max.y / row_stride).ceil() as usize + 1).min(total);
        let top = ui.max_rect().top();
        let rows_rect = Rect::from_x_y_ranges(
            ui.max_rect().x_range(),
            (top + first as f32 * row_stride)..=(top + end as f32 * row_stride),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(rows_rect), |ui| {
            ui.skip_ahead_auto_ids(first);
            for index in first..end {
                if show_archive_row && index == 0 {
                    archive_row(app, ui, archived);
                    continue;
                }
                // Group membership and previews can be large. Snapshot only
                // the visible rows. Chat IDs keep menus stable after reordering.
                let chat = app.chats[chats[index - usize::from(show_archive_row)]].clone();
                ui.push_id(("chat", &chat.id), |ui| row(app, ui, &chat));
            }
        });
        if chats.is_empty() && show_archive_row {
            let rect = Rect::from_min_size(
                pos2(ui.max_rect().left(), top + row_stride),
                viewport.size(),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                widgets::empty_state(
                    ui,
                    &palette,
                    Icon::Archive,
                    "All chats are archived",
                    "Pull down to see your archived chats.",
                );
            });
        }
    });
    if show_archive_row {
        if pull_offset.is_none() && pull.revealed && output.state.offset.y >= row_stride - 0.5 {
            pull.hide();
        }
        if pull_offset.is_none() && !pull.revealed && output.state.offset.y < row_stride {
            // Stop at the first chat for the entire gesture, including its
            // smoothed/momentum tail. A new pull at this boundary unlocks it.
            output.state.offset.y = row_stride;
            output.state.store(ui.ctx(), scroll_id);
            ui.ctx().request_repaint();
        }
    }
    if show_archive_row && app.show_archived {
        // Entering the folder hides its row again for the return to Chats.
        // Reset momentum as well, so an earlier pull cannot reveal it again.
        let mut hidden = egui::scroll_area::State::default();
        hidden.offset.y = row_stride;
        hidden.store(ui.ctx(), scroll_id);
        pull = ArchivePull::default();
    }
    ui.ctx().data_mut(|data| data.insert_temp(pull_id, pull));
}

/// Returns the smallest offset that fully reveals a fixed-height row.
fn row_scroll_offset(
    current: f32,
    viewport_height: f32,
    row: usize,
    row_height: f32,
    spacing: f32,
) -> f32 {
    let top = row as f32 * (row_height + spacing);
    let bottom = top + row_height;
    if top < current {
        top
    } else if bottom > current + viewport_height {
        (bottom - viewport_height).max(0.0)
    } else {
        current
    }
}

/// Search results grouped into chats, messages, and contacts.
fn results(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let chats: Vec<Chat> = app.visible_chats().into_iter().cloned().collect();
    let hits: Vec<Message> = app.search_hits.clone();
    let contacts: Vec<Contact> = app.matching_contacts().into_iter().cloned().collect();
    if chats.is_empty() && hits.is_empty() && contacts.is_empty() {
        widgets::empty_state(
            ui,
            &palette,
            Icon::Search,
            "No results",
            "Try another name, number, or message text.",
        );
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("search-results")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if !chats.is_empty() {
                section(ui, &palette, "Chats");
                for chat in &chats {
                    let reveal = app.scroll_chat_into_view.as_deref() == Some(chat.id.as_str());
                    let response = ui
                        .push_id(("chat", &chat.id), |ui| row(app, ui, chat))
                        .inner;
                    if reveal {
                        response.scroll_to_me(None);
                        app.scroll_chat_into_view = None;
                    }
                }
            }
            if !hits.is_empty() {
                section(ui, &palette, "Messages");
                for hit in &hits {
                    ui.push_id(("hit", &hit.chat, &hit.id), |ui| hit_row(app, ui, hit));
                }
            }
            if !contacts.is_empty() {
                section(ui, &palette, "Contacts");
                for contact in &contacts {
                    ui.push_id(("contact", &contact.id), |ui| contact_row(app, ui, contact));
                }
            }
            ui.add_space(8.0);
        });
}

fn section(ui: &mut egui::Ui, palette: &Palette, label: &str) {
    ui.add_space(10.0);
    Frame::new()
        .inner_margin(Margin {
            left: 14,
            right: 14,
            top: 0,
            bottom: 4,
        })
        .show(ui, |ui| {
            theme::text(ui, label, theme::semibold(12.5), palette.accent);
        });
}

/// A message search result. Clicking it opens the chat at that message.
fn hit_row(app: &mut App, ui: &mut egui::Ui, hit: &Message) {
    let palette = app.palette;
    let title = match app.chat(&hit.chat) {
        Some(chat) => app.chat_title(&chat.clone()),
        None => app.display_name_or(&hit.chat, None),
    };
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), theme::ROW_HEIGHT),
        Sense::click(),
    );
    if ui.is_rect_visible(rect) {
        if response.hovered() {
            ui.painter().rect_filled(rect, 0.0, palette.surface_hover);
        }
        let avatar_rect =
            Rect::from_center_size(pos2(rect.left() + 38.0, rect.center().y), Vec2::splat(48.0));
        let picture = app.avatar(&hit.chat);
        widgets::paint_avatar(
            ui,
            &palette,
            avatar_rect,
            &title,
            &hit.chat,
            picture.as_deref(),
        );
        let left = rect.left() + 76.0;
        let right = rect.right() - 14.0;
        let stamp_galley = ui.painter().layout_no_wrap(
            crate::util::chat_stamp(hit.timestamp),
            theme::regular(11.5),
            palette.dim,
        );
        let name_top = rect.top() + 14.0;
        ui.painter().galley(
            pos2(right - stamp_galley.size().x, name_top + 1.0),
            stamp_galley.clone(),
            palette.dim,
        );
        let name_width = (right - stamp_galley.size().x - 8.0 - left).max(0.0);
        let name = widgets::line(ui, &title, theme::medium(14.5), palette.text, name_width, 1);
        name.paint(ui, pos2(left, name_top), palette.text);
        // Show the sender for group messages.
        let line_y = rect.top() + 38.0;
        let mut x = left;
        if hit.from_me {
            let who = widgets::line(
                ui,
                "You: ",
                theme::regular(13.0),
                palette.dim,
                (right - x) * 0.5,
                1,
            );
            who.paint(ui, pos2(x, line_y), palette.dim);
            x += who.size().x;
        } else if crate::model::ChatKind::from_id(&hit.chat) == crate::model::ChatKind::Group {
            let sender = app.display_name_or(&hit.sender, hit.sender_name.as_deref());
            let first = sender.split_whitespace().next().unwrap_or(&sender);
            let who = widgets::line(
                ui,
                &format!("{first}: "),
                theme::regular(13.0),
                palette.dim,
                (right - x) * 0.5,
                1,
            );
            who.paint(ui, pos2(x, line_y), palette.dim);
            x += who.size().x;
        }
        let words = widgets::line(
            ui,
            &crate::markup::plain(&app.resolve_mention_tokens(&hit.summary()), &[]),
            theme::regular(13.0),
            palette.dim,
            (right - x).max(0.0),
            1,
        );
        words.paint(ui, pos2(x, line_y), palette.dim);
        ui.painter().hline(
            left..=rect.right(),
            rect.bottom() - 0.5,
            egui::Stroke::new(1.0, palette.outline),
        );
    }
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        app.actions.push(Action::OpenMessage {
            chat: hit.chat.clone(),
            message: hit.id.clone(),
        });
    }
}

/// A contact without a chat. Clicking starts one.
fn contact_row(app: &mut App, ui: &mut egui::Ui, contact: &Contact) {
    let palette = app.palette;
    let name = contact
        .display_name()
        .map(str::to_owned)
        .unwrap_or_else(|| app.display_name_or(&contact.id, None));
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), theme::ROW_HEIGHT),
        Sense::click(),
    );
    if ui.is_rect_visible(rect) {
        if response.hovered() {
            ui.painter().rect_filled(rect, 0.0, palette.surface_hover);
        }
        let avatar_rect =
            Rect::from_center_size(pos2(rect.left() + 38.0, rect.center().y), Vec2::splat(48.0));
        let picture = app.avatar(&contact.id);
        widgets::paint_avatar(
            ui,
            &palette,
            avatar_rect,
            &name,
            &contact.id,
            picture.as_deref(),
        );
        let left = rect.left() + 76.0;
        let name_line = widgets::line(
            ui,
            &name,
            theme::medium(14.5),
            palette.text,
            rect.right() - 14.0 - left,
            1,
        );
        name_line.paint(ui, pos2(left, rect.top() + 14.0), palette.text);
        if let Some(phone) = crate::model::phone_of(&contact.id) {
            let phone_line = widgets::line(
                ui,
                &format!("+{phone}"),
                theme::regular(13.0),
                palette.dim,
                rect.right() - 14.0 - left,
                1,
            );
            phone_line.paint(ui, pos2(left, rect.top() + 38.0), palette.dim);
        }
        ui.painter().hline(
            left..=rect.right(),
            rect.bottom() - 0.5,
            egui::Stroke::new(1.0, palette.outline),
        );
    }
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        app.actions.push(Action::StartChat {
            id: contact.id.clone(),
            name,
        });
    }
}

fn archive_row(app: &mut App, ui: &mut egui::Ui, count: usize) {
    let palette = app.palette;
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), theme::ROW_HEIGHT),
        Sense::click(),
    );
    if ui.is_rect_visible(rect) {
        if response.hovered() {
            ui.painter().rect_filled(rect, 0.0, palette.surface_hover);
        }
        let icon_rect =
            Rect::from_center_size(pos2(rect.left() + 38.0, rect.center().y), Vec2::splat(22.0));
        Icon::Archive
            .image(palette.accent, 22.0)
            .paint_at(ui, icon_rect);
        ui.painter().text(
            pos2(rect.left() + 76.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            "Archived",
            theme::medium(14.5),
            palette.text,
        );
        ui.painter().text(
            pos2(rect.right() - 16.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            count.to_string(),
            theme::regular(12.5),
            palette.accent,
        );
        ui.painter().hline(
            (rect.left() + 76.0)..=rect.right(),
            rect.bottom() - 0.5,
            egui::Stroke::new(1.0, palette.outline),
        );
    }
    if response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        app.show_archived = true;
    }
}

fn row(app: &mut App, ui: &mut egui::Ui, chat: &Chat) -> egui::Response {
    let palette = app.palette;
    let title = app.chat_title(chat);
    let selected = app.open_chat.as_deref() == Some(chat.id.as_str());
    let now = crate::util::now();
    let muted = chat.muted(now);
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), theme::ROW_HEIGHT),
        Sense::click(),
    );
    if ui.is_rect_visible(rect) {
        if selected {
            ui.painter().rect_filled(rect, 0.0, palette.surface_active);
        } else if response.hovered() {
            ui.painter().rect_filled(rect, 0.0, palette.surface_hover);
        }
        let avatar_rect =
            Rect::from_center_size(pos2(rect.left() + 38.0, rect.center().y), Vec2::splat(48.0));
        let picture = app.avatar(&chat.id);
        widgets::paint_avatar(
            ui,
            &palette,
            avatar_rect,
            &title,
            &chat.id,
            picture.as_deref(),
        );

        let left = rect.left() + 76.0;
        let right = rect.right() - 14.0;
        let stamp = if chat.last_activity > 0 {
            crate::util::chat_stamp(chat.last_activity)
        } else {
            String::new()
        };
        let unread = chat.unread > 0;
        let stamp_color = if unread && !muted {
            palette.accent
        } else {
            palette.dim
        };
        let stamp_galley = ui
            .painter()
            .layout_no_wrap(stamp, theme::regular(11.5), stamp_color);
        let name_top = rect.top() + 14.0;
        ui.painter().galley(
            pos2(right - stamp_galley.size().x, name_top + 1.0),
            stamp_galley.clone(),
            stamp_color,
        );
        let name_width = (right - stamp_galley.size().x - 8.0 - left).max(0.0);
        let name_font = if unread {
            theme::semibold(14.5)
        } else {
            theme::medium(14.5)
        };
        let name = widgets::line(ui, &title, name_font, palette.text, name_width, 1);
        name.paint(ui, pos2(left, name_top), palette.text);

        // Leave room for badges beside the latest-message preview.
        let mut badge_right = right;
        let line_y = rect.top() + 38.0;
        if unread {
            let width = widgets::badge(
                ui,
                &palette,
                pos2(badge_right - 10.0, line_y + 8.0),
                chat.unread,
                muted,
            );
            badge_right -= width + 6.0;
        }
        if muted {
            let icon_rect =
                Rect::from_center_size(pos2(badge_right - 8.0, line_y + 8.0), Vec2::splat(15.0));
            Icon::VolumeX
                .image(palette.dim, 15.0)
                .paint_at(ui, icon_rect);
            badge_right -= 20.0;
        }
        if chat.pinned {
            let icon_rect =
                Rect::from_center_size(pos2(badge_right - 8.0, line_y + 8.0), Vec2::splat(14.0));
            Icon::Pin.image(palette.dim, 14.0).paint_at(ui, icon_rect);
            badge_right -= 20.0;
        }
        let mut x = left;
        let typing = app.typing_in(&chat.id);
        let preview_color = if unread && !muted {
            palette.secondary
        } else {
            palette.dim
        };
        let preview = if !typing.is_empty() {
            let who = if chat.is_group() {
                format!("{} is typing…", typing[0].1.trim_start_matches('~'))
            } else {
                "typing…".to_owned()
            };
            widgets::line(
                ui,
                &who,
                theme::medium(13.0),
                palette.accent,
                badge_right - x,
                1,
            )
        } else if let Some(last) = &chat.last {
            if last.from_me {
                let tick_rect =
                    Rect::from_center_size(pos2(x + 8.0, line_y + 8.0), Vec2::splat(16.0));
                widgets::ticks(ui, &palette, tick_rect, last.status);
                x += 20.0;
            } else if chat.is_group() {
                let sender = app.display_name_or(&last.sender, last.sender_name.as_deref());
                let first = sender.split_whitespace().next().unwrap_or(&sender);
                let sender = widgets::line(
                    ui,
                    &format!("{first}: "),
                    theme::regular(13.0),
                    preview_color,
                    (badge_right - x) * 0.5,
                    1,
                );
                let width = sender.size().x;
                sender.paint(ui, pos2(x, line_y), preview_color);
                x += width;
            }
            widgets::line(
                ui,
                &crate::markup::plain(&app.resolve_mention_tokens(&last.summary), &[]),
                theme::regular(13.0),
                preview_color,
                (badge_right - x).max(0.0),
                1,
            )
        } else {
            widgets::line(ui, "", theme::regular(13.0), preview_color, 1.0, 1)
        };
        preview.paint(ui, pos2(x, line_y), preview_color);
        ui.painter().hline(
            left..=rect.right(),
            rect.bottom() - 0.5,
            egui::Stroke::new(1.0, palette.outline),
        );
    }
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        app.actions.push(Action::OpenChat(chat.id.clone()));
    }
    let menu_palette = palette;
    egui::Popup::context_menu(&response)
        .frame(widgets::menu_frame(&menu_palette))
        .show(|ui| {
            ui.set_min_width(190.0);
            context_menu(app, ui, chat, &menu_palette);
        });
    response
}

fn context_menu(app: &mut App, ui: &mut egui::Ui, chat: &Chat, palette: &Palette) {
    if chat.unread > 0 && widgets::menu_item(ui, palette, Some(Icon::CheckCheck), "Mark as read") {
        app.actions.push(Action::MarkRead(chat.id.clone()));
    }
    if widgets::menu_item(
        ui,
        palette,
        Some(if chat.pinned { Icon::PinOff } else { Icon::Pin }),
        if chat.pinned { "Unpin" } else { "Pin to top" },
    ) {
        app.actions
            .push(Action::SetPinned(chat.id.clone(), !chat.pinned));
    }
    if widgets::menu_item(
        ui,
        palette,
        Some(Icon::Archive),
        if chat.archived {
            "Unarchive"
        } else {
            "Archive"
        },
    ) {
        app.actions
            .push(Action::SetArchived(chat.id.clone(), !chat.archived));
    }
    let now = crate::util::now();
    if chat.muted(now) {
        if widgets::menu_item(ui, palette, Some(Icon::Bell), "Unmute") {
            app.actions.push(Action::SetMuted(chat.id.clone(), None));
        }
    } else {
        for (label, until) in [
            ("Mute for 8 hours", Some(now + 8 * 3600)),
            ("Mute for a week", Some(now + 7 * 86_400)),
            ("Mute indefinitely", Some(0)),
        ] {
            if widgets::menu_item(ui, palette, Some(Icon::BellOff), label) {
                app.actions.push(Action::SetMuted(chat.id.clone(), until));
            }
        }
    }
    widgets::menu_separator(ui, palette);
    if let Some(phone) = chat.phone()
        && widgets::menu_item(ui, palette, Some(Icon::Copy), "Copy number")
    {
        app.actions.push(Action::CopyText(format!("+{phone}")));
    }
    if widgets::menu_item(ui, palette, Some(Icon::Info), "Info") {
        app.actions
            .push(Action::ShowDialog(Dialog::ChatInfo(chat.id.clone())));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AppDirs;
    use crate::settings::Settings;

    fn archive_list(active: usize) -> (App, egui::Context) {
        let root = std::env::temp_dir().join(format!(
            "whatsapp-archive-pull-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let (mut app, _events) = App::headless(AppDirs::under(&root), Settings::default());
        app.chats = (0..active)
            .map(|index| {
                let mut chat = Chat::new(format!("{index}@g.us"), format!("Chat {index:03}"));
                chat.last_activity = (active - index) as i64;
                chat
            })
            .collect();
        let mut archived = Chat::new("saved@g.us".into(), "Saved chat".into());
        archived.archived = true;
        app.chats.push(archived);
        let ctx = egui::Context::default();
        app.attach(&ctx);
        (app, ctx)
    }

    fn list_frame(
        app: &mut App,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        list_frame_with_focus(app, ctx, events, true)
    }

    fn list_frame_with_focus(
        app: &mut App,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        focused: bool,
    ) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(360.0, 240.0))),
                time: Some(ctx.cumulative_pass_nr() as f64 / 60.0),
                events,
                focused,
                ..Default::default()
            },
            |ui| list(app, ui),
        );
        output.textures_delta.clear();
        output
    }

    fn visible_text(output: &egui::FullOutput, label: &str) -> Option<Rect> {
        output.shapes.iter().find_map(|clipped| {
            if let egui::Shape::Text(text) = &clipped.shape
                && text.galley.job.text == label
            {
                let rect = Rect::from_min_size(text.pos, text.galley.size());
                if clipped.clip_rect.intersect(rect).is_positive() {
                    return Some(rect);
                }
            }
            None
        })
    }

    fn wheel(pos: egui::Pos2, delta: Vec2) -> Vec<egui::Event> {
        wheel_phase(pos, delta, egui::TouchPhase::Move)
    }

    fn wheel_phase(pos: egui::Pos2, delta: Vec2, phase: egui::TouchPhase) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta,
                modifiers: egui::Modifiers::NONE,
                phase,
            },
        ]
    }

    fn pointer(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    #[test]
    fn short_pull_tracks_the_pointer_with_resistance_then_springs_closed() {
        let (mut app, ctx) = archive_list(100);
        let start = pos2(180.0, 80.0);
        list_frame(&mut app, &ctx, vec![]);
        let output = list_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(start)]);
        let top = visible_text(&output, "Chat 000").unwrap().top();
        list_frame(&mut app, &ctx, pointer(start, true));
        let mut previous = 0.0;
        let mut previous_step = f32::INFINITY;
        for distance in [20.0, 40.0, 60.0] {
            let output = list_frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerMoved(start + vec2(0.0, distance))],
            );
            let displacement = visible_text(&output, "Chat 000").unwrap().top() - top;
            let step = displacement - previous;
            assert!(
                step > 0.0 && step < 20.0,
                "the list follows with resistance"
            );
            assert!(step <= previous_step, "resistance increases with distance");
            previous = displacement;
            previous_step = step;
        }
        let output = list_frame(&mut app, &ctx, pointer(start + vec2(0.0, 60.0), false));
        let released = visible_text(&output, "Chat 000").unwrap().top() - top;
        assert!(
            released > 0.0 && released < previous,
            "release must not snap shut"
        );
        for _ in 0..60 {
            let output = list_frame(&mut app, &ctx, vec![]);
            let displacement = visible_text(&output, "Chat 000").unwrap().top() - top;
            assert!(displacement >= 0.0 && displacement <= previous);
            previous = displacement;
        }
        let output = list_frame(&mut app, &ctx, vec![]);
        assert_eq!(visible_text(&output, "Chat 000").unwrap().top(), top);
        assert!(visible_text(&output, "Archived").is_none());
        assert!(app.actions.is_empty());
        assert!(
            output.viewport_output[&egui::ViewportId::ROOT]
                .repaint_delay
                .as_secs()
                > 1
        );
    }

    #[test]
    fn full_pull_settles_open_and_cancelled_pull_settles_closed() {
        use egui::TouchPhase::{Cancel, End, Move, Start};

        for end in [End, Cancel] {
            let (mut app, ctx) = archive_list(1);
            list_frame(&mut app, &ctx, vec![]);
            let output = list_frame(&mut app, &ctx, vec![]);
            let top = visible_text(&output, "Chat 000").unwrap().top();
            let pos = pos2(180.0, 100.0);
            list_frame(&mut app, &ctx, wheel_phase(pos, Vec2::ZERO, Start));
            let output = list_frame(&mut app, &ctx, wheel_phase(pos, vec2(0.0, 120.0), Move));
            let pulled = visible_text(&output, "Chat 000").unwrap().top() - top;
            assert!(pulled > 0.0 && pulled < theme::ROW_HEIGHT);
            // A trackpad held in place must not settle until the fingers lift.
            for _ in 0..30 {
                let output = list_frame(&mut app, &ctx, vec![]);
                assert_eq!(
                    visible_text(&output, "Chat 000").unwrap().top() - top,
                    pulled
                );
            }
            let output = list_frame(&mut app, &ctx, wheel_phase(pos, Vec2::ZERO, end));
            let released = visible_text(&output, "Chat 000").unwrap().top() - top;
            if end == End {
                assert!(released > pulled && released < theme::ROW_HEIGHT);
            } else {
                assert!(released > 0.0 && released < pulled);
            }
            for _ in 0..60 {
                list_frame(&mut app, &ctx, vec![]);
            }
            let output = list_frame(&mut app, &ctx, vec![]);
            assert_eq!(visible_text(&output, "Archived").is_some(), end == End);
            let settled = visible_text(&output, "Chat 000").unwrap().top() - top;
            if end == End {
                assert_eq!(
                    settled,
                    theme::ROW_HEIGHT + ctx.global_style().spacing.item_spacing.y
                );
            } else {
                assert_eq!(settled, 0.0);
            }
            assert!(
                output.viewport_output[&egui::ViewportId::ROOT]
                    .repaint_delay
                    .as_secs()
                    > 1
            );
            assert!(app.actions.is_empty());
        }
    }

    #[test]
    fn a_plain_wheel_burst_settles_without_an_end_event() {
        for distance in [30.0, 120.0] {
            let (mut app, ctx) = archive_list(1);
            for _ in 0..3 {
                list_frame(&mut app, &ctx, vec![]);
            }
            list_frame(
                &mut app,
                &ctx,
                wheel(pos2(180.0, 100.0), vec2(0.0, distance)),
            );
            for _ in 0..90 {
                list_frame(&mut app, &ctx, vec![]);
            }
            let output = list_frame(&mut app, &ctx, vec![]);
            assert_eq!(
                visible_text(&output, "Archived").is_some(),
                distance > 100.0
            );
            assert!(
                output.viewport_output[&egui::ViewportId::ROOT]
                    .repaint_delay
                    .as_secs()
                    > 1
            );
        }
    }

    #[test]
    fn a_pointer_pull_can_start_without_keyboard_focus() {
        let (mut app, ctx) = archive_list(100);
        let start = pos2(180.0, 80.0);
        let frame = |app: &mut App, events| list_frame_with_focus(app, &ctx, events, false);
        frame(&mut app, vec![]);
        frame(&mut app, vec![egui::Event::PointerMoved(start)]);
        frame(&mut app, pointer(start, true));
        for distance in [30.0, 60.0, 90.0, 120.0] {
            frame(
                &mut app,
                vec![egui::Event::PointerMoved(start + vec2(0.0, distance))],
            );
        }
        frame(&mut app, pointer(start + vec2(0.0, 120.0), false));
        for _ in 0..60 {
            frame(&mut app, vec![]);
        }
        let output = frame(&mut app, vec![]);
        assert!(visible_text(&output, "Archived").is_some());
        assert!(app.actions.is_empty());
    }

    #[test]
    fn moving_to_the_press_position_is_not_counted_as_a_pull() {
        let (mut app, ctx) = archive_list(100);
        list_frame(&mut app, &ctx, vec![]);
        let output = list_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(pos2(180.0, 20.0))],
        );
        let top = visible_text(&output, "Chat 000").unwrap().top();
        let output = list_frame(&mut app, &ctx, pointer(pos2(180.0, 180.0), true));
        assert_eq!(visible_text(&output, "Chat 000").unwrap().top(), top);
        list_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(pos2(180.0, 200.0))],
        );
        list_frame(&mut app, &ctx, pointer(pos2(180.0, 200.0), false));
        for _ in 0..60 {
            list_frame(&mut app, &ctx, vec![]);
        }
        let output = list_frame(&mut app, &ctx, vec![]);
        assert_eq!(visible_text(&output, "Chat 000").unwrap().top(), top);
        assert!(visible_text(&output, "Archived").is_none());
    }

    #[test]
    fn a_press_and_drag_batched_in_one_frame_preserve_the_drag_distance() {
        for release_in_same_frame in [false, true] {
            for distance in [50.0, 130.0] {
                let (mut app, ctx) = archive_list(100);
                list_frame(&mut app, &ctx, vec![]);
                list_frame(
                    &mut app,
                    &ctx,
                    vec![egui::Event::PointerMoved(pos2(180.0, 20.0))],
                );
                let start = pos2(180.0, 85.0);
                let end = start + vec2(0.0, distance);
                let mut events = pointer(start, true);
                events.push(egui::Event::PointerMoved(end));
                if release_in_same_frame {
                    events.extend(pointer(end, false));
                }
                list_frame(&mut app, &ctx, events);
                if !release_in_same_frame {
                    list_frame(&mut app, &ctx, pointer(end, false));
                }
                for _ in 0..60 {
                    list_frame(&mut app, &ctx, vec![]);
                }
                let output = list_frame(&mut app, &ctx, vec![]);
                assert_eq!(
                    visible_text(&output, "Archived").is_some(),
                    distance > 100.0
                );
                assert!(app.actions.is_empty());
            }
        }
    }

    #[test]
    fn archive_row_is_hidden_until_pulled_and_hides_when_scrolled_away() {
        // Include lists that fit, lists that need virtualization, and a list
        // whose only conversation is archived.
        for active in [0, 1, 100] {
            let (mut app, ctx) = archive_list(active);
            let output = list_frame(&mut app, &ctx, vec![]);
            assert!(visible_text(&output, "Archived").is_none());
            if active > 0 {
                assert!(visible_text(&output, "Chat 000").is_some());
            } else {
                assert!(visible_text(&output, "All chats are archived").is_some());
            }
            list_frame(&mut app, &ctx, vec![]);
            list_frame(&mut app, &ctx, wheel(pos2(180.0, 100.0), vec2(0.0, 120.0)));
            let output = list_frame(&mut app, &ctx, vec![]);
            assert!(
                visible_text(&output, "Archived").is_some(),
                "{active} chats"
            );
            list_frame(&mut app, &ctx, wheel(pos2(180.0, 100.0), vec2(0.0, -160.0)));
            for _ in 0..20 {
                list_frame(&mut app, &ctx, vec![]);
            }
            let output = list_frame(&mut app, &ctx, vec![]);
            assert!(visible_text(&output, "Archived").is_none());
            assert!(!app.show_archived);
            assert!(app.actions.is_empty());
        }
    }

    #[test]
    fn returning_to_the_top_keeps_archive_hidden_until_a_new_pull() {
        let (mut app, ctx) = archive_list(100);
        for _ in 0..3 {
            list_frame(&mut app, &ctx, vec![]);
        }
        let pos = pos2(180.0, 100.0);
        list_frame(&mut app, &ctx, wheel(pos, vec2(0.0, 120.0)));
        let output = list_frame(&mut app, &ctx, vec![]);
        assert!(visible_text(&output, "Archived").is_some());
        list_frame(&mut app, &ctx, wheel(pos, vec2(0.0, -450.0)));
        for _ in 0..30 {
            list_frame(&mut app, &ctx, vec![]);
        }
        // This gesture begins below the top and reaches it. Continuing to
        // pull during the same gesture must not expose the archive folder.
        for _ in 0..12 {
            list_frame(&mut app, &ctx, wheel(pos, vec2(0.0, 120.0)));
            let output = list_frame(&mut app, &ctx, vec![]);
            assert!(visible_text(&output, "Archived").is_none());
        }
        let output = list_frame(&mut app, &ctx, vec![]);
        assert!(visible_text(&output, "Chat 000").is_some());
        for _ in 0..30 {
            list_frame(&mut app, &ctx, vec![]);
        }
        list_frame(&mut app, &ctx, wheel(pos, vec2(0.0, 120.0)));
        let output = list_frame(&mut app, &ctx, vec![]);
        assert!(visible_text(&output, "Archived").is_some());
    }

    #[test]
    fn trackpad_momentum_cannot_turn_a_return_scroll_into_a_new_pull() {
        use egui::TouchPhase::{End, Move, Start};

        let (mut app, ctx) = archive_list(100);
        for _ in 0..3 {
            list_frame(&mut app, &ctx, vec![]);
        }
        let pos = pos2(180.0, 100.0);
        for (phase, dy) in [(Start, 0.0), (Move, 120.0), (End, 0.0)] {
            list_frame(&mut app, &ctx, wheel_phase(pos, vec2(0.0, dy), phase));
        }
        let output = list_frame(&mut app, &ctx, vec![]);
        assert!(visible_text(&output, "Archived").is_some());
        for (phase, dy) in [(Start, 0.0), (Move, -450.0), (End, 0.0)] {
            list_frame(&mut app, &ctx, wheel_phase(pos, vec2(0.0, dy), phase));
        }
        // Return to the top, then the OS supplies a second Start for momentum.
        for (phase, dy) in [
            (Start, 0.0),
            (Move, 600.0),
            (Move, 100.0),
            (End, 0.0),
            (Start, 0.0),
            (Move, 80.0),
            (Move, 20.0),
            (End, 0.0),
        ] {
            list_frame(&mut app, &ctx, wheel_phase(pos, vec2(0.0, dy), phase));
            let output = list_frame(&mut app, &ctx, vec![]);
            assert!(visible_text(&output, "Archived").is_none());
            if dy == 600.0 {
                // Pausing with fingers still down does not start a new gesture.
                for _ in 0..30 {
                    list_frame(&mut app, &ctx, vec![]);
                }
            }
        }
        for _ in 0..15 {
            list_frame(&mut app, &ctx, vec![]);
        }
        for (phase, dy) in [(Start, 0.0), (Move, 120.0), (End, 0.0)] {
            list_frame(&mut app, &ctx, wheel_phase(pos, vec2(0.0, dy), phase));
        }
        let output = list_frame(&mut app, &ctx, vec![]);
        assert!(visible_text(&output, "Archived").is_some());
    }

    #[test]
    fn a_drag_from_below_the_top_requires_release_before_revealing_archive() {
        let (mut app, ctx) = archive_list(100);
        for _ in 0..3 {
            list_frame(&mut app, &ctx, vec![]);
        }
        list_frame(&mut app, &ctx, wheel(pos2(180.0, 100.0), vec2(0.0, -120.0)));
        for _ in 0..30 {
            list_frame(&mut app, &ctx, vec![]);
        }
        // Move before pressing so the repositioning is not part of the drag.
        list_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(pos2(180.0, 35.0))],
        );
        list_frame(&mut app, &ctx, pointer(pos2(180.0, 35.0), true));
        for y in [215.0, 225.0, 235.0] {
            list_frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerMoved(pos2(180.0, y))],
            );
            let output = list_frame(&mut app, &ctx, vec![]);
            assert!(visible_text(&output, "Archived").is_none());
        }
        list_frame(&mut app, &ctx, pointer(pos2(180.0, 235.0), false));
        for _ in 0..40 {
            list_frame(&mut app, &ctx, vec![]);
        }
        let output = list_frame(&mut app, &ctx, vec![]);
        assert!(visible_text(&output, "Chat 000").is_some());
        list_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(pos2(180.0, 85.0))],
        );
        list_frame(&mut app, &ctx, pointer(pos2(180.0, 85.0), true));
        list_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(pos2(180.0, 195.0))],
        );
        list_frame(&mut app, &ctx, pointer(pos2(180.0, 195.0), false));
        let output = list_frame(&mut app, &ctx, vec![]);
        assert!(visible_text(&output, "Archived").is_some());
        assert!(app.actions.is_empty());
    }

    #[test]
    fn dragging_down_reveals_archive_without_opening_a_chat() {
        for active in [0, 1, 100] {
            let (mut app, ctx) = archive_list(active);
            for _ in 0..3 {
                list_frame(&mut app, &ctx, vec![]);
            }
            list_frame(&mut app, &ctx, pointer(pos2(180.0, 85.0), true));
            list_frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerMoved(pos2(180.0, 195.0))],
            );
            list_frame(&mut app, &ctx, pointer(pos2(180.0, 195.0), false));
            let output = list_frame(&mut app, &ctx, vec![]);
            assert!(
                visible_text(&output, "Archived").is_some(),
                "{active} chats"
            );
            assert!(app.actions.is_empty(), "a pull must not open a chat");
            assert!(!app.show_archived);
        }
    }

    #[test]
    fn archive_opens_at_its_first_chat_and_is_hidden_on_return() {
        let (mut app, ctx) = archive_list(100);
        for _ in 0..3 {
            list_frame(&mut app, &ctx, vec![]);
        }
        list_frame(&mut app, &ctx, wheel(pos2(180.0, 100.0), vec2(0.0, 120.0)));
        list_frame(&mut app, &ctx, vec![]);
        let output = list_frame(&mut app, &ctx, vec![]);
        let archive = visible_text(&output, "Archived").unwrap().center();
        for pressed in [true, false] {
            list_frame(&mut app, &ctx, pointer(archive, pressed));
        }
        assert!(app.show_archived);
        let output = list_frame(&mut app, &ctx, vec![]);
        assert!(visible_text(&output, "Saved chat").is_some());
        assert!(visible_text(&output, "Chat 000").is_none());
        app.show_archived = false;
        let output = list_frame(&mut app, &ctx, vec![]);
        assert!(visible_text(&output, "Archived").is_none());
        assert!(visible_text(&output, "Chat 000").is_some());
    }

    #[test]
    fn horizontal_and_outside_scrolling_do_not_reveal_archive() {
        let (mut app, ctx) = archive_list(1);
        for _ in 0..3 {
            list_frame(&mut app, &ctx, vec![]);
        }
        for (pos, delta) in [
            (pos2(180.0, 100.0), vec2(120.0, 0.0)),
            (pos2(500.0, 100.0), vec2(0.0, 120.0)),
        ] {
            list_frame(&mut app, &ctx, wheel(pos, delta));
            let output = list_frame(&mut app, &ctx, vec![]);
            assert!(visible_text(&output, "Archived").is_none());
        }
    }

    #[test]
    fn archive_count_changes_preserve_the_top_chat_position() {
        let (mut app, ctx) = archive_list(1);
        let archive = app.chats.pop().unwrap();
        let before = list_frame(&mut app, &ctx, vec![]);
        let top = visible_text(&before, "Chat 000").unwrap().top();
        app.chats.push(archive);
        let added = list_frame(&mut app, &ctx, vec![]);
        assert_eq!(visible_text(&added, "Chat 000").unwrap().top(), top);
        assert!(visible_text(&added, "Archived").is_none());
        app.chats.pop();
        let removed = list_frame(&mut app, &ctx, vec![]);
        assert_eq!(visible_text(&removed, "Chat 000").unwrap().top(), top);
    }

    #[test]
    fn clicks_follow_chat_ids_after_reordering_and_archiving() {
        let root = std::env::temp_dir().join(format!(
            "whatsapp-chat-clicks-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let (mut app, _events) = App::headless(AppDirs::under(&root), Settings::default());
        app.chats = (0..100)
            .map(|index| {
                let mut chat = Chat::new(format!("{index}@g.us"), format!("Group {index:03}"));
                chat.last_activity = 100 - index;
                chat.participants = (0..64).map(|member| format!("{member}@lid")).collect();
                chat
            })
            .collect();
        let ctx = egui::Context::default();
        app.attach(&ctx);
        let frame = |app: &mut App, events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(360.0, 240.0))),
                    events,
                    ..Default::default()
                },
                |ui| list(app, ui),
            );
            output.textures_delta.clear();
            output
        };
        frame(&mut app, vec![]);
        // Move a previously offscreen group to the top; indices change too.
        app.chats.reverse();
        app.chats
            .iter_mut()
            .find(|chat| chat.id == "99@g.us")
            .unwrap()
            .pinned = true;
        app.chats
            .iter_mut()
            .find(|chat| chat.id == "0@g.us")
            .unwrap()
            .archived = true;
        for archived in [false, true] {
            app.show_archived = archived;
            let (name, id) = if archived {
                ("Group 000", "0@g.us")
            } else {
                ("Group 099", "99@g.us")
            };
            frame(&mut app, vec![]);
            let output = frame(&mut app, vec![]);
            let pos = output
                .shapes
                .iter()
                .find_map(|clipped| {
                    if let egui::Shape::Text(text) = &clipped.shape
                        && text.galley.job.text == name
                    {
                        Some(text.pos + text.galley.size() / 2.0)
                    } else {
                        None
                    }
                })
                .expect("destination is visible");
            for pressed in [true, false] {
                frame(
                    &mut app,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            assert_eq!(app.actions, vec![Action::OpenChat(id.into())]);
            app.actions.clear();
        }
    }

    #[test]
    fn alt_navigation_scrolls_the_destination_chat_into_view() {
        let root = std::env::temp_dir().join(format!(
            "whatsapp-chat-list-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let (mut app, _events) = App::headless(AppDirs::under(&root), Settings::default());
        let mut first = String::new();
        let mut last = String::new();
        for index in 0..24 {
            let id = format!("49170000{index:04}@s.whatsapp.net");
            let mut chat = Chat::new(id.clone(), format!("Chat {index:02}"));
            chat.last_activity = 100 - i64::from(index);
            if index == 0 {
                first.clone_from(&id);
            }
            last.clone_from(&id);
            app.chats.push(chat);
        }
        app.open_chat = Some(first);

        let ctx = egui::Context::default();
        app.attach(&ctx);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(360.0, 240.0))),
            events: vec![egui::Event::Key {
                key: egui::Key::ArrowUp,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::ALT,
            }],
            ..Default::default()
        };
        let mut offset = 0.0;
        let mut output = ctx.run_ui(input, |ui| {
            super::super::keys::handle(&mut app, ui.ctx());
            let scroll_id = ui.make_persistent_id(egui::IdSalt::new("chat-list"));
            list(&mut app, ui);
            offset = egui::scroll_area::State::load(ui.ctx(), scroll_id)
                .expect("chat-list scroll state")
                .offset
                .y;
        });
        output.textures_delta.clear();

        assert!(
            app.actions.contains(&Action::OpenChat(last)),
            "Alt+Up wraps to the last visible chat"
        );
        assert!(app.scroll_chat_into_view.is_none(), "reveal was consumed");
        assert!(offset > 0.0, "the list moved down to reveal the last row");
    }
}
