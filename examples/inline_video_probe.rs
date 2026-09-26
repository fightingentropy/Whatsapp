//! Offline AVFoundation regression probe: decoded pixels, audio track, seek,
//! pause, end/replay, switching and invalid media. Never opens a linked session.
use std::path::Path;
use std::time::{Duration, Instant};
use whatsapp::video::{Player, State};

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
    println!(
        "PASS: decoded color frames, enabled audio track, pause, forward/backward seek, mute, end/replay, portrait rotation, clip switching, bounded textures and invalid-media failure"
    );
}
