//! Playback for WhatsApp GIFs, animated WebP stickers, and GIF files.
//!
//! Decoding runs off the UI thread. WebP/GIF decode in-process; H.264 MP4 uses
//! VideoToolbox with an OpenH264 fallback. Other MP4 codecs use `ffmpeg` when
//! available. Idle animations are removed from memory.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{ColorImage, TextureHandle, TextureOptions};

mod queue;
mod videotoolbox;

#[cfg(feature = "demo")]
pub mod diagnostics;

/// Maximum frame width or height uploaded to the GPU.
const MAX_WIDTH: u32 = 320;
/// Maximum frames kept per animation.
const MAX_FRAMES: usize = 150;
/// Time an unseen animation remains decoded.
const IDLE: Duration = Duration::from_secs(20);
/// Maximum concurrent decoders.
const MAX_DECODERS: usize = 2;
/// Decoded pixel budget, independent of frame dimensions. GPU playback keeps
/// one texture per animation instead of uploading the entire clip at once.
const MAX_RESIDENT_BYTES: usize = 128 * 1024 * 1024;

static DECODING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Guard for one decoder slot.
struct DecodeSlot;

impl Drop for DecodeSlot {
    fn drop(&mut self) {
        DECODING.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

#[cfg(any(test, feature = "demo"))]
struct Decoded {
    frames: Vec<(ColorImage, Duration)>,
}

struct Playing {
    frames: Vec<(Arc<ColorImage>, Duration)>,
    texture: Option<(usize, TextureHandle)>,
    bytes: usize,
    total: Duration,
    started: Instant,
    last_drawn: Instant,
    receiver: Option<queue::Receiver>,
    complete: bool,
    failed: bool,
}

#[derive(Clone, Default)]
struct Cache(Arc<Mutex<HashMap<PathBuf, Playing>>>);

fn cache(ctx: &egui::Context) -> Cache {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Cache>(egui::Id::new("animations"))
            .clone()
    })
}

/// At most eight unpublished preview frames per decoder (3.125 MiB at 320²).
const QUEUED_FRAMES: usize = 8;
/// Stop a job shortly after its last visible frame. Completed loops keep the
/// longer cache lifetime, but invisible work must not monopolize decoder slots.
const DECODE_IDLE: Duration = Duration::from_secs(1);

enum Update {
    Frame(ColorImage, Duration),
    Reset,
    Complete(bool),
}

type Sink<'a> = dyn FnMut(Update) -> Option<()> + 'a;

/// Current display state for an animated file.
pub enum Frame {
    /// Current animation frame.
    Ready(TextureHandle),
    /// Decode in progress; show the poster.
    Pending,
    /// Unsupported in-app; show the poster and allow opening the file.
    Unavailable,
}

