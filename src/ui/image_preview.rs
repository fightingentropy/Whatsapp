//! Native preview for downloaded image attachments.

use egui::{Align, CornerRadius, Frame, Layout, Margin, Rect, Stroke, UiBuilder, Vec2, vec2};

use crate::app::App;
use crate::image_preview::PreviewState;
use crate::model::Action;
use crate::theme::{self, Icon, Palette};

/// Narrowest picture area: room for the title row's controls and part of the
/// file name. Narrower pictures are centered inside it.
const MIN_WIDTH: f32 = 360.0;
/// Picture area while the image loads or when it cannot be shown.
const PLACEHOLDER: Vec2 = vec2(MIN_WIDTH, 240.0);
/// Panel space around the picture until it has been measured: the frame's
/// margin and stroke on every side, plus the title row and its separator.
const CHROME: Vec2 = vec2(30.0, 78.0);

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(preview) = app.image_preview.clone() else {
        return;
    };
    let palette = app.palette;
    let frame = Frame::new()
        .fill(palette.overlay)
        .stroke(Stroke::new(1.0, palette.outline))
        .corner_radius(CornerRadius::same(theme::RADIUS + 4))
        .inner_margin(Margin::same(14));
    let id = egui::Id::new("image-preview");
    let chrome_id = id.with("chrome");
    let chrome = ctx
        .data(|data| data.get_temp::<Vec2>(chrome_id))
        .unwrap_or(CHROME);
    let window = ctx.content_rect();
    let viewport = window.size();
    // The largest panel content, title row included; the picture gets what
    // is left under the title row.
    let bounds = vec2(
        (viewport.x * 0.9).clamp(viewport.x.min(320.0), 1200.0),
        (viewport.y * 0.88).clamp(viewport.y.min(260.0), 900.0),
    );
    let limit = (bounds - vec2(0.0, chrome.y - chrome.x))
        .floor()
        .max(Vec2::splat(1.0));
    // The conversation cache retains the open preview path.
    let image = egui::Image::new(super::conversation::file_uri(preview.path()));
    let poll = image.load_for_size(ctx, limit);
    // The panel wraps the picture, so a fitted image has no empty bands
    // beside or below it; zooming grows the panel up to the limit.
    let canvas = match &poll {
        Ok(egui::load::TexturePoll::Ready { texture }) => canvas_size(
            display_size(texture.size, limit, preview.is_fit(), preview.zoom()),
            limit,
        ),
        _ => PLACEHOLDER.min(limit),
    };
    // Position from this frame's size rather than the area's last size,
    // which belongs to the previous zoom level, image or window size.
    let area = egui::Area::new(id)
        .kind(egui::UiKind::Modal)
        .sense(egui::Sense::hover())
        .order(egui::Order::Foreground)
        .interactable(true)
        .fixed_pos(window.center() - (canvas + chrome) * 0.5)
        .default_size(canvas + chrome)
        .constrain(false);
    let response = egui::Modal::new(id)
        .area(area)
        .frame(frame)
        .backdrop_color(palette.shadow)
        .show(ctx, |ui| {
            ui.set_width(canvas.x);
            header(app, ui, &preview, &palette);
            ui.separator();
            let area = Rect::from_min_size(ui.cursor().min, canvas);
            ui.scope_builder(UiBuilder::new().max_rect(area), |ui| match poll {
                Ok(egui::load::TexturePoll::Ready { texture }) => {
                    picture(app, ui, image, texture, limit);
                }
                Ok(egui::load::TexturePoll::Pending { .. }) => {
                    let (rect, _) = ui.allocate_exact_size(canvas, egui::Sense::hover());
                    theme::paint_spinner(ui, rect, 28.0, palette.accent);
                }
                Err(_) => {
                    ui.allocate_ui_with_layout(
                        canvas,
                        Layout::centered_and_justified(egui::Direction::TopDown),
                        |ui| {
                            ui.label("This image could not be displayed in Whatsapp.");
                            if ui.button("Open externally").clicked() {
                                app.actions
                                    .push(Action::OpenFile(preview.path().to_owned()));
                            }
                        },
                    );
                }
            });
        });
    // The title row's height is only known once laid out; a change is drawn
    // centered again on the next frame.
    let measured = response.response.rect.size() - canvas;
    if (measured - chrome).length() > 0.5 {
        ctx.data_mut(|data| data.insert_temp(chrome_id, measured));
        ctx.request_repaint();
    }
    #[cfg(any(test, feature = "demo"))]
    ctx.data_mut(|data| {
        data.insert_temp(id.with("bounds"), response.response.rect);
    });
    if response.should_close() {
        app.actions.push(Action::CloseImagePreview);
    }
}

