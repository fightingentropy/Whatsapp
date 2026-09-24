//! Search the local archive without leaving the conversation.

use egui::{Align, Frame, Layout, Margin, Sense, vec2};
use jiff::{Span, civil::Date};

use super::widgets;
use crate::{
    app::App,
    model::{Action, Content},
    theme::{self, Icon},
};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    if app.chat_search.chat.is_none() {
        return;
    }
    let palette = app.palette;
    egui::Panel::right("chat-search-pane")
        .default_size(310.0)
        .size_range(240.0..=420.0)
        .resizable(true)
        .frame(
            Frame::new()
                .fill(palette.panel)
                .inner_margin(Margin::same(14)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                theme::text(ui, "Search this chat", theme::semibold(16.0), palette.text);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if theme::icon_button(
                        ui,
                        Icon::X,
                        18.0,
                        palette.secondary,
                        palette.text,
                        "Close search (Esc)",
                    )
                    .clicked()
                    {
                        app.actions.push(Action::CloseChatSearch);
                    }
                });
            });
            ui.add_space(10.0);
            let response = ui.add(
                egui::TextEdit::singleline(&mut app.chat_search.query)
                    .id(egui::Id::new("conversation-search"))
                    .hint_text("Search messages")
                    .desired_width(f32::INFINITY),
            );
            if std::mem::take(&mut app.chat_search.focus) {
                response.request_focus();
            }
            if response.changed() {
                app.actions
                    .push(Action::SearchChat(app.chat_search.query.clone()));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let label = app.chat_search.day.map_or_else(
                    || "Any date".into(),
                    |day| day.strftime("%d %b %Y").to_string(),
                );
                let calendar_button = theme::soft_button(
                    ui,
                    &palette,
                    Some(Icon::Calendar),
                    &label,
                    app.chat_search.day.is_some(),
                );
                egui::Popup::menu(&calendar_button).show(|ui| calendar(app, ui));
                if app.chat_search.day.is_some()
                    && theme::icon_button(
                        ui,
                        Icon::X,
                        14.0,
                        palette.secondary,
                        palette.text,
                        "Clear date",
                    )
                    .clicked()
                {
                    app.actions.push(Action::SearchChatDay(None));
                }
            });
            ui.add_space(12.0);
            let empty = if let Some(error) = &app.chat_search.error {
                Some(error.as_str())
            } else if app.chat_search.pending {
                Some("Searching…")
            } else if app.chat_search.query.trim().is_empty() && app.chat_search.day.is_none() {
                Some("Search messages or choose a day.")
            } else if app.chat_search.hits.is_empty() {
                Some("No messages found.")
            } else {
                None
            };
            if let Some(text) = empty {
                widgets::rich_text(ui, text, theme::regular(13.0), palette.secondary);
                return;
            }
            let count = app.chat_search.hits.len();
            let label = if app.chat_search.truncated {
                format!("Newest {count} matches · narrow your search for more")
            } else {
                format!(
                    "{count} {}",
                    if count == 1 { "message" } else { "messages" }
                )
            };
            widgets::rich_text(ui, &label, theme::regular(12.0), palette.secondary);
            ui.add_space(8.0);
            let mut select = None;
            egui::ScrollArea::vertical()
                .id_salt("chat-search-results")
                .show_rows(ui, 76.0, count, |ui, range| {
                    for index in range {
                        let message = &app.chat_search.hits[index];
                        let text = matching_line(&message.content, &app.chat_search.query);
                        let text = crate::markup::plain(&app.resolve_mention_tokens(&text), &[]);
                        let (rect, response) = ui
                            .allocate_exact_size(vec2(ui.available_width(), 76.0), Sense::click());
                        let selected = app.chat_search.selected == Some(index);
                        if selected || response.hovered() || response.has_focus() {
                            ui.painter().rect_filled(
                                rect,
                                6.0,
                                if selected {
                                    palette.surface_active
                                } else {
                                    palette.surface_hover
                                },
                            );
                        }
                        let sender = if message.from_me {
                            "You".into()
                        } else {
                            app.display_name_or(&message.sender, message.sender_name.as_deref())
                        };
                        let stamp = format!(
                            "{} · {}",
                            sender,
                            crate::util::moment_stamp(message.timestamp)
                        );
                        let line = widgets::line(
                            ui,
                            &stamp,
                            theme::regular(11.5),
                            palette.dim,
                            rect.width() - 12.0,
                            1,
                        );
                        line.paint(ui, rect.min + vec2(6.0, 7.0), palette.dim);
                        let line = widgets::line(
                            ui,
                            &text,
                            theme::regular(13.0),
                            palette.text,
                            rect.width() - 12.0,
                            2,
                        );
                        line.paint(ui, rect.min + vec2(6.0, 28.0), palette.text);
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::SelectableLabel,
                                ui.is_enabled(),
                                selected,
                                format!("{stamp}: {text}"),
                            )
                        });
                        if response.clicked() {
                            select = Some(index);
                        }
                    }
                });
            if let Some(index) = select {
                app.chat_search.selected = Some(index);
                app.actions
                    .push(Action::ScrollTo(app.chat_search.hits[index].id.clone()));
            }
        });
}

