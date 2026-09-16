//! Exact measured heights for offscreen rows. No estimated heights: a new or
//! invalidated row is measured until its height settles. Scrollbar and jump
//! targets use the same measurements.

use std::collections::HashMap;

#[derive(Default)]
pub struct Heights {
    rows: HashMap<String, Row>,
    context: Option<(u32, u32, bool, bool)>,
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
    }

    pub fn clear(&mut self) {
        self.rows.clear();
        self.changed = true;
    }

    pub fn invalidate(&mut self, id: &str) {
        self.rows.remove(id);
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

    pub fn set(&mut self, id: &str, previous: Option<&str>, height: f32) {
        let settled = self.rows.get(id).is_some_and(|row| {
            row.previous.as_deref() == previous && (row.height - height).abs() < 0.5
        });
        if !settled {
            self.changed = true;
        }
        self.rows.insert(
            id.to_owned(),
            Row {
                previous: previous.map(str::to_owned),
                height,
                drawn: true,
                settled,
            },
        );
    }
}