/// File name and controls. The controls are laid out first, right to left,
/// so a long name is shortened to the space they leave rather than pushing
/// them past a narrow picture.
fn header(app: &mut App, ui: &mut egui::Ui, preview: &PreviewState, palette: &Palette) {
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if theme::icon_button(
                ui,
                Icon::X,
                18.0,
                palette.secondary,
                palette.text,
                "Close preview (Esc)",
            )
            .clicked()
            {
                app.actions.push(Action::CloseImagePreview);
            }
            if theme::icon_button(
                ui,
                Icon::ExternalLink,
                18.0,
                palette.secondary,
                palette.text,
                "Open in another app",
            )
            .clicked()
            {
                app.actions
                    .push(Action::OpenFile(preview.path().to_owned()));
            }
            if theme::icon_button(
                ui,
                Icon::Copy,
                18.0,
                palette.secondary,
                palette.text,
                "Copy image (⌘C)",
            )
            .clicked()
            {
                app.actions
                    .push(Action::CopyImage(preview.path().to_owned()));
            }
            ui.add_space(8.0);
            // Right to left: zoom in, the current scale, zoom out.
            if theme::icon_button(
                ui,
                Icon::Plus,
                18.0,
                palette.secondary,
                palette.text,
                "Zoom in",
            )
            .clicked()
            {
                app.actions.push(Action::ZoomImageIn);
            }
            // One control shows the scale and switches between fitting
            // the window and the original size.
            let (label, hint, action) = if preview.is_fit() {
                (
                    "Fit".to_owned(),
                    "Show at original size",
                    Action::ImageActualSize,
                )
            } else {
                (
                    format!("{:.0}%", preview.zoom() * 100.0),
                    "Fit to the window (0)",
                    Action::FitImage,
                )
            };
            if theme::soft_button(ui, palette, None, &label, false)
                .on_hover_text(hint)
                .clicked()
            {
                app.actions.push(action);
            }
            if theme::icon_button(
                ui,
                Icon::Minus,
                18.0,
                palette.secondary,
                palette.text,
                "Zoom out",
            )
            .clicked()
            {
                app.actions.push(Action::ZoomImageOut);
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                let name = preview
                    .path()
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Image");
                crate::ui::widgets::rich_text(ui, name, theme::semibold(14.0), palette.text);
            });
        });
    });
}

