//! Compare scrolling UI layout without requiring a foreground window.
//! This measures egui passes, not native rendering, GPU time or frame rate.

use std::time::Instant;
use whatsapp::{app::App, demo, paths::AppDirs, settings::Settings};

fn main() -> anyhow::Result<()> {
    let count = std::env::args()
        .nth(1)
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(10_000);
    anyhow::ensure!(
        count >= 120,
        "use at least 120 messages to measure scrolling"
    );
    let root = std::env::temp_dir().join(format!("whatsapp-layout-probe-{}", std::process::id()));
    // Only an isolated, newly created directory may be cleaned up afterwards.
    std::fs::create_dir(&root)?;
    let mut results = Vec::new();
    for run in 0..5 {
        let mut modes = [
            ("scroll", false, false),
            ("selection", false, true),
            ("full-selection", true, true),
        ];
        if run % 2 != 0 {
            modes.reverse();
        }
        for (mode, full, selecting) in modes {
            let (mut app, _events) = App::headless(AppDirs::under(&root), Settings::default());
            demo::populate(&mut app);
            demo::long_history(&mut app, count);
            let ctx = egui::Context::default();
            app.attach(&ctx);
            let chat = app.open_chat.clone().unwrap();
            let viewport = std::sync::Arc::clone(&app.selection_view);
            let mut pass = |events| {
                let mut input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1180.0, 780.0),
                    )),
                    focused: false,
                    events,
                    ..Default::default()
                };
                // Settings apply a zoom factor, so set native scale on every
                // input rather than letting the next input reset it to one.
                input
                    .viewports
                    .get_mut(&egui::ViewportId::ROOT)
                    .unwrap()
                    .native_pixels_per_point = Some(2.0);
                let mut output = ctx.run_ui(input, |ui| {
                    app.background_frame(ui.ctx());
                    app.frame_ui(ui);
                });
                output.textures_delta.clear();
            };
            for _ in 0..8 {
                pass(Vec::new());
            }
            let offset = || {
                ctx.data(|data| data.get_temp::<f32>(egui::Id::new("message-scroll-offset")))
                    .unwrap()
            };
            let first_offset = offset();
            let view = viewport.lock().unwrap().unwrap();
            let start = (0..count)
                .filter_map(|index| {
                    let id = whatsapp::ui::conversation::bubble_id(&chat, &format!("long-{index}"))
                        .with("body");
                    ctx.data(|data| data.get_temp::<egui::Rect>(id))
                        .filter(|rect| view.contains_rect(*rect))
                })
                .min_by(|a, b| {
                    (a.center().y - view.center().y)
                        .abs()
                        .total_cmp(&(b.center().y - view.center().y).abs())
                })
                .map(|rect| rect.min + egui::vec2(3.0, 3.0))
                .expect("a visible text body");
            ctx.data_mut(|data| {
                data.insert_temp(egui::Id::new("benchmark-scroll"), true);
                data.insert_temp(egui::Id::new("full-message-layout"), full);
            });
            let mut samples = Vec::new();
            let passes = ((first_offset / 80.0) as usize)
                .saturating_sub(1)
                .clamp(1, 240);
            for index in 0..passes {
                let events = if selecting && index == 0 {
                    vec![
                        egui::Event::PointerMoved(start),
                        egui::Event::PointerButton {
                            pos: start,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ]
                } else {
                    Vec::new()
                };
                let started = Instant::now();
                pass(events);
                samples.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            let last_offset = offset();
            anyhow::ensure!(
                first_offset - last_offset > (passes.saturating_sub(2) as f32 * 80.0),
                "probe must actually scroll"
            );
            let laid_out = ctx
                .data(|data| data.get_temp::<usize>(egui::Id::new("message-layout-count")))
                .unwrap();
            anyhow::ensure!(
                if full {
                    laid_out == count
                } else if selecting {
                    // Histories larger than the cache budget may need full rows.
                    laid_out <= count
                } else {
                    laid_out < 50
                },
                "row count"
            );
            anyhow::ensure!(
                !selecting
                    || ctx
                        .plugin::<egui::text_selection::LabelSelectionState>()
                        .lock()
                        .has_selection(),
                "the probe must keep a real text selection"
            );
            let registered = app.copy_rows.lock().unwrap().len();
            anyhow::ensure!(
                !selecting || registered == count,
                "all selected rows must remain registered"
            );
            let mut sorted = samples.clone();
            sorted.sort_by(f64::total_cmp);
            eprintln!(
                "run {run}, {mode}: {:.3} ms median; {laid_out} rows",
                sorted[sorted.len() / 2]
            );
            results.push(serde_json::json!({
                "run": run, "mode": mode, "full_message_layout": full, "loaded_messages": count,
                "pixels_per_point": ctx.pixels_per_point(), "registered_transcript_rows": registered,
                "layout_cache_bytes": app.conversations[&chat].row_heights.estimated_bytes(),
                "messages_laid_out": laid_out, "first_scroll_offset": first_offset,
                "last_scroll_offset": last_offset, "ui_pass_median_ms": sorted[sorted.len()/2],
                "ui_pass_p95_ms": sorted[(sorted.len()-1)*95/100], "first_scroll_pass_ms": samples[0], "samples_ms": samples,
            }));
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "workload": format!("{count} synthetic messages, 1180x780 points, 2 pixels/point, eight warmup passes then up to 240 passes scrolling up 80 points/pass without reaching the top; five alternating runs per mode"),
            "note": "headless egui UI-pass CPU time, no native renderer, tessellation, GPU or energy measurement",
            "runs": results,
        }))?
    );
    std::fs::remove_dir_all(root)?;
    Ok(())
}
