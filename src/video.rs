//! One native, on-demand video player. AVFoundation owns decoding and the audio
//! clock; egui displays only its current frame, never a cache of the whole movie.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::{Color32, ColorImage, Pos2, Rect, TextureHandle, TextureOptions};
use objc2::{AllocAnyThread, MainThreadMarker, MainThreadOnly, rc::Retained, runtime::AnyObject};
use objc2_av_foundation::{
    AVMediaTypeAudio, AVMediaTypeVideo, AVPlayer, AVPlayerItem, AVPlayerItemStatus,
    AVPlayerItemVideoOutput, AVPlayerStatus,
};
use objc2_core_media::{CMTime, CMTimeFlags};
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow,
    CVPixelBufferGetHeight, CVPixelBufferGetPixelFormatType, CVPixelBufferGetWidth,
    CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
    kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_32BGRA,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};

const MAX_SIDE: usize = 640;
const FRAME_INTERVAL: Duration = Duration::from_millis(33);
const LOAD_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Loading,
    Playing,
    Paused,
    Ended,
    Failed,
}

pub struct Playback {
    pub chat: String,
    pub message: String,
    pub path: PathBuf,
    pub state: State,
    pub position: f64,
    pub duration: f64,
    pub muted: bool,
    pub has_audio: bool,
    pub texture: Option<TextureHandle>,
    transform: [f32; 4],
    native: Native,
    paused: bool,
    seeking: bool,
    ready: bool,
    deadline: Instant,
    last_poll: Option<Instant>,
    /// Count of decoded frames uploaded, for offline playback diagnostics.
    pub frames: u64,
}

impl Playback {
    pub fn matches(&self, chat: &str, message: &str) -> bool {
        self.chat == chat && self.message == message
    }

    /// Includes buffering, so the pause button also cancels an initial load.
    pub fn is_playing(&self) -> bool {
        !self.paused && matches!(self.state, State::Loading | State::Playing)
    }

    pub fn paint(&self, painter: &egui::Painter, rect: Rect) {
        let Some(texture) = &self.texture else { return };
        let corners = frame_corners(texture.size(), self.transform, rect);
        let mut mesh = egui::Mesh::with_texture(texture.id());
        for (pos, uv) in corners.into_iter().zip([
            egui::pos2(0.0, 0.0),
            egui::pos2(1.0, 0.0),
            egui::pos2(1.0, 1.0),
            egui::pos2(0.0, 1.0),
        ]) {
            mesh.vertices.push(egui::epaint::Vertex {
                pos,
                uv,
                color: Color32::WHITE,
            });
        }
        mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
        painter.add(mesh);
    }
}

#[derive(Default)]
pub struct Player {
    active: Option<Playback>,
}

impl Player {
    pub fn active(&self) -> Option<&Playback> {
        self.active.as_ref()
    }

    pub fn for_message(&self, chat: &str, message: &str) -> Option<&Playback> {
        self.active
            .as_ref()
            .filter(|active| active.matches(chat, message))
    }

    pub fn stop(&mut self) {
        self.active = None;
    }

    pub fn pause(&mut self) {
        if let Some(active) = &mut self.active {
            // SAFETY: all AVPlayer access stays on the creating main thread.
            unsafe { active.native.player.pause() };
            active.paused = true;
            if active.state == State::Playing {
                active.state = State::Paused;
            }
        }
    }

