//! Exact measured heights for offscreen rows. No estimated heights: a new or
//! invalidated row is measured until its height settles. Scrollbar and jump
//! targets use the same measurements.

use std::collections::HashMap;
use std::mem::size_of;
use std::sync::Arc;

use egui::{Galley, Id, Pos2, Rect, Sense};

/// Selection geometry is optional. Beyond this per-chat budget, use full layout.
const SELECTION_BYTES: usize = 16 * 1024 * 1024;

#[derive(Default)]
pub struct Heights {
    rows: HashMap<String, Row>,
    context: Option<(u32, u32, bool, bool)>,
    selection_bytes: usize,
    pub changed: bool,
    /// First visible row and its screen position, for preserving the reader's
    /// place when history is prepended or an earlier message changes height.
    pub anchor: Option<(String, f32)>,
}

struct Row {
    previous: Option<String>,
    height: f32,
    drawn: bool,
    settled: bool,
    selection: Option<Box<Selection>>,
    selection_rejected: bool,
}

/// Everything egui needs from an offscreen message during a selection, without
/// rebuilding the bubble. Coordinates are relative to the measured row origin.
pub struct Selection {
    pub transcript: Arc<crate::transcript::Row>,
    body: Option<Body>,
    bytes: usize,
}

struct Body {
    id: Id,
    rect: Rect,
    galley: Arc<Galley>,
}

impl Selection {
    pub fn new(transcript: Arc<crate::transcript::Row>) -> Self {
        let bytes = size_of::<Self>() + transcript.estimated_bytes();
        Self {
            transcript,
            body: None,
            bytes,
        }
    }

    pub fn with_body(mut self, id: Id, rect: Rect, mut galley: Galley) -> Self {
        // Keep glyph metrics for hit testing and copy, but discard all paint
        // meshes. These galleys are ONLY replayed outside the viewport, so font
        // atlas resets cannot leave visible text using stale texture coordinates.
        self.bytes += size_of::<Galley>()
            + size_of::<egui::text::LayoutJob>()
            + galley.job.text.capacity()
            + galley.job.sections.capacity() * size_of::<egui::text::LayoutSection>()
            + galley.rows.capacity() * size_of::<egui::epaint::text::PlacedRow>();
        for placed in &mut galley.rows {
            let mut row = (*placed.row).clone();
            row.visuals = Default::default();
            self.bytes += size_of::<egui::epaint::text::Row>()
                + row.glyphs.capacity() * size_of::<egui::epaint::text::Glyph>();
            placed.row = Arc::new(row);
        }
        galley.mesh_bounds = Rect::NOTHING;
        galley.num_vertices = 0;
        galley.num_indices = 0;
        self.body = Some(Body {
            id,
            rect,
            galley: Arc::new(galley),
        });
        self
    }

    pub fn relative_to(mut self, origin: Pos2) -> Self {
        if let Some(body) = &mut self.body {
            body.rect = body.rect.translate(-origin.to_vec2());
        }
        self
    }

    /// Register every text body in order, including both selection endpoints.
    pub fn register(&self, ui: &egui::Ui, origin: Pos2) {
        if let Some(body) = &self.body {
            let rect = body.rect.translate(origin.to_vec2());
            debug_assert!(!ui.is_rect_visible(rect));
            let response = ui.interact(rect, body.id, Sense::click_and_drag());
            egui::text_selection::LabelSelectionState::label_text_selection(
                ui,
                &response,
                Pos2::new(ui.clip_rect().left(), rect.top()),
                Arc::clone(&body.galley),
                ui.visuals().text_color(),
                egui::Stroke::NONE,
            );
        }
    }
}

impl Heights {
    /// Owned allocations only; hash-table control/allocator overhead is approximate.
    pub fn estimated_bytes(&self) -> usize {
        self.rows.capacity() * (std::mem::size_of::<(String, Row)>() + 1)
            + self
                .rows
                .iter()
                .map(|(id, row)| id.capacity() + row.previous.as_ref().map_or(0, String::capacity))
                .sum::<usize>()
            + self.anchor.as_ref().map_or(0, |(id, _)| id.capacity())
            + self.selection_bytes
    }

    pub fn clear(&mut self) {
        self.rows.clear();
        self.selection_bytes = 0;
        self.changed = true;
    }

    pub fn invalidate(&mut self, id: &str) {
        if let Some(row) = self.rows.remove(id)
            && let Some(selection) = row.selection
        {
            self.selection_bytes -= selection.bytes;
        }
        self.changed = true;
    }