/// Returns the current frame, starting decoding when needed. Schedules the next repaint.
pub fn frame(ctx: &egui::Context, path: &Path) -> Frame {
    let cache = cache(ctx);
    let mut entries = cache.0.lock().unwrap_or_else(|p| p.into_inner());
    let now = Instant::now();
    // Refresh before pruning: a busy UI may not have painted for a while.
    if let Some(playing) = entries.get_mut(path) {
        playing.last_drawn = now;
    }
    receive(&mut entries, now);
    prune(&mut entries, now, MAX_RESIDENT_BYTES);
    if !entries.contains_key(path) {
        // compare/exchange is required because multiple contexts can request
        // previews; a load followed by an increment could overbook the slots.
        if DECODING
            .fetch_update(
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
                |active| (active < MAX_DECODERS).then_some(active + 1),
            )
            .is_err()
        {
            ctx.request_repaint_after(Duration::from_millis(150));
            return Frame::Pending;
        }
        let slot = DecodeSlot;
        let (sender, receiver) = queue::channel();
        let file = path.to_path_buf();
        let decoder_ctx = ctx.clone();
        let mut playing = Playing::new(now);
        playing.receiver = Some(receiver);
        let spawned = std::thread::Builder::new()
            .name("animation-decode".into())
            .spawn(move || {
                let _slot = slot;
                let mut emit = |update| {
                    // A dropped receiver cancels the job, including a producer
                    // blocked by backpressure. Never hold the cache/UI lock here.
                    sender.send(update)?;
                    decoder_ctx.request_repaint();
                    Some(())
                };
                let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    decode_stream(&file, &mut emit)
                }))
                .is_ok_and(|result| result.is_some());
                let _ = emit(Update::Complete(ok));
            });
        if spawned.is_err() {
            playing.receiver = None;
            playing.complete = true;
            playing.failed = true;
        }
        entries.insert(path.to_path_buf(), playing);
    }
    let playing = entries.get_mut(path).expect("inserted above");
    if playing.failed {
        return Frame::Unavailable;
    }
    if playing.frames.is_empty() {
        return Frame::Pending;
    }
    let elapsed = now.saturating_duration_since(playing.started);
    // Until EOF, hold the last available frame instead of looping a partial clip.
    let mut position = if playing.complete {
        Duration::from_nanos((elapsed.as_nanos() % playing.total.as_nanos()) as u64)
    } else {
        elapsed
    };
    let mut chosen = playing.frames.len() - 1;
    for (index, (_, delay)) in playing.frames.iter().enumerate() {
        if position < *delay {
            chosen = index;
            ctx.request_repaint_after((*delay - position).max(Duration::from_millis(10)));
            break;
        }
        position -= *delay;
    }
    match &mut playing.texture {
        Some((uploaded, texture)) => {
            if *uploaded != chosen {
                texture.set(playing.frames[chosen].0.clone(), TextureOptions::LINEAR);
                *uploaded = chosen;
            }
            Frame::Ready(texture.clone())
        }
        None => {
            let texture = ctx.load_texture(
                path.display().to_string(),
                playing.frames[chosen].0.clone(),
                TextureOptions::LINEAR,
            );
            playing.texture = Some((chosen, texture.clone()));
            Frame::Ready(texture)
        }
    }
}

impl Playing {
    fn new(now: Instant) -> Self {
        Self {
            frames: Vec::new(),
            texture: None,
            bytes: 0,
            total: Duration::ZERO,
            started: now,
            last_drawn: now,
            receiver: None,
            complete: false,
            failed: false,
        }
    }

    fn accept(&mut self, update: Update, now: Instant) {
        match update {
            Update::Frame(image, delay) => {
                if self.frames.is_empty() {
                    self.started = now;
                }
                self.bytes += image.pixels.len() * 4;
                self.total += delay;
                self.frames.push((Arc::new(image), delay));
            }
            Update::Reset => {
                self.frames.clear();
                self.bytes = 0;
                self.total = Duration::ZERO;
                if let Some((uploaded, _)) = &mut self.texture {
                    *uploaded = usize::MAX;
                }
            }
            Update::Complete(ok) => {
                self.complete = true;
                self.failed = !ok || self.frames.is_empty();
                self.receiver = None;
                if self.failed {
                    self.frames.clear();
                    self.texture = None;
                    self.bytes = 0;
                }
            }
        }
    }
}

fn receive(entries: &mut HashMap<PathBuf, Playing>, now: Instant) {
    for playing in entries.values_mut() {
        // Bound UI work even if a fast producer keeps replenishing its queue.
        for _ in 0..=QUEUED_FRAMES {
            let Some(receiver) = &playing.receiver else {
                break;
            };
            match receiver.try_recv() {
                Ok(update) => playing.accept(update, now),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    playing.accept(Update::Complete(false), now);
                    break;
                }
            }
        }
    }
}