fn calendar(app: &mut App, ui: &mut egui::Ui) {
    let id = ui.id().with("search-month");
    let current = app
        .chat_search
        .day
        .unwrap_or_else(|| jiff::Zoned::now().date())
        .first_of_month();
    let mut month = ui.data_mut(|data| *data.get_temp_mut_or(id, current));
    ui.set_min_width(238.0);
    ui.horizontal(|ui| {
        if ui.button("‹").on_hover_text("Previous month").clicked()
            && let Ok(previous) = month.checked_sub(Span::new().months(1))
        {
            month = previous;
        }
        theme::text(
            ui,
            month.strftime("%B %Y").to_string(),
            theme::semibold(13.0),
            app.palette.text,
        );
        if ui.button("›").on_hover_text("Next month").clicked()
            && let Ok(next) = month.checked_add(Span::new().months(1))
        {
            month = next;
        }
    });
    ui.data_mut(|data| data.insert_temp(id, month));
    egui::Grid::new("search-calendar")
        .spacing(vec2(2.0, 3.0))
        .show(ui, |ui| {
            for name in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"] {
                ui.add_sized([30.0, 24.0], egui::Label::new(name));
            }
            ui.end_row();
            let offset = i16::from(month.weekday().to_monday_zero_offset());
            let cells = ((offset + i16::from(month.days_in_month()) + 6) / 7) * 7;
            for cell in 0..cells {
                let day = cell - offset + 1;
                if day > 0 && day <= i16::from(month.days_in_month()) {
                    let date = Date::new(month.year(), month.month(), day as i8)
                        .expect("valid calendar day");
                    if ui
                        .add_sized(
                            [30.0, 26.0],
                            egui::Button::new(day.to_string())
                                .selected(app.chat_search.day == Some(date)),
                        )
                        .on_hover_text(date.to_string())
                        .clicked()
                    {
                        app.actions.push(Action::SearchChatDay(Some(date)));
                        ui.close();
                    }
                } else {
                    ui.allocate_space(vec2(30.0, 26.0));
                }
                if cell % 7 == 6 {
                    ui.end_row();
                }
            }
        });
}

/// Preview the field and line actually matched by the archive index.
pub fn matching_line(content: &Content, query: &str) -> String {
    let fields: Vec<&str> = match content {
        Content::Text { text, .. } => vec![text],
        Content::Image { caption, .. } | Content::Video { caption, .. } => {
            caption.iter().map(String::as_str).collect()
        }
        Content::Document {
            file_name, caption, ..
        } => std::iter::once(file_name.as_str())
            .chain(caption.as_deref())
            .collect(),
        Content::Poll { question, .. } => vec![question],
        Content::Contact { display_name, .. } => vec![display_name],
        Content::Location { name, .. } => name.iter().map(String::as_str).collect(),
        _ => Vec::new(),
    };
    let query = query.trim().to_lowercase();
    fields
        .into_iter()
        .flat_map(str::lines)
        .find(|line| !query.is_empty() && line.to_lowercase().contains(&query))
        .map_or_else(|| content.summary(), str::to_owned)
}

#[cfg(test)]
mod tests {
    #[test]
    fn previews_the_matching_line_instead_of_the_first_line() {
        let content =
            crate::model::Content::text("Unrelated heading\nMeet at the library tomorrow");
        assert_eq!(
            super::matching_line(&content, "LIBRARY"),
            "Meet at the library tomorrow"
        );
        assert_eq!(super::matching_line(&content, ""), "Unrelated heading");
    }
}