    pub fn prepare(&mut self, width: f32, pixels: f32, pictures: bool, contact_names: bool) {
        let context = (width.to_bits(), pixels.to_bits(), pictures, contact_names);
        if self.context != Some(context) {
            self.clear();
            self.context = Some(context);
        }
    }

    pub fn get(&self, id: &str, previous: Option<&str>) -> Option<f32> {
        self.rows
            .get(id)
            .filter(|row| row.settled && row.previous.as_deref() == previous)
            .map(|row| row.height)
    }

    pub fn mark_skipped(&mut self, id: &str) -> bool {
        self.rows
            .get_mut(id)
            .is_some_and(|row| std::mem::take(&mut row.drawn))
    }

    pub fn selection(&self, id: &str) -> Option<&Selection> {
        self.rows.get(id)?.selection.as_deref()
    }

    pub fn needs_selection(&self, id: &str) -> bool {
        self.selection_bytes < SELECTION_BYTES
            && self
                .rows
                .get(id)
                .is_none_or(|row| row.selection.is_none() && !row.selection_rejected)
    }

    pub fn set_selection(&mut self, id: &str, selection: Selection) {
        let Some(row) = self.rows.get_mut(id) else {
            return;
        };
        if let Some(previous) = row.selection.take() {
            self.selection_bytes -= previous.bytes;
        }
        row.selection_rejected = self.selection_bytes + selection.bytes > SELECTION_BYTES;
        if !row.selection_rejected {
            self.selection_bytes += selection.bytes;
            row.selection = Some(Box::new(selection));
        }
    }

    pub fn set(&mut self, id: &str, previous: Option<&str>, height: f32) {
        if let Some(row) = self.rows.get_mut(id) {
            row.settled = row.previous.as_deref() == previous && (row.height - height).abs() < 0.5;
            if !row.settled {
                self.changed = true;
                if let Some(selection) = row.selection.take() {
                    self.selection_bytes -= selection.bytes;
                }
                row.selection_rejected = false;
            }
            if row.previous.as_deref() != previous {
                row.previous = previous.map(str::to_owned);
            }
            row.height = height;
            row.drawn = true;
            return;
        }
        self.changed = true;
        self.rows.insert(
            id.to_owned(),
            Row {
                previous: previous.map(str::to_owned),
                height,
                drawn: true,
                settled: false,
                selection: None,
                selection_rejected: false,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(body: String) -> Selection {
        Selection::new(Arc::new(crate::transcript::Row {
            body,
            ..Default::default()
        }))
    }

    #[test]
    fn selection_cache_is_bounded_and_invalidations_release_it() {
        let mut heights = Heights::default();
        heights.prepare(600.0, 2.0, false, true);
        for id in ["cached", "too-large"] {
            heights.set(id, None, 30.0);
            heights.set(id, None, 30.0);
        }
        heights.set_selection("cached", snapshot("original".into()));
        let bytes = heights.selection_bytes;
        assert!(bytes > 0);
        assert!(heights.estimated_bytes() > bytes);
        heights.set_selection("too-large", snapshot("x".repeat(SELECTION_BYTES)));
        assert!(heights.selection("too-large").is_none());
        assert!(
            !heights.needs_selection("too-large"),
            "do not retry every frame"
        );
        assert_eq!(heights.get("too-large", None), Some(30.0));
        assert_eq!(heights.selection_bytes, bytes);

        heights.set("cached", None, 30.0);
        assert!(
            heights.selection("cached").is_some(),
            "unchanged layout survives"
        );
        heights.set("cached", Some("prepended"), 30.0);
        assert!(
            heights.selection("cached").is_none(),
            "grouping can move the body"
        );
        assert_eq!(heights.selection_bytes, 0);
        heights.set_selection("cached", snapshot("new grouping".into()));
        heights.set("cached", Some("prepended"), 60.0);
        assert!(
            heights.selection("cached").is_none(),
            "height changes remeasure"
        );
        heights.set_selection("cached", snapshot("edited".into()));
        heights.invalidate("cached");
        assert_eq!(heights.selection_bytes, 0);
        heights.set_selection("too-large", snapshot("smaller".into()));
        heights.prepare(500.0, 2.0, false, true);
        assert_eq!(heights.selection_bytes, 0, "resizing clears geometry");
        assert!(heights.selection("too-large").is_none());
    }
}
