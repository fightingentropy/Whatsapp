//! Repeatable native renderer measurements using offline demo data only.

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

/// Measures a fixed demo scene after allowing fonts/textures to settle.
pub struct Probe {
    path: PathBuf,
    backend: crate::renderer::Backend,
    launched: Instant,
    first_frame_ms: Option<f64>,
    previous: Option<Instant>,
    started: Option<Instant>,
    cpu_ms: Vec<f64>,
    interval_ms: Vec<f64>,
    done: bool,
    scroll: bool,
    first_offset: Option<f32>,
}

impl Probe {
    /// Creates a probe without touching the live account or saving its data.
    pub fn new(
        path: PathBuf,
        backend: crate::renderer::Backend,
        launched: Instant,
        scroll: bool,
    ) -> Self {
        Self {
            path,
            backend,
            launched,
            first_frame_ms: None,
            previous: None,
            started: None,
            cpu_ms: Vec::new(),
            interval_ms: Vec::new(),
            done: false,
            scroll,
            first_offset: None,
        }
    }

    /// Runs at the end of each demo UI frame, after two seconds of warmup.
    pub fn frame(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        if self.done {
            return;
        }
        let now = Instant::now();
        self.first_frame_ms
            .get_or_insert_with(|| now.duration_since(self.launched).as_secs_f64() * 1000.0);
        let started = *self.started.get_or_insert(now);
        if now.duration_since(started) >= Duration::from_secs(2) {
            ctx.data_mut(|data| data.insert_temp(egui::Id::new("benchmark-scroll"), self.scroll));
            if self.first_offset.is_none() {
                self.first_offset =
                    ctx.data(|data| data.get_temp::<f32>(egui::Id::new("message-scroll-offset")));
            }
            if let Some(cpu) = frame.info().cpu_usage {
                self.cpu_ms.push(f64::from(cpu) * 1000.0);
            }
            if let Some(previous) = self.previous {
                self.interval_ms
                    .push(now.duration_since(previous).as_secs_f64() * 1000.0);
            }
        }
        self.previous = Some(now);
        if self.cpu_ms.len() >= 240 {
            let report = serde_json::json!({
                "backend": format!("{:?}", self.backend),
                "full_message_layout": ctx.data(|data| data.get_temp::<bool>(egui::Id::new("full-message-layout"))).unwrap_or(false),
                "messages_laid_out": ctx.data(|data| data.get_temp::<usize>(egui::Id::new("message-layout-count"))),
                "workload": if self.scroll { "offline chat scrolling up 80 points per frame, 2 seconds warmup then 240 frames, continuous repaint stress" }
                    else { "fixed offline chat, 2 seconds warmup then 240 frames, continuous repaint stress test" },
                "first_ui_frame_ms": self.first_frame_ms,
                "first_scroll_offset": self.first_offset,
                "last_scroll_offset": ctx.data(|data| data.get_temp::<f32>(egui::Id::new("message-scroll-offset"))),
                "cpu_frame_ms": summary(&self.cpu_ms),
                "frame_interval_ms": summary(&self.interval_ms),
                "cpu_samples_ms": self.cpu_ms,
                "interval_samples_ms": self.interval_ms,
                "note": "eframe CPU time includes UI/render submission, excludes vsync wait; no GPU timestamps or energy measurement",
            });
            match std::fs::write(
                &self.path,
                serde_json::to_vec_pretty(&report).expect("finite measurements"),
            ) {
                Ok(()) => log::info!("renderer measurements saved"),
                Err(error) => log::error!("could not save renderer measurements: {error}"),
            }
            self.done = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            ctx.request_repaint();
        }
    }
}

fn summary(values: &[f64]) -> serde_json::Value {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let percentile = |p: f64| {
        sorted
            .get(((sorted.len().saturating_sub(1)) as f64 * p).round() as usize)
            .copied()
    };
    serde_json::json!({ "count": sorted.len(), "median": percentile(0.5), "p95": percentile(0.95) })
}