    pub fn toggle(&mut self, chat: &str, message: &str, path: &Path) -> Result<(), &'static str> {
        if let Some(active) = &mut self.active
            && active.matches(chat, message)
            && active.path == path
            && active.state != State::Failed
        {
            if active.is_playing() {
                self.pause();
            } else {
                if active.state == State::Ended {
                    self.seek(chat, message, 0.0);
                }
                let active = self.active.as_mut().expect("the same playback");
                active.paused = false;
                active.state = if active.ready {
                    State::Playing
                } else {
                    State::Loading
                };
                unsafe { active.native.player.play() };
            }
            return Ok(());
        }
        self.stop();
        let native = Native::new(path)?;
        self.active = Some(Playback {
            chat: chat.to_owned(),
            message: message.to_owned(),
            path: path.to_owned(),
            state: State::Loading,
            position: 0.0,
            duration: 0.0,
            muted: false,
            has_audio: false,
            texture: None,
            transform: [1.0, 0.0, 0.0, 1.0],
            native,
            paused: false,
            seeking: false,
            ready: false,
            deadline: Instant::now() + LOAD_TIMEOUT,
            last_poll: None,
            frames: 0,
        });
        Ok(())
    }

    /// Seeking preserves pause state and rejects stale controls from another row.
    pub fn seek(&mut self, chat: &str, message: &str, fraction: f64) {
        let Some(active) = self.active.as_mut().filter(|p| p.matches(chat, message)) else {
            return;
        };
        let Some(position) = seek_position(active.duration, fraction) else {
            return;
        };
        let time = seconds_to_time(position);
        let tolerance = seconds_to_time(0.02);
        unsafe {
            active
                .native
                .player
                .seekToTime_toleranceBefore_toleranceAfter(time, tolerance, tolerance)
        };
        active.position = position;
        active.seeking = true;
        active.deadline = Instant::now() + LOAD_TIMEOUT;
        active.state = if active.paused {
            State::Paused
        } else {
            State::Playing
        };
        if !active.paused {
            unsafe { active.native.player.play() };
        }
    }

    pub fn toggle_mute(&mut self, chat: &str, message: &str) {
        if let Some(active) = self.active.as_mut().filter(|p| p.matches(chat, message)) {
            active.muted = !active.muted;
            unsafe { active.native.player.setMuted(active.muted) };
        }
    }

    /// AVFoundation decodes asynchronously. Poll only while loading, playing or
    /// producing a sought frame; a settled paused player schedules no repaints.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let Some(active) = &mut self.active else {
            return;
        };
        if matches!(active.state, State::Ended | State::Failed) {
            return;
        }
        let now = Instant::now();
        if let Some(last) = active.last_poll
            && now.duration_since(last) < FRAME_INTERVAL
        {
            if active.is_playing() || active.seeking || active.texture.is_none() {
                ctx.request_repaint_after(FRAME_INTERVAL - now.duration_since(last));
            }
            return;
        }
        active.last_poll = Some(now);
        // SAFETY: the retained native objects are used only on their main thread;
        // currentTime/status are nonblocking, and output retains each acquired buffer.
        unsafe {
            if active.native.item.status() == AVPlayerItemStatus::Failed
                || active.native.player.status() == AVPlayerStatus::Failed
                || ((active.texture.is_none() || active.seeking) && now >= active.deadline)
            {
                active.native.player.pause();
                active.state = State::Failed;
                active.paused = true;
                return;
            }
            if active.native.item.status() == AVPlayerItemStatus::ReadyToPlay {
                if !active.ready {
                    (active.transform, active.has_audio) = active.native.geometry_and_audio();
                    active.ready = true;
                }
                active.duration = time_seconds(active.native.item.duration()).unwrap_or(0.0);
                let time = active.native.player.currentTime();
                active.position = time_seconds(time).unwrap_or(0.0);
                if active.native.output.hasNewPixelBufferForItemTime(time)
                    && let Some(buffer) = active
                        .native
                        .output
                        .copyPixelBufferForItemTime_itemTimeForDisplay(time, std::ptr::null_mut())
                    && let Some(image) = pixels(&buffer)
                {
                    if let Some(texture) = &mut active.texture {
                        texture.set(image, TextureOptions::LINEAR);
                    } else {
                        active.texture =
                            Some(ctx.load_texture("inline-video", image, TextureOptions::LINEAR));
                    }
                    active.frames += 1;
                    active.seeking = false;
                }
                active.state = if active.duration > 0.0
                    && active.position >= active.duration - 0.02
                    && active.native.player.rate() == 0.0
                    && !active.seeking
                {
                    active.paused = true;
                    State::Ended
                } else if active.texture.is_none() {
                    State::Loading
                } else if active.paused {
                    State::Paused
                } else {
                    State::Playing
                };
            }
        }
        if active.is_playing() || active.seeking || active.texture.is_none() {
            ctx.request_repaint_after(FRAME_INTERVAL);
        }
    }
}

