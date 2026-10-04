//! State and routing for the in-app image preview.

use std::path::{Path, PathBuf};

use egui::{Event, Key, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenTarget {
    Preview,
    External,
}

/// Raster formats supported by the image loader used in chat bubbles.
pub fn can_preview_image(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "gif" | "webp"
            )
        })
}

/// Chooses the native preview for an image rendered successfully by the conversation view.
pub fn open_target(path: &Path, rendered: bool) -> OpenTarget {
    if rendered && can_preview_image(path) {
        OpenTarget::Preview
    } else {
        OpenTarget::External
    }
}

/// Keyboard command the preview handles while it owns the window.
pub fn preview_action(key: Key, modifiers: Modifiers) -> Option<crate::model::Action> {
    let command = modifiers.command || modifiers.ctrl;
    match (command, key) {
        (true, Key::Plus) | (true, Key::Equals) => Some(crate::model::Action::ZoomImageIn),
        (true, Key::Minus) => Some(crate::model::Action::ZoomImageOut),
        (true, Key::Num0) => Some(crate::model::Action::FitImage),
        (false, Key::Plus) | (false, Key::Equals) if !modifiers.any() => {
            Some(crate::model::Action::ZoomImageIn)
        }
        (false, Key::Minus) if !modifiers.any() => Some(crate::model::Action::ZoomImageOut),
        (false, Key::Num0) if !modifiers.any() => Some(crate::model::Action::FitImage),
        _ => None,
    }
}

/// The preview swallows keys that would type into or edit the chat behind
/// it, while leaving navigation and activation keys (Tab, Enter, Space,
/// arrows) for the preview modal's own controls.
pub fn consumes_key(key: &Event) -> bool {
    match key {
        Event::Text(_) | Event::Paste(_) | Event::Copy | Event::Cut => true,
        Event::Key { key, .. } => !is_modal_navigation(*key),
        _ => false,
    }
}

/// Keys the preview modal needs for focus traversal, activation, and
/// scrolling its own controls.
fn is_modal_navigation(key: Key) -> bool {
    matches!(
        key,
        Key::Tab
            | Key::Enter
            | Key::Space
            | Key::ArrowUp
            | Key::ArrowDown
            | Key::ArrowLeft
            | Key::ArrowRight
            | Key::Home
            | Key::End
            | Key::PageUp
            | Key::PageDown
    )
}

/// Fitted image size for a canvas, keeping the aspect ratio and never
/// enlarging past the original pixels. `(0, 0)` original sizes get `(0, 0)`.
pub fn fit_size(width: f32, height: f32, canvas_width: f32, canvas_height: f32) -> (f32, f32) {
    if width <= 0.0 || height <= 0.0 {
        return (0.0, 0.0);
    }
    let scale = (canvas_width / width).min(canvas_height / height).min(1.0);
    (width * scale, height * scale)
}

/// Image size for the zoom level, relative to the original pixels.
pub fn zoomed_size(width: f32, height: f32, zoom: f32) -> (f32, f32) {
    (width * zoom, height * zoom)
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreviewState {
    path: PathBuf,
    zoom: f32,
    fit: bool,
    /// Scale the fitted image is drawn at, so zooming starts from what is
    /// on screen rather than from the original pixels.
    fit_scale: f32,
}

impl PreviewState {
    const MIN_ZOOM: f32 = 0.25;
    const MAX_ZOOM: f32 = 4.0;
    pub const ZOOM_STEP: f32 = 1.25;

    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            zoom: 1.0,
            fit: true,
            fit_scale: 1.0,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    pub fn is_fit(&self) -> bool {
        self.fit
    }

    /// The scale on screen: the fitted scale while fitting, else the zoom.
    pub fn scale(&self) -> f32 {
        if self.fit { self.fit_scale } else { self.zoom }
    }

    /// Records the scale the view fitted the image at.
    pub fn set_fit_scale(&mut self, scale: f32) {
        if scale.is_finite() && scale > 0.0 {
            self.fit_scale = scale;
        }
    }

    pub fn zoom_in(&mut self) {
        self.zoom_by(Self::ZOOM_STEP);
    }

    pub fn zoom_out(&mut self) {
        self.zoom_by(1.0 / Self::ZOOM_STEP);
    }

    pub fn zoom_by(&mut self, factor: f32) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        self.zoom = (self.scale() * factor)
            .clamp(Self::MIN_ZOOM.min(self.fit_scale * 0.25), Self::MAX_ZOOM);
        self.fit = false;
    }

    pub fn fit(&mut self) {
        self.fit = true;
        self.zoom = 1.0;
    }

    /// Shows the original pixels at 100%.
    pub fn actual_size(&mut self) {
        self.fit = false;
        self.zoom = 1.0;
    }
}

