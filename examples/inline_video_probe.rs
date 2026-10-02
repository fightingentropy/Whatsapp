//! Offline AVFoundation regression probe: decoded pixels, audio track, seek,
//! pause, end/replay, switching and invalid media. Never opens a linked session.
use std::path::Path;
use std::time::{Duration, Instant};
use whatsapp::video::{Player, State};

fn apply(app: &mut whatsapp::app::App, ctx: &egui::Context, action: whatsapp::model::Action) {
    app.actions.push(action);
    ctx.begin_pass(egui::RawInput::default());
    app.background_frame(ctx);
    ctx.end_pass().textures_delta.clear();
}

fn preview_input(
    app: &mut whatsapp::app::App,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    elapsed: f64,
) {
    let time = ctx.input(|input| input.time) + elapsed;
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1180.0, 850.0),
            )),
            time: Some(time),
            events,
            ..Default::default()
        },
        |ui| app.frame_ui(ui),
    )
    .textures_delta
    .clear();
}

fn overlay_controls(app: &mut whatsapp::app::App, ctx: &egui::Context) {
    app.attach(ctx);
    for _ in 0..3 {
        preview_input(app, ctx, Vec::new(), 0.02);
    }
    let rect = ctx
        .data(|data| data.get_temp::<egui::Rect>(egui::Id::new("video-preview-canvas")))
        .unwrap();
    let visible = || {
        ctx.data(|data| data.get_temp::<bool>(egui::Id::new("video-preview-controls-visible")))
            .unwrap()
    };
    preview_input(
        app,
        ctx,
        vec![egui::Event::PointerMoved(rect.center())],
        0.02,
    );
    assert!(visible(), "moving over the video reveals controls");
    preview_input(app, ctx, Vec::new(), 3.0);
    assert!(!visible(), "idle playback shows only the video");
    let toggle = egui::pos2(rect.left() + 20.0, rect.bottom() - 23.0);
    for playing in [false, true] {
        preview_input(app, ctx, vec![egui::Event::PointerMoved(toggle)], 0.02);
        for pressed in [true, false] {
            preview_input(
                app,
                ctx,
                vec![egui::Event::PointerButton {
                    pos: toggle,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
                0.02,
            );
        }
        assert_eq!(
            app.video.active().unwrap().is_playing(),
            playing,
            "the overlaid button must toggle playback exactly once"
        );
    }
    let close = rect.right_top() + egui::vec2(-24.0, 24.0);
    preview_input(app, ctx, vec![egui::Event::PointerMoved(close)], 0.02);
    for pressed in [true, false] {
        preview_input(
            app,
            ctx,
            vec![egui::Event::PointerButton {
                pos: close,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
            0.02,
        );
    }
    assert!(app.video_preview.is_none());
    assert!(
        app.video.active().unwrap().is_playing(),
        "closing the overlay must not toggle the video underneath"
    );
}

fn expanded_playback(ctx: &egui::Context) {
    use whatsapp::{app::App, model::Action, paths::AppDirs, settings::Settings};
    let root = tempfile::tempdir().unwrap();
    let (mut app, _events) = App::headless(AppDirs::under(root.path()), Settings::default());
    whatsapp::demo::populate(&mut app);
    whatsapp::demo::apply_flags(&mut app, Some("inline-video"));
    let chat = app.open_chat.clone().unwrap();
    let message = "demo-inline-video".to_owned();
    apply(
        &mut app,
        ctx,
        Action::PlayVideo {
            chat: chat.clone(),
            message: message.clone(),
        },
    );
    apply(
        &mut app,
        ctx,
        Action::MuteVideo {
            chat: chat.clone(),
            message: message.clone(),
        },
    );
    until(
        &mut app.video,
        ctx,
        "preview fixture ready",
        |player, color| player.active().unwrap().position > 0.15 && color.is_some(),
    );
    app.video.pause();
    app.video.seek(&chat, &message, 0.5);
    until(
        &mut app.video,
        ctx,
        "preview fixture seeks before expanding",
        |player, color| (player.active().unwrap().position - 3.0).abs() < 0.1 && color.is_some(),
    );
    let texture = app.video.active().unwrap().texture.as_ref().unwrap().id();
    let preview = Action::PreviewVideo {
        chat: chat.clone(),
        message: message.clone(),
    };
    // A double-click must resume the first click's pause and retain the clock,
    // texture, audio and mute state. Expanding an already playing clip is idempotent.
    for _ in 0..2 {
        apply(&mut app, ctx, preview.clone());
        assert_eq!(app.video_preview, Some((chat.clone(), message.clone())));
        let active = app.video.active().unwrap();
        assert!(active.is_playing() && active.has_audio && active.muted);
        assert!((active.position - 3.0).abs() < 0.2);
        assert_eq!(active.texture.as_ref().unwrap().id(), texture);
    }
    apply(&mut app, ctx, Action::CloseVideoPreview);
    assert!(app.video_preview.is_none());
    let active = app.video.active().unwrap();
    assert!(active.is_playing() && active.muted);
    assert!((active.position - 3.0).abs() < 0.2);
    assert_eq!(active.texture.as_ref().unwrap().id(), texture);
    apply(&mut app, ctx, preview);
    overlay_controls(&mut app, ctx);
    app.video.stop();
}

fn step(player: &mut Player, ctx: &egui::Context) -> Option<[u8; 3]> {
    // AVFoundation completions use the main run loop.
    unsafe {
        core_foundation_sys::runloop::CFRunLoopRunInMode(
            core_foundation_sys::runloop::kCFRunLoopDefaultMode,
            0.01,
            false as u8,
        );
    }
    ctx.begin_pass(egui::RawInput::default());
    player.poll(ctx);
    let texture = player
        .active()
        .and_then(|p| p.texture.as_ref())
        .map(egui::TextureHandle::id);
    let mut output = ctx.end_pass();
    let mut color = None;
    for (id, deltas) in &output.textures_delta.set {
        if Some(*id) == texture
            && let Some(delta) = deltas.last()
        {
            let egui::ImageData::Color(image) = &delta.image;
            assert!(image.size[0] <= 640 && image.size[1] <= 640);
            let pixel = image.pixels[image.pixels.len() / 2 + image.size[0] / 2];
            color = Some([pixel.r(), pixel.g(), pixel.b()]);
        }
    }
    output.textures_delta.clear();
    color
}

fn until(
    player: &mut Player,
    ctx: &egui::Context,
    description: &str,
    condition: impl Fn(&Player, Option<[u8; 3]>) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let color = step(player, ctx);
        if condition(player, color) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out: {description}; state={:?}, position={:?}",
            player.active().map(|p| p.state),
            player.active().map(|p| p.position)
        );
    }
}

fn main() {
    let ctx = egui::Context::default();
    let mut player = Player::default();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let file = root.join("inline-video.mp4");
    player.toggle("fixture", "video", &file).unwrap();
    player.toggle_mute("fixture", "video");
    until(
        &mut player,
        &ctx,
        "first decoded red frame with audio",
        |player, color| {
            let active = player.active().unwrap();
            active.has_audio
                && active.position > 0.15
                && color.is_some_and(|[r, g, b]| r > 200 && g < 20 && b < 20)
        },
    );
    player.pause();
    let paused = player.active().unwrap().position;
    let until_paused = Instant::now() + Duration::from_millis(200);
    while Instant::now() < until_paused {
        step(&mut player, &ctx);
    }
    assert!((player.active().unwrap().position - paused).abs() < 0.1);
    assert_eq!(player.active().unwrap().state, State::Paused);
    player.seek("fixture", "video", 0.75);
    until(
        &mut player,
        &ctx,
        "paused seek decodes the blue frame",
        |player, color| {
            let active = player.active().unwrap();
            active.state == State::Paused
                && (active.position - 4.5).abs() < 0.1
                && color.is_some_and(|[r, g, b]| b > 200 && r < 20 && g < 20)
        },
    );
    player.seek("wrong-chat", "video", 0.0);
    assert!(player.active().unwrap().position > 4.4);
    player.seek("fixture", "video", 1.0);
    until(&mut player, &ctx, "seek to the exact end", |player, _| {
        player.active().unwrap().state == State::Ended
    });
    player.seek("fixture", "video", 0.25);
    until(
        &mut player,
        &ctx,
        "backward seek decodes red",
        |_, color| color.is_some_and(|[r, g, b]| r > 200 && g < 20 && b < 20),
    );
    player.toggle_mute("fixture", "video");
    assert!(!player.active().unwrap().muted);
    player.toggle_mute("fixture", "video");
    player.toggle("fixture", "video", &file).unwrap();
    player.seek("fixture", "video", 0.98);
    until(&mut player, &ctx, "natural end", |player, _| {
        player.active().unwrap().state == State::Ended
    });
    player.toggle("fixture", "video", &file).unwrap();
    until(
        &mut player,
        &ctx,
        "replay decodes red from the beginning",
        |player, color| {
            player.active().unwrap().position < 1.0
                && color.is_some_and(|[r, g, b]| r > 200 && g < 20 && b < 20)
        },
    );
    player
        .toggle(
            "second-chat",
            "video",
            &root.join("inline-video-portrait.mp4"),
        )
        .unwrap();
    player.toggle_mute("second-chat", "video");
    until(
        &mut player,
        &ctx,
        "switch to portrait clip",
        |player, color| player.active().unwrap().matches("second-chat", "video") && color.is_some(),
    );
    // Check the native track transform as painted, not just synthetic geometry.
    player.pause();
    let displayed = player.active().unwrap().display_size().unwrap();
    assert!(
        (displayed.x / displayed.y - 9.0 / 16.0).abs() < 0.001,
        "expanded playback must use the rotated track's aspect ratio"
    );
    ctx.begin_pass(egui::RawInput::default());
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(180.0, 320.0));
    player
        .active()
        .unwrap()
        .paint(&ctx.layer_painter(egui::LayerId::background()), rect);
    let mut output = ctx.end_pass();
    let mesh = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Mesh(mesh) => Some(mesh),
            _ => None,
        })
        .expect("the rotated frame mesh");
    let corners: Vec<_> = mesh.vertices.iter().map(|vertex| vertex.pos).collect();
    assert_eq!(egui::Rect::from_points(&corners), rect);
    assert_eq!(
        corners[0],
        rect.left_bottom(),
        "the top-left marker rotates to the bottom-left"
    );
    output.textures_delta.clear();
    player.stop();
    assert!(player.active().is_none());
    let broken = tempfile::Builder::new().suffix(".mp4").tempfile().unwrap();
    std::fs::write(broken.path(), b"not a movie").unwrap();
    player.toggle("fixture", "broken", broken.path()).unwrap();
    until(&mut player, &ctx, "invalid media failure", |player, _| {
        player.active().unwrap().state == State::Failed
    });
    player.stop();
    expanded_playback(&ctx);
    println!(
        "PASS: decoded color frames, enabled audio track, pause, forward/backward seek, mute, end/replay, portrait rotation, clip switching, bounded textures, expanded playback continuity and invalid-media failure"
    );
}