struct Native {
    player: Retained<AVPlayer>,
    item: Retained<AVPlayerItem>,
    output: Retained<AVPlayerItemVideoOutput>,
}

impl Native {
    fn new(path: &Path) -> Result<Self, &'static str> {
        if !path.is_file() {
            return Err("This video is no longer available. Download it again.");
        }
        let main =
            MainThreadMarker::new().ok_or("Video playback requires the application window.")?;
        let file =
            CString::new(path.as_os_str().as_bytes()).map_err(|_| "Cannot open this video.")?;
        // SAFETY: a local, validated file URL; NSDictionary contains the documented
        // pixel-format key and NSNumber. CFString/NSString are toll-free bridged.
        unsafe {
            let url = NSURL::fileURLWithFileSystemRepresentation_isDirectory_relativeToURL(
                std::ptr::NonNull::new(file.as_ptr().cast_mut()).expect("CString is nonnull"),
                false,
                None,
            );
            let item = AVPlayerItem::initWithURL(AVPlayerItem::alloc(main), &url);
            let key = &*(kCVPixelBufferPixelFormatTypeKey as *const _ as *const NSString);
            let format = NSNumber::new_u32(kCVPixelFormatType_32BGRA);
            let attributes = NSDictionary::<NSString, AnyObject>::from_slices(&[key], &[&format]);
            let output = AVPlayerItemVideoOutput::initWithPixelBufferAttributes(
                AVPlayerItemVideoOutput::alloc(),
                Some(&attributes),
            );
            // Consume video ourselves while AVPlayer keeps rendering synchronized audio.
            output.setSuppressesPlayerRendering(true);
            item.addOutput(&output);
            let player = AVPlayer::playerWithPlayerItem(Some(&item), main);
            player.play();
            Ok(Self {
                player,
                item,
                output,
            })
        }
    }

    // macOS 11 remains supported. Its synchronous track getters are safe here:
    // AVPlayerItem is already ready and has loaded the track metadata for us.
    fn geometry_and_audio(&self) -> ([f32; 4], bool) {
        let mut transform = [1.0, 0.0, 0.0, 1.0];
        let mut audio = false;
        unsafe {
            for item_track in self.item.tracks() {
                let Some(track) = item_track.assetTrack() else {
                    continue;
                };
                if &*track.mediaType() == AVMediaTypeVideo.unwrap() {
                    let t = track.preferredTransform();
                    transform = [t.a as f32, t.b as f32, t.c as f32, t.d as f32];
                } else if &*track.mediaType() == AVMediaTypeAudio.unwrap() && item_track.isEnabled()
                {
                    audio = true;
                }
            }
        }
        (transform, audio)
    }
}

impl Drop for Native {
    fn drop(&mut self) {
        unsafe {
            self.player.pause();
            self.item.removeOutput(&self.output);
            self.player.replaceCurrentItemWithPlayerItem(None);
        }
    }
}

fn seek_position(duration: f64, fraction: f64) -> Option<f64> {
    (duration.is_finite() && duration > 0.0 && fraction.is_finite())
        .then(|| duration * fraction.clamp(0.0, 1.0))
}

fn seconds_to_time(seconds: f64) -> CMTime {
    CMTime {
        value: (seconds * 600.0).round() as i64,
        timescale: 600,
        flags: CMTimeFlags::Valid,
        epoch: 0,
    }
}

fn time_seconds(time: CMTime) -> Option<f64> {
    (time.flags.contains(CMTimeFlags::Valid)
        && !time.flags.intersects(
            CMTimeFlags::Indefinite | CMTimeFlags::PositiveInfinity | CMTimeFlags::NegativeInfinity,
        )
        && time.timescale > 0)
        .then(|| (time.value as f64 / f64::from(time.timescale)).max(0.0))
}