fn prune(entries: &mut HashMap<PathBuf, Playing>, now: Instant, budget: usize) {
    entries.retain(|_, playing| {
        now.saturating_duration_since(playing.last_drawn)
            < if playing.complete { IDLE } else { DECODE_IDLE }
    });
    let mut resident: usize = entries.values().map(|playing| playing.bytes).sum();
    while resident > budget {
        let victim = entries
            .iter()
            .min_by_key(|(_, playing)| playing.last_drawn)
            .map(|(path, playing)| (path.clone(), playing.bytes));
        let Some((path, bytes)) = victim else { break };
        // Dropping the receiver also releases a blocked producer.
        entries.remove(&path);
        resident -= bytes;
    }
}

/// Drain bounded queues and retire invisible decoders, including while headless.
pub fn maintain(ctx: &egui::Context) {
    let cache = cache(ctx);
    let mut entries = cache.0.lock().unwrap_or_else(|p| p.into_inner());
    let now = Instant::now();
    receive(&mut entries, now);
    prune(&mut entries, now, MAX_RESIDENT_BYTES);
    if let Some(next) = entries
        .values()
        .map(|playing| {
            let idle = if playing.complete { IDLE } else { DECODE_IDLE };
            idle.saturating_sub(now.saturating_duration_since(playing.last_drawn))
        })
        .min()
    {
        ctx.request_repaint_after(next);
    }
}

/// MP4 playback is always available because H.264 decodes in-process.
pub fn can_play_video() -> bool {
    true
}

/// Whether `ffmpeg` is available for other MP4 codecs.
fn ffmpeg_present() -> bool {
    static KNOWN: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *KNOWN.get_or_init(|| {
        Command::new("ffmpeg")
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    })
}

fn decode_stream(path: &Path, emit: &mut Sink<'_>) -> Option<()> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "webp" | "gif" => decode_image(path, &extension, emit),
        _ => decode_video(path, emit),
    }
}

/// Decodes animated GIF with the `image` crate.
fn decode_image(path: &Path, extension: &str, emit: &mut Sink<'_>) -> Option<()> {
    use image::AnimationDecoder;
    if extension != "gif" {
        return decode_webp(path, emit);
    }
    let file = std::fs::File::open(path).ok()?;
    let reader = std::io::BufReader::new(file);
    let frames = image::codecs::gif::GifDecoder::new(reader)
        .ok()?
        .into_frames();
    for frame in frames.take(MAX_FRAMES) {
        let frame = frame.ok()?;
        let (numerator, denominator) = frame.delay().numer_denom_ms();
        let delay = Duration::from_millis(u64::from(numerator / denominator.max(1)).max(20));
        let image = frame.into_buffer();
        emit(Update::Frame(to_color_image(&image), delay))?;
    }
    Some(())
}

/// Decodes animated WebP with libwebp. It returns complete canvas frames,
/// unlike the `image` decoder, which did not apply frame disposal correctly.
fn decode_webp(path: &Path, emit: &mut Sink<'_>) -> Option<()> {
    let bytes = std::fs::read(path).ok()?;
    let decoder = webp_animation::Decoder::new(&bytes).ok()?;
    let (width, height) = decoder.dimensions();
    let mut first = None;
    let mut count = 0;
    let mut previous = 0i64;
    for frame in decoder.into_iter().take(MAX_FRAMES) {
        let image = image::RgbaImage::from_raw(width, height, frame.data().to_vec())?;
        let delay = (i64::from(frame.timestamp()) - previous).max(20) as u64;
        previous = i64::from(frame.timestamp());
        let update = Update::Frame(to_color_image(&image), Duration::from_millis(delay));
        count += 1;
        if count == 1 {
            first = Some(update);
        } else {
            if let Some(first) = first.take() {
                emit(first)?;
            }
            emit(update)?;
        }
    }
    // Do not publish a still WebP as an animation even temporarily.
    (count > 1).then_some(())
}

fn to_color_image(image: &image::RgbaImage) -> ColorImage {
    let largest = image.width().max(image.height());
    let image = if largest > MAX_WIDTH {
        let width =
            ((u64::from(image.width()) * u64::from(MAX_WIDTH)) / u64::from(largest)).max(1) as u32;
        let height =
            ((u64::from(image.height()) * u64::from(MAX_WIDTH)) / u64::from(largest)).max(1) as u32;
        image::imageops::resize(image, width, height, image::imageops::FilterType::Triangle)
    } else {
        image.clone()
    };
    ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    )
}

