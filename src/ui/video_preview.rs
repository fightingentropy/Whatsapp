//! Expanded presentation and shared controls for the single native video player.

use egui::{Align, Align2, Color32, Frame, Layout, Rect, Sense, UiBuilder, Vec2, vec2};

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
    let source = active.and_then(Playback::display_size).unwrap_or_else(|| {
        vec2(
            media.width.filter(|width| *width > 0).unwrap_or(16) as f32,
            media.height.filter(|height| *height > 0).unwrap_or(9) as f32,
        )
    });
    let available = (ctx.content_rect().size() - Vec2::splat(48.0)).max(Vec2::splat(1.0));
    let size = source * (available.x / source.x).min(available.y / source.y);
    let id = egui::Id::new("video-preview");
    // Position from this frame's dimensions rather than the area's last size,
    // which may belong to a different rotation, window size or UI zoom.
    let area = egui::Area::new(id)
        .kind(egui::UiKind::Modal)
        .sense(Sense::hover())
        .order(egui::Order::Foreground)
        .fixed_pos(ctx.content_rect().center() - size * 0.5)
        .default_size(size)
        .constrain(false);
    let response = egui::Modal::new(id)
        .area(area)
        .frame(Frame::NONE)
        .backdrop_color(palette.shadow)
        .show(ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            ui.set_clip_rect(rect);
            ui.painter().rect_filled(rect, 0.0, Color32::BLACK);
            if let Some(active) = active.filter(|active| active.texture.is_some()) {
                active.paint(ui.painter(), rect);
            } else {
                super::conversation::paint_video_poster(ui, row, rect, 0.0);
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
                    rect.center_bottom() - vec2(0.0, 64.0),
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
            let bar = Rect::from_min_max(rect.left_bottom() - vec2(0.0, 46.0), rect.right_bottom());
            let close = Rect::from_min_size(rect.right_top() + vec2(-40.0, 8.0), Vec2::splat(32.0));
            let show_controls = !playing
                || ctx.memory(|memory| memory.focused().is_some())
                || ctx.input(|input| {
                    input.key_pressed(egui::Key::Tab)
                        || input.pointer.hover_pos().is_some_and(|pos| {
                            rect.contains(pos)
                                && (input
                                    .pointer
                                    .time_since_last_movement()
                                    .min(input.pointer.time_since_last_click())
                                    < 2.0
                                    || bar.contains(pos)
                                    || close.contains(pos)
                                    || input.pointer.any_down())
                        })
                });
            if show_controls {
                // Child UIs overlay the picture without adding to the modal's
                // measured size. No title bar, padding or surrounding panel.
                if rect.width() >= 160.0 {
                    ui.painter()
                        .rect_filled(bar, 0.0, Color32::from_black_alpha(170));
                    let mut overlay = ui.new_child(UiBuilder::new().max_rect(bar.shrink(8.0)));
                    let overlay_palette = Palette {
                        text: Color32::WHITE,
                        secondary: Color32::WHITE,
                        ..*palette
                    };
                    controls(
                        &mut overlay,
                        &overlay_palette,
                        active,
                        row,
                        bar.width() - 16.0,
                        &mut actions,
                    );
                }
                ui.painter()
                    .circle_filled(close.center(), 16.0, Color32::from_black_alpha(150));
                let mut overlay = ui.new_child(
                    UiBuilder::new()
                        .max_rect(close)
                        .layout(Layout::centered_and_justified(egui::Direction::LeftToRight)),
                );
                if theme::icon_button(
                    &mut overlay,
                    Icon::X,
                    18.0,
                    Color32::WHITE,
                    Color32::WHITE,
                    "Back to chat (Esc)",
                )
                .clicked()
                {
                    actions.push(Action::CloseVideoPreview);
                }
            }
            #[cfg(any(test, feature = "demo"))]
            ctx.data_mut(|data| {
                data.insert_temp(egui::Id::new("video-preview-canvas"), rect);
                data.insert_temp(
                    egui::Id::new("video-preview-controls-visible"),
                    show_controls,
                );
            });
        });
    #[cfg(any(test, feature = "demo"))]
    ctx.data_mut(|data| {
        data.insert_temp(
            egui::Id::new("video-preview-bounds"),
            response.response.rect,
        )
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