/// Preserve rotation/mirroring without allocating a second, rotated frame.
fn frame_corners(size: [usize; 2], transform: [f32; 4], rect: Rect) -> [Pos2; 4] {
    let [a, b, c, d] = transform;
    let [w, h] = size.map(|side| side as f32);
    let mut points = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]
        .map(|(x, y)| egui::pos2(a * x + c * y, b * x + d * y));
    let bounds = Rect::from_points(&points);
    if !bounds.is_finite() || bounds.width() <= 0.0 || bounds.height() <= 0.0 {
        return [
            rect.left_top(),
            rect.right_top(),
            rect.right_bottom(),
            rect.left_bottom(),
        ];
    }
    let scale = (rect.width() / bounds.width()).min(rect.height() / bounds.height());
    for point in &mut points {
        *point = rect.center() + (*point - bounds.center()) * scale;
    }
    points
}

struct LockedPixels<'a>(&'a CVPixelBuffer);
impl Drop for LockedPixels<'_> {
    fn drop(&mut self) {
        unsafe { CVPixelBufferUnlockBaseAddress(self.0, CVPixelBufferLockFlags::ReadOnly) };
    }
}

fn pixels(buffer: &CVPixelBuffer) -> Option<ColorImage> {
    let width = CVPixelBufferGetWidth(buffer);
    let height = CVPixelBufferGetHeight(buffer);
    let stride = CVPixelBufferGetBytesPerRow(buffer);
    if CVPixelBufferGetPixelFormatType(buffer) != kCVPixelFormatType_32BGRA
        || width == 0
        || height == 0
        || stride < width.checked_mul(4)?
        || unsafe { CVPixelBufferLockBaseAddress(buffer, CVPixelBufferLockFlags::ReadOnly) } != 0
    {
        return None;
    }
    let _lock = LockedPixels(buffer);
    let address = std::ptr::NonNull::new(CVPixelBufferGetBaseAddress(buffer))?;
    // SAFETY: the retained CVPixelBuffer is locked read-only and the checked
    // dimensions/stride describe its BGRA storage, including row padding.
    let bytes = unsafe {
        std::slice::from_raw_parts(address.as_ptr().cast::<u8>(), stride.checked_mul(height)?)
    };
    let scale = MAX_SIDE as f64 / width.max(height).max(MAX_SIDE) as f64;
    let w = ((width as f64 * scale).round() as usize).max(1);
    let h = ((height as f64 * scale).round() as usize).max(1);
    let mut colors = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let at = (y * height / h) * stride + (x * width / w) * 4;
            colors.push(Color32::from_rgb(bytes[at + 2], bytes[at + 1], bytes[at]));
        }
    }
    Some(ColorImage::new([w, h], colors))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portrait_rotation_and_mirroring_keep_the_frame_inside_its_bubble() {
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(180.0, 320.0));
        assert_eq!(
            frame_corners([640, 360], [0.0, 1.0, -1.0, 0.0], rect),
            [
                rect.right_top(),
                rect.right_bottom(),
                rect.left_bottom(),
                rect.left_top()
            ]
        );
        let wide = Rect::from_min_size(Pos2::ZERO, egui::vec2(320.0, 180.0));
        assert_eq!(
            frame_corners([640, 360], [-1.0, 0.0, 0.0, 1.0], wide),
            [
                wide.right_top(),
                wide.left_top(),
                wide.left_bottom(),
                wide.right_bottom()
            ]
        );
    }

    #[test]
    fn timeline_rejects_unknown_duration_and_non_finite_seek_input() {
        assert_eq!(seek_position(0.0, 0.5), None);
        assert_eq!(seek_position(f64::INFINITY, 0.5), None);
        assert_eq!(seek_position(20.0, f64::NAN), None);
        assert_eq!(seek_position(20.0, -1.0), Some(0.0));
        assert_eq!(seek_position(20.0, 1.5), Some(20.0));
        assert_eq!(time_seconds(seconds_to_time(3.5)), Some(3.5));
        let mut unknown = seconds_to_time(1.0);
        unknown.flags |= CMTimeFlags::Indefinite;
        assert_eq!(time_seconds(unknown), None);
    }
}