/// Try each backend, resetting already-published frames before a fallback.
fn decode_video(path: &Path, emit: &mut Sink<'_>) -> Option<()> {
    if videotoolbox::stream(path, emit).is_some() {
        return Some(());
    }
    emit(Update::Reset)?;
    if stream_mp4(path, emit).is_some() {
        return Some(());
    }
    emit(Update::Reset)?;
    decode_with_ffmpeg(path, emit)
}

#[cfg(any(test, feature = "demo"))]
fn collect(run: impl FnOnce(&mut Sink<'_>) -> Option<()>) -> Option<Decoded> {
    let mut frames = Vec::new();
    run(&mut |update| {
        match update {
            Update::Frame(image, delay) => frames.push((image, delay)),
            Update::Reset => frames.clear(),
            Update::Complete(_) => unreachable!("completion belongs to the worker"),
        }
        Some(())
    })?;
    (!frames.is_empty()).then_some(Decoded { frames })
}

#[cfg(test)]
fn decode(path: &Path) -> Option<Decoded> {
    collect(|emit| decode_stream(path, emit))
}

#[cfg(any(test, feature = "demo"))]
fn decode_mp4(path: &Path) -> Option<Decoded> {
    collect(|emit| stream_mp4(path, emit))
}

/// Decodes an MP4 video track in-process.
fn stream_mp4(path: &Path, emit: &mut Sink<'_>) -> Option<()> {
    let file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    let mut mp4 = mp4::Mp4Reader::read_header(std::io::BufReader::new(file), size).ok()?;
    let (track_id, timescale, sps, pps, count) = {
        let track = mp4
            .tracks()
            .values()
            .find(|track| track.track_type().ok() == Some(mp4::TrackType::Video))?;
        (
            track.track_id(),
            u64::from(track.timescale().max(1)),
            track.sequence_parameter_set().ok()?.to_vec(),
            track.picture_parameter_set().ok()?.to_vec(),
            track.sample_count(),
        )
    };
    let config = openh264::decoder::DecoderConfig::new()
        .flush_after_decode(openh264::decoder::Flush::NoFlush);
    let mut decoder =
        openh264::decoder::Decoder::with_api_config(openh264::OpenH264API::from_source(), config)
            .ok()?;
    let mut produced = 0;
    let mut delays: std::collections::VecDeque<Duration> = std::collections::VecDeque::new();
    // Send parameter sets and samples to the decoder in Annex B format.
    let mut parameters = Vec::new();
    push_annex_b(&mut parameters, &sps);
    push_annex_b(&mut parameters, &pps);
    let _ = decoder.decode(&parameters);
    for sample_id in 1..=count {
        if produced >= MAX_FRAMES {
            break;
        }
        let Ok(Some(sample)) = mp4.read_sample(track_id, sample_id) else {
            break;
        };
        let delay =
            Duration::from_millis((u64::from(sample.duration) * 1000 / timescale).clamp(20, 1000));
        delays.push_back(delay);
        let mut annex_b = Vec::with_capacity(sample.bytes.len() + 16);
        avcc_to_annex_b(&mut annex_b, &sample.bytes);
        if let Ok(Some(yuv)) = decoder.decode(&annex_b) {
            let delay = delays.pop_front().unwrap_or(delay);
            if let Some(frame) = frame_of(&yuv, delay) {
                emit(Update::Frame(frame.0, frame.1))?;
                produced += 1;
            }
        }
    }
    // Mark end-of-stream before flushing. Otherwise OpenH264 keeps the final
    // reorder buffer (commonly two B frames) even after flush_remaining().
    if produced < MAX_FRAMES
        && let Ok(Some(yuv)) = decoder.decode(&[])
    {
        let delay = delays.pop_front().unwrap_or(Duration::from_millis(66));
        if let Some(frame) = frame_of(&yuv, delay) {
            emit(Update::Frame(frame.0, frame.1))?;
            produced += 1;
        }
    }
    if let Ok(rest) = decoder.flush_remaining() {
        for yuv in &rest {
            if produced >= MAX_FRAMES {
                break;
            }
            let delay = delays.pop_front().unwrap_or(Duration::from_millis(66));
            if let Some(frame) = frame_of(yuv, delay) {
                emit(Update::Frame(frame.0, frame.1))?;
                produced += 1;
            }
        }
    }
    (produced > 0).then_some(())
}

/// Converts and scales one decoded frame.
fn frame_of(
    yuv: &openh264::decoder::DecodedYUV<'_>,
    delay: Duration,
) -> Option<(ColorImage, Duration)> {
    use openh264::formats::YUVSource;

    let (width, height) = yuv.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    let mut rgba = vec![0u8; width * height * 4];
    yuv.write_rgba8(&mut rgba);
    let image = image::RgbaImage::from_raw(width as u32, height as u32, rgba)?;
    Some((to_color_image(&image), delay))
}

fn push_annex_b(out: &mut Vec<u8>, nal: &[u8]) {
    out.extend_from_slice(&[0, 0, 0, 1]);
    out.extend_from_slice(nal);
}

/// Converts length-prefixed AVCC NAL units to Annex B start codes.
fn avcc_to_annex_b(out: &mut Vec<u8>, sample: &[u8]) {
    let mut rest = sample;
    while rest.len() >= 4 {
        let length = u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize;
        rest = &rest[4..];
        if length == 0 || length > rest.len() {
            break;
        }
        push_annex_b(out, &rest[..length]);
        rest = &rest[length..];
    }
}

fn decode_with_ffmpeg(path: &Path, emit: &mut Sink<'_>) -> Option<()> {
    if !ffmpeg_present() {
        return None;
    }
    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .ok()?;
    let dimensions = String::from_utf8_lossy(&probe.stdout);
    let mut parts = dimensions.trim().split(',');
    let width: u32 = parts.next()?.trim().parse().ok()?;
    let height: u32 = parts.next()?.trim().parse().ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    // Bound both dimensions, including portrait clips. RGBA output permits
    // odd dimensions, so tiny frames do not need to be stretched to even sizes.
    let largest = width.max(height).max(MAX_WIDTH);
    let out_width = (u64::from(width) * u64::from(MAX_WIDTH) / u64::from(largest)).max(1) as u32;
    let out_height = (u64::from(height) * u64::from(MAX_WIDTH) / u64::from(largest)).max(1) as u32;
    let fps = 15u32;
    let mut child = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args([
            "-an",
            "-vf",
            &format!("fps={fps},scale={out_width}:{out_height}"),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let frame_bytes = (out_width * out_height * 4) as usize;
    let mut produced = 0;
    let delay = Duration::from_millis(1000 / u64::from(fps));
    let mut buffer = vec![0u8; frame_bytes];
    let mut cancelled = false;
    while produced < MAX_FRAMES {
        if stdout.read_exact(&mut buffer).is_err() {
            break;
        }
        if emit(Update::Frame(
            ColorImage::from_rgba_unmultiplied([out_width as usize, out_height as usize], &buffer),
            delay,
        ))
        .is_none()
        {
            cancelled = true;
            break;
        }
        produced += 1;
    }
    let _ = child.kill();
    let _ = child.wait();
    (produced > 0 && !cancelled).then_some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(side: usize) -> Decoded {
        Decoded {
            frames: [egui::Color32::RED, egui::Color32::BLUE]
                .into_iter()
                .map(|color| {
                    (
                        ColorImage::filled([side, side], color),
                        Duration::from_secs(10),
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn tall_frames_are_bounded_without_stretching_small_images() {
        assert_eq!(
            to_color_image(&image::RgbaImage::new(100, 2000)).size,
            [16, 320]
        );
        assert_eq!(
            to_color_image(&image::RgbaImage::new(2000, 100)).size,
            [320, 16]
        );
        assert_eq!(
            to_color_image(&image::RgbaImage::new(21, 17)).size,
            [21, 17]
        );
    }

    fn install(ctx: &egui::Context, path: &Path, decoded: Decoded, complete: bool) {
        let now = Instant::now();
        let mut playing = Playing::new(now);
        for (image, delay) in decoded.frames {
            playing.accept(Update::Frame(image, delay), now);
        }
        if complete {
            playing.accept(Update::Complete(true), now);
        }
        cache(ctx)
            .0
            .lock()
            .unwrap()
            .insert(path.to_owned(), playing);
    }

    #[test]
    fn playback_uploads_one_frame_and_reuses_its_texture() {
        let ctx = egui::Context::default();
        let path = PathBuf::from("test-animation");
        install(&ctx, &path, clip(8), true);
        let Frame::Ready(first) = frame(&ctx, &path) else {
            panic!("ready")
        };
        let mut first_delta = ctx.tex_manager().write().take_delta();
        assert_eq!(
            first_delta
                .set
                .iter()
                .filter(|(id, _)| **id == first.id())
                .count(),
            1
        );
        first_delta.clear();
        let _ = frame(&ctx, &path);
        assert!(ctx.tex_manager().write().take_delta().set.is_empty());
        cache(&ctx)
            .0
            .lock()
            .unwrap()
            .get_mut(&path)
            .unwrap()
            .started = Instant::now() - Duration::from_secs(11);
        let Frame::Ready(second) = frame(&ctx, &path) else {
            panic!("ready")
        };
        assert_eq!(first.id(), second.id());
        let mut delta = ctx.tex_manager().write().take_delta();
        assert_eq!(delta.set.len(), 1);
        let egui::ImageData::Color(image) = &delta.set[&second.id()][0].image;
        assert_eq!(image.pixels[0], egui::Color32::BLUE);
        delta.clear();
    }

    #[test]
    fn unfinished_playback_holds_its_last_frame_and_fallback_replaces_pixels() {
        let ctx = egui::Context::default();
        let path = PathBuf::from("streaming-animation");
        install(&ctx, &path, clip(8), false);
        cache(&ctx)
            .0
            .lock()
            .unwrap()
            .get_mut(&path)
            .unwrap()
            .started = Instant::now() - Duration::from_secs(25);
        let Frame::Ready(first) = frame(&ctx, &path) else {
            panic!("first frames play before EOF")
        };
        let mut delta = ctx.tex_manager().write().take_delta();
        let egui::ImageData::Color(image) = &delta.set[&first.id()][0].image;
        assert_eq!(
            image.pixels[0],
            egui::Color32::BLUE,
            "do not loop before EOF"
        );
        delta.clear();
        {
            let cache = cache(&ctx);
            let mut entries = cache.0.lock().unwrap();
            let playing = entries.get_mut(&path).unwrap();
            playing.accept(Update::Reset, Instant::now());
            playing.accept(
                Update::Frame(
                    ColorImage::filled([8, 8], egui::Color32::GREEN),
                    Duration::from_secs(10),
                ),
                Instant::now(),
            );
            playing.accept(Update::Complete(true), Instant::now());
            assert_eq!(playing.bytes, 8 * 8 * 4);
        }
        let Frame::Ready(second) = frame(&ctx, &path) else {
            panic!("fallback ready")
        };
        assert_eq!(first.id(), second.id(), "fallback also reuses the texture");
        let mut delta = ctx.tex_manager().write().take_delta();
        let egui::ImageData::Color(image) = &delta.set[&second.id()][0].image;
        assert_eq!(image.pixels[0], egui::Color32::GREEN);
        delta.clear();
    }

    #[test]
    fn cache_evicts_by_bytes_and_drains_completed_decodes_without_media_in_view() {
        let ctx = egui::Context::default();
        let path = PathBuf::from("offscreen");
        let (sender, receiver) = queue::channel();
        let mut playing = Playing::new(Instant::now());
        playing.receiver = Some(receiver);
        cache(&ctx).0.lock().unwrap().insert(path.clone(), playing);
        for (image, delay) in clip(16).frames {
            sender.send(Update::Frame(image, delay)).unwrap();
        }
        sender.send(Update::Complete(true)).unwrap();
        maintain(&ctx);
        let cache = cache(&ctx);
        let mut entries = cache.0.lock().unwrap();
        let now = Instant::now();
        let playing = entries.get_mut(&path).unwrap();
        assert!(playing.complete);
        assert!(playing.texture.is_none(), "offscreen frames never upload");
        playing.last_drawn = now - IDLE;
        drop(entries);
        maintain(&ctx);
        assert!(cache.0.lock().unwrap().is_empty());
        install(&ctx, Path::new("old"), clip(16), true);
        install(&ctx, Path::new("new"), clip(8), true);
        let mut entries = cache.0.lock().unwrap();
        prune(&mut entries, now, 8 * 8 * 4 * 2);
        assert_eq!(entries.len(), 1);
        assert!(entries.contains_key(Path::new("new")));
    }

    #[test]
    fn retiring_an_invisible_stream_disconnects_its_producer() {
        let ctx = egui::Context::default();
        let (sender, receiver) = queue::channel();
        let mut playing = Playing::new(Instant::now() - DECODE_IDLE);
        playing.receiver = Some(receiver);
        cache(&ctx)
            .0
            .lock()
            .unwrap()
            .insert("hidden".into(), playing);
        maintain(&ctx);
        assert!(sender.send(Update::Reset).is_none());
        assert!(cache(&ctx).0.lock().unwrap().is_empty());
    }

    #[test]
    fn a_bounded_stream_delivers_before_eof_and_cancels_a_blocked_decoder() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let path = std::env::temp_dir().join(format!("zapfast-stream-{}.gif", std::process::id()));
        {
            let mut encoder =
                image::codecs::gif::GifEncoder::new(std::fs::File::create(&path).unwrap());
            for shade in 0..150 {
                encoder
                    .encode_frame(image::Frame::from_parts(
                        image::RgbaImage::from_pixel(8, 8, image::Rgba([shade, 0, 0, 255])),
                        0,
                        0,
                        image::Delay::from_numer_denom_ms(100, 1),
                    ))
                    .unwrap();
            }
        }
        let (sender, receiver) = queue::channel();
        let produced = Arc::new(AtomicUsize::new(0));
        let count = produced.clone();
        let file = path.clone();
        let worker = std::thread::spawn(move || {
            decode_stream(&file, &mut |update| {
                sender.send(update)?;
                count.fetch_add(1, Ordering::Release);
                Some(())
            })
        });
        let first_update = loop {
            match receiver.try_recv() {
                Ok(update) => break update,
                Err(std::sync::mpsc::TryRecvError::Empty) => std::thread::yield_now(),
                Err(error) => panic!("{error}"),
            }
        };
        let Update::Frame(first, delay) = first_update else {
            panic!("first frame before EOF")
        };
        assert_eq!(first.size, [8, 8]);
        assert_eq!(delay, Duration::from_millis(100));
        assert!(produced.load(Ordering::Acquire) <= QUEUED_FRAMES + 1);
        drop(receiver);
        assert!(
            worker.join().unwrap().is_none(),
            "cancellation stops before frame 150"
        );
        std::fs::remove_file(path).unwrap();
    }

    /// Verifies animated WebP frame disposal.
    #[test]
    fn a_moving_subject_leaves_no_trace_behind() {
        use webp_animation::prelude::*;
        let side = 64u32;
        let square = |x0: u32, y0: u32, color: [u8; 4]| {
            let mut frame = vec![0u8; (side * side * 4) as usize];
            for y in y0..y0 + 16 {
                for x in x0..x0 + 16 {
                    let at = ((y * side + x) * 4) as usize;
                    frame[at..at + 4].copy_from_slice(&color);
                }
            }
            frame
        };
        let mut encoder = Encoder::new((side, side)).expect("encoder");
        encoder
            .add_frame(&square(0, 0, [255, 0, 0, 255]), 0)
            .expect("frame");
        encoder
            .add_frame(&square(40, 40, [0, 255, 0, 255]), 100)
            .expect("frame");
        let webp = encoder.finalize(200).expect("finalizes");
        let dir = std::env::temp_dir().join(format!("zapfast-ghost-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("moving.webp");
        std::fs::write(&path, &webp).expect("writes");
        let decoded = decode(&path).expect("decodes");
        assert_eq!(decoded.frames.len(), 2);
        let second = &decoded.frames[1].0;
        let old = second.pixels[8 * second.width() + 8];
        assert_eq!(old.a(), 0, "the first frame's square is gone: {old:?}");
        let new = second.pixels[48 * second.width() + 48];
        assert!(new.a() > 200, "the second frame's square shows: {new:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn animated_webp_decodes_into_frames() {
        // Two frames 100 ms apart.
        let dir = std::env::temp_dir().join(format!("zapfast-anim-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("two.gif");
        {
            let file = std::fs::File::create(&path).expect("file");
            let mut encoder = image::codecs::gif::GifEncoder::new(file);
            encoder
                .set_repeat(image::codecs::gif::Repeat::Infinite)
                .expect("repeat");
            for shade in [40u8, 200u8] {
                let frame = image::Frame::from_parts(
                    image::RgbaImage::from_pixel(8, 8, image::Rgba([shade, shade, shade, 255])),
                    0,
                    0,
                    image::Delay::from_numer_denom_ms(100, 1),
                );
                encoder.encode_frame(frame).expect("frame");
            }
        }
        let decoded = decode(&path).expect("decodes");
        assert_eq!(decoded.frames.len(), 2);
        assert_eq!(decoded.frames[0].1, Duration::from_millis(100));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_mp4_made_by_ffmpeg_decodes_in_process() {
        if !can_play_video() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("zapfast-mp4-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("clip.mp4");
        let made = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=0.5:size=64x48:rate=10",
            ])
            .args(["-pix_fmt", "yuv420p"])
            .arg(&path)
            .status()
            .is_ok_and(|status| status.success());
        if !made {
            // Skip when this ffmpeg lacks the encoder.
            return;
        }
        let decoded = decode(&path).expect("decodes");
        // Five frames at 10 fps. The in-process path preserves their timing.
        assert_eq!(decoded.frames.len(), 5);
        assert_eq!(decoded.frames[0].1, Duration::from_millis(100));
        assert_eq!(decoded.frames[0].0.size, [64, 48]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_still_webp_is_not_an_animation() {
        let dir = std::env::temp_dir().join(format!("zapfast-still-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("still.webp");
        image::RgbaImage::from_pixel(4, 4, image::Rgba([1, 2, 3, 255]))
            .save(&path)
            .expect("saves");
        assert!(decode(&path).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod probe {
    use super::*;

    /// Decodes the file in `ZAPFAST_MP4_PROBE`:
    /// `ZAPFAST_MP4_PROBE=some.mp4 cargo test --all-features probe -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs a file to look at"]
    fn decodes_the_file_named_by_the_environment() {
        let Some(path) = std::env::var_os("ZAPFAST_MP4_PROBE") else {
            return;
        };
        let started = Instant::now();
        let decoded = decode_mp4(Path::new(&path)).expect("decodes in-process");
        eprintln!(
            "{} frames of {:?}, first delay {:?}, in {:?}",
            decoded.frames.len(),
            decoded.frames[0].0.size,
            decoded.frames[0].1,
            started.elapsed()
        );
        assert!(!decoded.frames.is_empty());
    }
}