/// The loaded image in the picture area, which is `ui`'s whole rect. Zooming
/// and double-clicking store the scroll offset for the next frame's area.
fn picture(
    app: &mut App,
    ui: &mut egui::Ui,
    image: egui::Image<'_>,
    texture: egui::load::SizedTexture,
    limit: Vec2,
) {
    let Some(preview) = app.image_preview.clone() else {
        return;
    };
    let ctx = ui.ctx().clone();
    let area = ui.max_rect();
    let canvas = area.size();
    let size = display_size(texture.size, limit, preview.is_fit(), preview.zoom());
    if preview.is_fit()
        && texture.size.x > 0.0
        && let Some(state) = &mut app.image_preview
    {
        state.set_fit_scale(size.x / texture.size.x);
    }
    let scroll_id = ui.make_persistent_id(egui::IdSalt::new("image-preview-scroll"));
    let trackpad = app.scroll_from_trackpad();
    // Read before the scroll area, which would otherwise take the
    // wheel. The zoom itself is applied by `App` after the frame.
    let zoom = zoom_input(ui, area, trackpad).and_then(|(factor, pointer)| {
        let mut next = app.image_preview.clone()?;
        next.zoom_by(factor);
        let zoomed = display_size(texture.size, limit, next.is_fit(), next.zoom());
        Some((factor, pointer, zoomed))
    });
    let output = egui::ScrollArea::both()
        .id_salt("image-preview-scroll")
        .auto_shrink([false, false])
        // egui drags only on touch screens by default.
        .scroll_source(egui::scroll_area::ScrollSource {
            drag: egui::scroll_area::DragScroll::Always,
            ..Default::default()
        })
        .on_hover_cursor(egui::CursorIcon::Grab)
        .on_drag_cursor(egui::CursorIcon::Grabbing)
        .show(ui, |ui| {
            ui.allocate_ui_with_layout(
                canvas.max(size),
                Layout::centered_and_justified(egui::Direction::TopDown),
                |ui| {
                    let image_response =
                        ui.add(image.fit_to_exact_size(size).sense(egui::Sense::click()));
                    // Painted centered in the justified response.
                    #[cfg(any(test, feature = "demo"))]
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(
                            egui::Id::new("image-preview").with("picture"),
                            Rect::from_center_size(image_response.rect.center(), size),
                        );
                    });
                    image_response
                        .interact_pointer_pos()
                        .filter(|_| image_response.double_clicked())
                },
            )
            .inner
        });
    // Stored after the scroll area, which clamps its offset to this frame's
    // size; the next frame lays out the zoomed size with the pointed-at pixel
    // still under the pointer. The panel stays centered while it grows or
    // shrinks with the picture, so the area's corner moves by half the change.
    if let Some((factor, pointer, zoomed)) = zoom {
        let next = canvas_size(zoomed, limit);
        let mut scroll = output.state;
        scroll.offset = crate::image_preview::anchored_offset(
            canvas,
            next,
            size,
            zoomed,
            output.state.offset,
            pointer,
            pointer + (next - canvas) / 2.0,
        );
        scroll.store(&ctx, scroll_id);
        app.actions.push(Action::ZoomImageBy(factor));
    }
    // The header's Fit/% toggle. The original size opens with the
    // double-clicked point in the middle: the offset is stored for
    // the next frame, which lays out the new size.
    if let Some(pos) = output.inner {
        if preview.is_fit() {
            let next = canvas_size(texture.size, limit);
            let mut scroll = output.state;
            scroll.offset = crate::image_preview::anchored_offset(
                canvas,
                next,
                size,
                texture.size,
                output.state.offset,
                pos - area.min,
                next / 2.0,
            );
            scroll.store(&ctx, scroll_id);
            app.actions.push(Action::ImageActualSize);
        } else {
            app.actions.push(Action::FitImage);
        }
    }
}

