//! Compare scrolling UI layout without requiring a foreground window.
//! This measures egui passes, not native rendering, GPU time or frame rate.

use std::time::Instant;
use whatsapp::{app::App, demo, paths::AppDirs, settings::Settings};

fn main() -> anyhow::Result<()> {
    let root = std::env::temp_dir().join(format!("whatsapp-layout-probe-{}", std::process::id()));
    // Only an isolated, newly created directory may be cleaned up afterwards.
    std::fs::create_dir(&root)?;
    let mut results = Vec::new();
    for run in 0..5 {
        for full in if run % 2 == 0 {
            [true, false]
        } else {
            [false, true]
        } {
            let (mut app, _events) = App::headless(AppDirs::under(&root), Settings::default());
            demo::populate(&mut app);
            demo::long_history(&mut app, 10_000);
            let ctx = egui::Context::default();
            app.attach(&ctx);
            ctx.set_pixels_per_point(2.0);
            ctx.data_mut(|data| data.insert_temp(egui::Id::new("full-message-layout"), full));
            let mut pass = || {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1180.0, 780.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        app.background_frame(ui.ctx());
                        app.frame_ui(ui);
                    },
                );
                output.textures_delta.clear();
            };
            for _ in 0..8 {
                pass();
            }
            let offset = || {
                ctx.data(|data| data.get_temp::<f32>(egui::Id::new("message-scroll-offset")))
                    .unwrap()
            };
            let first_offset = offset();
            ctx.data_mut(|data| data.insert_temp(egui::Id::new("benchmark-scroll"), true));
            let mut samples = Vec::new();
            for _ in 0..240 {
                let started = Instant::now();
                pass();
                samples.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            let last_offset = offset();
            anyhow::ensure!(
                first_offset - last_offset > 18_000.0,
                "probe must actually scroll"
            );
            let laid_out = ctx
                .data(|data| data.get_temp::<usize>(egui::Id::new("message-layout-count")))
                .unwrap();
            anyhow::ensure!(
                if full {
                    laid_out == 10_000
                } else {
                    laid_out < 50
                },
                "row count"
            );
            let mut sorted = samples.clone();
            sorted.sort_by(f64::total_cmp);
            eprintln!(
                "run {run}, full={full}: {:.3} ms median; {laid_out} rows",
                sorted[120]
            );
            results.push(serde_json::json!({
                "run": run, "full_message_layout": full, "loaded_messages": 10_000,
                "messages_laid_out": laid_out, "first_scroll_offset": first_offset,
                "last_scroll_offset": last_offset, "ui_pass_median_ms": sorted[120],
                "ui_pass_p95_ms": sorted[227], "samples_ms": samples,
            }));
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "workload": "10000 synthetic messages, 1180x780 points, 2 pixels/point, eight warmup passes then 240 passes scrolling up 80 points/pass; five alternating runs per mode",
            "note": "headless egui UI-pass CPU time, no native renderer, tessellation, GPU or energy measurement",
            "runs": results,
        }))?
    );
    std::fs::remove_dir_all(root)?;
    Ok(())
}
