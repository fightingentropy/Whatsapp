//! Expanded presentation and shared controls for the single native video player.

use egui::{
    Align, Align2, Color32, CornerRadius, Frame, Layout, Margin, Rect, Sense, Stroke, Vec2, vec2,
};

use super::widgets;
use crate::app::App;
use crate::model::{Action, Content, MediaState, Message};
use crate::theme::{self, Icon, Palette};
use crate::video::{Playback, State};

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some((chat, message)) = &app.video_preview else {
        return;
    };
    let Some(row) = app
        .conversations
        .get(chat)
        .and_then(|conversation| conversation.message(message))
    else {
        return;
    };
    let Content::Video {
        media, gif: false, ..
    } = &row.content
    else {
        return;
    };
    let palette = &app.palette;
    let active = app.video.for_message(chat, message);
    let playing = active.is_some_and(Playback::is_playing);
    let loading = matches!(media.state, MediaState::Downloading)
        || active.is_some_and(|active| active.state == State::Loading && active.is_playing());
    let failed = matches!(media.state, MediaState::Failed(_))
        || active.is_some_and(|active| active.state == State::Failed);
    let mut actions = Vec::new();
    let size = (ctx.content_rect().size() - vec2(64.0, 80.0))
        .max(vec2(160.0, 160.0))
        .min(vec2(1440.0, 1000.0));
    let response = egui::Modal::new(egui::Id::new("video-preview"))
        .frame(
            Frame::new()
                .fill(palette.overlay)
                .stroke(Stroke::new(1.0, palette.outline))
                .corner_radius(CornerRadius::same(theme::RADIUS + 4))
                .inner_margin(Margin::same(14)),
        )
        .backdrop_color(palette.shadow)
        .show(ctx, |ui| {
            ui.set_width(size.x);
            ui.set_height(size.y);
            ui.horizontal(|ui| {
                widgets::rich_text(ui, "Video", theme::semibold(14.0), palette.text);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if theme::icon_button(
                        ui,
                        Icon::X,
                        18.0,
                        palette.secondary,
                        palette.text,
                        "Back to chat (Esc)",
                    )
                    .clicked()
                    {
                        actions.push(Action::CloseVideoPreview);
                    }
                });
            });
            ui.separator();
            let canvas = vec2(
                ui.available_width(),
                (ui.available_height() - 38.0).max(1.0),
            );
            let (rect, response) = ui.allocate_exact_size(canvas, Sense::click());
            ui.painter().rect_filled(rect, 6.0, Color32::BLACK);
            if let Some(active) = active.filter(|active| active.texture.is_some()) {
                active.paint(ui.painter(), rect);
            } else if let Some(thumbnail) = row.thumbnail.as_deref() {
                let source = vec2(
                    media.width.unwrap_or(16).max(1) as f32,
                    media.height.unwrap_or(9).max(1) as f32,
                );
                let fitted = source * (canvas.x / source.x).min(canvas.y / source.y);
                egui::Image::new(super::conversation::thumbnail_uri(
                    ctx, chat, message, thumbnail,
                ))
                .fit_to_exact_size(fitted)
                .paint_at(ui, Rect::from_center_size(rect.center(), fitted));
            }
            if !playing || loading {
                let disc = Rect::from_center_size(rect.center(), Vec2::splat(64.0));
                ui.painter()
                    .circle_filled(disc.center(), 32.0, Color32::from_black_alpha(150));
                if loading {
                    theme::paint_spinner(ui, disc, 30.0, Color32::WHITE);
                } else {
                    theme::paint_icon(
                        ui,
                        if failed { Icon::Refresh } else { Icon::Play },
                        disc,
                        28.0,
                        Color32::WHITE,
                    );
                }
            }
            if failed {
                ui.painter().text(
                    rect.center_bottom() - vec2(0.0, 16.0),
                    Align2::CENTER_BOTTOM,
                    "Cannot play · click to retry",
                    theme::regular(13.0),
                    Color32::WHITE,
                );
            }
            if response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                actions.push(Action::PlayVideo {
                    chat: chat.clone(),
                    message: message.clone(),
                });
            }
            controls(ui, palette, active, row, canvas.x, &mut actions);
            #[cfg(any(test, feature = "demo"))]
            ctx.data_mut(|data| data.insert_temp(egui::Id::new("video-preview-canvas"), rect));
        });
    if response.should_close() {
        actions.push(Action::CloseVideoPreview);
    }
    app.actions.extend(actions);
}

pub(super) fn controls(
    ui: &mut egui::Ui,
    palette: &Palette,
    active: Option<&Playback>,
    message: &Message,
    width: f32,
    actions: &mut Vec<Action>,
) {
    let Content::Video { seconds, .. } = &message.content else {
        return;
    };
    let playing = active.is_some_and(Playback::is_playing);
    let failed = active.is_some_and(|active| active.state == State::Failed);
    let total = active.map_or_else(|| f64::from(seconds.unwrap_or(0)), |active| active.duration);
    let position = active.map_or(0.0, |active| active.position);
    let mut fraction = if total > 0.0 {
        (position / total).clamp(0.0, 1.0)
    } else {
        0.0
    };
    ui.allocate_ui_with_layout(
        vec2(width, 30.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if theme::icon_button(
                ui,
                if playing { Icon::Pause } else { Icon::Play },
                14.0,
                palette.secondary,
                palette.text,
                if playing { "Pause video" } else { "Play video" },
            )
            .clicked()
            {
                actions.push(Action::PlayVideo {
                    chat: message.chat.clone(),
                    message: message.id.clone(),
                });
            }
            ui.spacing_mut().slider_width = (width - 102.0).max(16.0);
            let slider = ui
                .add_enabled(
                    active.is_some_and(|active| active.duration > 0.0 && !failed),
                    egui::Slider::new(&mut fraction, 0.0..=1.0).show_value(false),
                )
                .on_hover_text(format!(
                    "{} / {}",
                    crate::util::duration(position as u32),
                    crate::util::duration(total as u32)
                ));
            if slider.changed() {
                actions.push(Action::SeekVideo {
                    chat: message.chat.clone(),
                    message: message.id.clone(),
                    fraction,
                });
            }
            let label = widgets::line(
                ui,
                &crate::util::duration(if active.is_some() { position } else { total } as u32),
                theme::regular(10.5),
                palette.secondary,
                36.0,
                1,
            );
            let (time_rect, _) = ui.allocate_exact_size(vec2(36.0, label.size().y), Sense::hover());
            if ui.is_rect_visible(time_rect) {
                label.paint(ui, time_rect.min, palette.secondary);
            }
            let muted = active.is_some_and(|active| active.muted);
            ui.add_enabled_ui(
                active.is_some_and(|active| active.has_audio && !failed),
                |ui| {
                    if theme::icon_button(
                        ui,
                        if muted { Icon::VolumeX } else { Icon::Volume2 },
                        14.0,
                        palette.secondary,
                        palette.text,
                        if muted { "Unmute video" } else { "Mute video" },
                    )
                    .clicked()
                    {
                        actions.push(Action::MuteVideo {
                            chat: message.chat.clone(),
                            message: message.id.clone(),
                        });
                    }
                },
            );
        },
    );
}