/// Zoom factor the wheel or a pinch asks for over the preview area, with the
/// anchor point relative to the picture area. Each wheel notch is one header
/// zoom step. A plain mouse wheel zooms instead of scrolling, so its delta is
/// taken from the scroll area. Windows and X11 report touchpads as wheel
/// lines, so two fingers zoom there; only scrolling reported in points
/// (macOS, Wayland) keeps moving the picture.
fn zoom_input(ui: &mut egui::Ui, area: Rect, trackpad: bool) -> Option<(f32, Vec2)> {
    let (touch, hover) = ui.input(|input| {
        (
            input.multi_touch().map(|touch| touch.center_pos),
            input.pointer.hover_pos(),
        )
    });
    let anchor = touch.or(hover)?;
    if !area.contains(anchor) || touch.is_none() && !ui.rect_contains_pointer(area) {
        return None;
    }
    let (zoom_speed, line_speed) = ui.ctx().options(|options| {
        let input = &options.input_options;
        (input.scroll_zoom_speed, input.line_scroll_speed)
    });
    let step = crate::image_preview::PreviewState::ZOOM_STEP;
    let factor = ui.input_mut(|input| {
        let pinch = input.multi_touch().is_some()
            || input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Zoom(_)));
        let mut factor = input.zoom_delta();
        if !pinch {
            // Without a pinch the zoom came from Ctrl/Cmd+wheel (Windows
            // touchpad pinches included), to which egui applies its own curve,
            // exp(scroll_zoom_speed * points). It keeps the modifiers from the
            // start of the gesture, so this checks the source, not the keys.
            factor = factor.powf(step.ln() / (zoom_speed * line_speed));
        }
        // Shift and Alt turn the wheel sideways; Ctrl pressed during a plain
        // notch must not hand the rest of it to the scroll area.
        if !trackpad
            && !input.modifiers.shift
            && !input.modifiers.alt
            && input.smooth_scroll_delta.y != 0.0
        {
            factor *= step.powf(input.smooth_scroll_delta.y / line_speed);
            input.smooth_scroll_delta.y = 0.0;
        }
        factor
    });
    (factor != 1.0).then_some((factor, anchor - area.min))
}

/// Size the image is drawn at from the texture's intrinsic pixel dimensions:
/// fitted into the largest picture area, or scaled by the preview's zoom
/// factor. Zoom is applied here only. The size hint passed when loading does
/// not change the texture: egui decodes raster formats (all the preview
/// accepts) once at full resolution and reports the source size, whatever
/// size is asked for.
fn display_size(original: Vec2, limit: Vec2, fit: bool, zoom: f32) -> Vec2 {
    let (width, height) = if fit {
        crate::image_preview::fit_size(original.x, original.y, limit.x, limit.y)
    } else {
        crate::image_preview::zoomed_size(original.x, original.y, zoom)
    };
    vec2(width, height)
}

/// Picture area for an image drawn at `size`: the image itself, rounded up to
/// whole points so a fitted image never scrolls, at least [`MIN_WIDTH`] wide
/// for the title row and at most `limit`, past which the image scrolls.
fn canvas_size(size: Vec2, limit: Vec2) -> Vec2 {
    size.ceil()
        .max(vec2(MIN_WIDTH.min(limit.x), 0.0))
        .min(limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitted_images_keep_aspect_ratio_inside_the_canvas() {
        assert_eq!(
            display_size(vec2(1600.0, 1200.0), vec2(800.0, 700.0), true, 1.0),
            vec2(800.0, 600.0)
        );
        assert_eq!(
            display_size(vec2(320.0, 240.0), vec2(800.0, 700.0), true, 1.0),
            vec2(320.0, 240.0)
        );
        assert_eq!(
            display_size(vec2(320.0, 240.0), vec2(800.0, 700.0), false, 2.0),
            vec2(640.0, 480.0)
        );
    }

    #[test]
    fn the_picture_area_wraps_the_image_within_its_limits() {
        let limit = vec2(1000.0, 700.0);
        assert_eq!(
            canvas_size(vec2(525.0, 700.0), limit),
            vec2(525.0, 700.0),
            "a fitted portrait leaves no bands beside it"
        );
        assert_eq!(
            canvas_size(vec2(1000.0, 250.4), limit),
            vec2(1000.0, 251.0),
            "a fitted panorama leaves none below it, rounded up so it never scrolls"
        );
        assert_eq!(
            canvas_size(vec2(64.0, 48.0), limit),
            vec2(MIN_WIDTH, 48.0),
            "a small image keeps room for the title row"
        );
        assert_eq!(
            canvas_size(vec2(2000.0, 1500.0), limit),
            limit,
            "a zoomed image scrolls inside the largest area"
        );
        assert_eq!(
            canvas_size(vec2(64.0, 48.0), vec2(300.0, 200.0)),
            vec2(300.0, 48.0),
            "a small window wins over the title row's minimum"
        );
    }
}
