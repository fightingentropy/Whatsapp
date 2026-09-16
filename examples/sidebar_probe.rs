//! Measure steady-state sidebar work without a native window or live account.
//! Timings include the whole demo UI pass, but no rendering or GPU work.

use std::time::Instant;
use zapfast::{app::App, demo, paths::AppDirs, settings::Settings};

fn main() -> anyhow::Result<()> {
    let root = std::env::temp_dir().join(format!("zapfast-sidebar-probe-{}", std::process::id()));
    std::fs::create_dir(&root)?;
    let mut results = Vec::new();
    for count in [1_000, 10_000] {
        for run in 0..5 {
            let (mut app, _events) = App::headless(AppDirs::under(&root), Settings::default());
            demo::populate(&mut app);
            demo::many_chats(&mut app, count);
            let ctx = egui::Context::default();
            app.attach(&ctx);
            ctx.set_pixels_per_point(2.0);
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
            let mut samples = Vec::new();
            for _ in 0..240 {
                let started = Instant::now();
                pass();
                samples.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            let mut sorted = samples.clone();
            sorted.sort_by(f64::total_cmp);
            eprintln!("{count} chats, run {run}: {:.3} ms median", sorted[120]);
            results.push(serde_json::json!({
                "run": run, "chats": count,
                "ui_pass_median_ms": sorted[120],
                "ui_pass_p95_ms": sorted[227], "samples_ms": samples,
            }));
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "workload": "1000/10000 synthetic chats; half of added chats are groups with 64 members; 1180x780 points, two pixels/point, eight warmup passes then 240 unchanged passes, five runs per size",
            "note": "headless whole-demo egui UI-pass CPU time, no native rendering, tessellation, GPU or energy measurement; the application does not continuously repaint while idle",
            "runs": results,
        }))?
    );
    std::fs::remove_dir_all(root)?;
    Ok(())
}