/// Keeps an image point anchored as its size changes, centered in a viewport
/// that may change size too: `from` is the point in the old viewport, `to`
/// where it should land in the new one.
pub fn anchored_offset(
    viewport: egui::Vec2,
    new_viewport: egui::Vec2,
    old: egui::Vec2,
    new: egui::Vec2,
    offset: egui::Vec2,
    from: egui::Vec2,
    to: egui::Vec2,
) -> egui::Vec2 {
    let axis = |d: usize| {
        let origin = |viewport: f32, size: f32| (viewport.max(size) - size) / 2.0;
        let fraction = if old[d] > 0.0 {
            (offset[d] + from[d] - origin(viewport[d], old[d])) / old[d]
        } else {
            0.5
        };
        let limit = new_viewport[d].max(new[d]) - new_viewport[d];
        (origin(new_viewport[d], new[d]) + fraction * new[d] - to[d]).clamp(0.0, limit)
    };
    egui::vec2(axis(0), axis(1))
}

/// Decodes one clipboard image on a worker with a bounded pixel allocation.
pub fn clipboard_image(path: &Path) -> Result<egui::ColorImage, String> {
    let decode = || -> Result<egui::ColorImage, image::ImageError> {
        let mut reader = image::ImageReader::open(path)?.with_guessed_format()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(8192);
        limits.max_image_height = Some(8192);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        use image::ImageDecoder;
        let decoder = reader.into_decoder()?;
        let (width, height) = decoder.dimensions();
        // Bound conversion and the clipboard's second RGBA buffer too.
        if u64::from(width) * u64::from(height) > 16 * 1024 * 1024 {
            return Err(image::ImageError::Limits(
                image::error::LimitError::from_kind(
                    image::error::LimitErrorKind::InsufficientMemory,
                ),
            ));
        }
        let image = image::DynamicImage::from_decoder(decoder)?.to_rgba8();
        Ok(egui::ColorImage::from_rgba_unmultiplied(
            [image.width() as usize, image.height() as usize],
            &image,
        ))
    };
    decode().map_err(|_| "Could not copy this image".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn clipboard_copy_decodes_pixels_and_rejects_invalid_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("copy.png");
        image::RgbaImage::from_pixel(3, 2, image::Rgba([30, 80, 120, 255]))
            .save(&path)
            .unwrap();
        let copied = clipboard_image(&path).unwrap();
        assert_eq!(copied.size, [3, 2]);
        assert!(
            copied
                .pixels
                .iter()
                .all(|p| p.to_array() == [30, 80, 120, 255])
        );
        image::RgbaImage::new(8193, 1).save(&path).unwrap();
        assert!(
            clipboard_image(&path).is_err(),
            "reject oversized dimensions before decoding"
        );
        std::fs::write(&path, b"invalid image").unwrap();
        assert!(clipboard_image(&path).is_err());
    }

    #[test]
    fn zoom_anchor_preserves_the_point_and_clamps_to_the_canvas() {
        use egui::vec2;
        assert_eq!(
            anchored_offset(
                vec2(400., 400.),
                vec2(400., 400.),
                vec2(400., 400.),
                vec2(800., 800.),
                vec2(0., 0.),
                vec2(100., 200.),
                vec2(100., 200.)
            ),
            vec2(100., 200.)
        );
        assert_eq!(
            anchored_offset(
                vec2(400., 400.),
                vec2(400., 400.),
                vec2(800., 800.),
                vec2(200., 200.),
                vec2(400., 400.),
                vec2(100., 200.),
                vec2(100., 200.)
            ),
            vec2(0., 0.)
        );
        // A 300x400 picture area that wraps the image grows to a 600x500 one,
        // its corner moving by half the growth (150, 50): the pointed-at pixel
        // (100, 200) of the old image is (200, 400) of the doubled one, under
        // the pointer at (100, 200) + (150, 50) in the new area.
        assert_eq!(
            anchored_offset(
                vec2(300., 400.),
                vec2(600., 500.),
                vec2(300., 400.),
                vec2(600., 800.),
                vec2(0., 0.),
                vec2(100., 200.),
                vec2(250., 250.)
            ),
            vec2(0., 150.)
        );
        let mut preview = PreviewState::new("fixture.png".into());
        preview.zoom_by(f32::NAN);
        preview.zoom_by(-1.);
        assert!(preview.is_fit());
    }

    #[test]
    fn zooming_out_of_a_large_fitted_photo_never_enlarges_it() {
        let mut preview = PreviewState::new(PathBuf::from("large.png"));
        preview.set_fit_scale(0.1);
        preview.zoom_out();
        assert!((preview.zoom() - 0.08).abs() < 1e-6);
    }

    #[test]
    fn zooming_from_fit_starts_at_the_fitted_scale() {
        let mut preview = PreviewState::new(PathBuf::from("photo.png"));
        preview.set_fit_scale(0.4);
        preview.zoom_in();
        assert!(!preview.is_fit());
        assert!((preview.zoom() - 0.5).abs() < 1e-6);
        preview.actual_size();
        assert_eq!(preview.zoom(), 1.0);
        preview.fit();
        preview.zoom_out();
        assert!((preview.zoom() - 0.32).abs() < 1e-6);
    }

    #[test]
    fn rendered_supported_images_route_to_the_preview() {
        assert_eq!(
            open_target(Path::new("photo.PNG"), true),
            OpenTarget::Preview
        );
        assert_eq!(
            open_target(Path::new("photo.heic"), true),
            OpenTarget::External
        );
        assert_eq!(
            open_target(Path::new("photo.png"), false),
            OpenTarget::External
        );
    }

    #[test]
    fn preview_keys_map_to_zoom_commands_including_shifted_equals() {
        use egui::{Key, Modifiers};

        assert_eq!(
            preview_action(Key::Equals, Modifiers::COMMAND),
            Some(crate::model::Action::ZoomImageIn)
        );
        assert_eq!(
            preview_action(
                Key::Equals,
                Modifiers {
                    ctrl: true,
                    shift: true,
                    ..Default::default()
                },
            ),
            Some(crate::model::Action::ZoomImageIn)
        );
        assert_eq!(
            preview_action(Key::Minus, Modifiers::NONE),
            Some(crate::model::Action::ZoomImageOut)
        );
        assert_eq!(
            preview_action(Key::Num0, Modifiers::COMMAND),
            Some(crate::model::Action::FitImage)
        );
        assert_eq!(preview_action(Key::Escape, Modifiers::NONE), None);
    }

    #[test]
    fn the_preview_swallows_chat_input_keys() {
        use egui::{Event, Key, Modifiers};

        assert!(consumes_key(&Event::Text("a".into())));
        assert!(consumes_key(&Event::Copy));
        assert!(consumes_key(&Event::Cut));
        assert!(consumes_key(&Event::Paste("a".into())));
        assert!(consumes_key(&Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }));
        assert!(!consumes_key(&Event::PointerMoved(egui::pos2(1.0, 2.0))));
        for key in [
            Key::Tab,
            Key::Enter,
            Key::Space,
            Key::ArrowDown,
            Key::ArrowUp,
        ] {
            assert!(
                !consumes_key(&Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }),
                "modal navigation key {key:?} must reach the preview"
            );
        }
    }

    #[test]
    fn fit_and_zoom_sizes_keep_the_aspect_ratio() {
        assert_eq!(fit_size(1600.0, 1200.0, 800.0, 700.0), (800.0, 600.0));
        assert_eq!(fit_size(320.0, 240.0, 800.0, 700.0), (320.0, 240.0));
        assert_eq!(fit_size(600.0, 1200.0, 800.0, 700.0), (350.0, 700.0));
        assert_eq!(fit_size(0.0, 0.0, 800.0, 700.0), (0.0, 0.0));
        assert_eq!(zoomed_size(320.0, 240.0, 2.0), (640.0, 480.0));
        assert_eq!(zoomed_size(320.0, 240.0, 0.25), (80.0, 60.0));
    }

    #[test]
    fn preview_starts_fitted_and_zoom_has_sensible_limits() {
        let mut preview = PreviewState::new(PathBuf::from("photo.png"));
        assert_eq!(preview.path(), Path::new("photo.png"));
        assert!(preview.is_fit());

        preview.zoom_in();
        assert!(!preview.is_fit());
        assert_eq!(preview.zoom(), 1.25);
        for _ in 0..20 {
            preview.zoom_in();
        }
        assert_eq!(preview.zoom(), 4.0);
        for _ in 0..40 {
            preview.zoom_out();
        }
        assert_eq!(preview.zoom(), 0.25);

        preview.fit();
        assert!(preview.is_fit());
        assert_eq!(preview.zoom(), 1.0);
    }
}
