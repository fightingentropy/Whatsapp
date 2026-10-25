//! Animated GIFs and WebPs as the looping H.264 MP4 WhatsApp sends as a GIF.
use std::ffi::{CString, c_char};
use std::path::Path;

/// Largest animation read from a file, the clipboard or a download.
pub const MAX_BYTES: usize = 24 * 1024 * 1024;

unsafe extern "C" {
    fn whatsapp_animation_video(bytes: *const u8, length: usize, output: *const c_char) -> bool;
}

/// Whether the bytes are a GIF or WebP container, either of which may animate.
pub fn is_animation_container(bytes: &[u8]) -> bool {
    bytes.starts_with(b"GIF87a")
        || bytes.starts_with(b"GIF89a")
        || (bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP")
}

/// Encodes an animation to `output`, which must not exist yet. Returns false,
/// leaving nothing behind, for a still image or anything that cannot be
/// encoded, so the caller keeps its own fallback. Runs on a bounded upload
/// worker, never the UI thread or the FFI control queue.
pub fn encode(bytes: &[u8], output: &Path) -> bool {
    if bytes.len() > MAX_BYTES || !is_animation_container(bytes) {
        return false;
    }
    let Ok(path) = CString::new(output.as_os_str().as_encoded_bytes()) else {
        return false;
    };
    // SAFETY: both pointers stay alive for the call. Native code reads the
    // bytes without retaining them and catches its Objective-C exceptions.
    let encoded = unsafe { whatsapp_animation_video(bytes.as_ptr(), bytes.len(), path.as_ptr()) };
    if !encoded {
        let _ = std::fs::remove_file(output);
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame, Rgba, RgbaImage};

    /// A synthetic GIF whose frames alternate between two colours.
    fn sample_gif(frames: usize, delay_ms: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = GifEncoder::new(&mut bytes);
            encoder.set_repeat(Repeat::Infinite).unwrap();
            for index in 0..frames {
                let colour = if index % 2 == 0 {
                    Rgba([220, 40, 40, 255])
                } else {
                    Rgba([40, 40, 220, 255])
                };
                let image = RgbaImage::from_pixel(99, 61, colour);
                let delay = Delay::from_numer_denom_ms(delay_ms, 1);
                encoder
                    .encode_frame(Frame::from_parts(image, 0, 0, delay))
                    .unwrap();
            }
        }
        bytes
    }

    #[test]
    fn an_animated_gif_becomes_an_even_sized_looping_video_of_the_same_length() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("clip.mp4");
        assert!(encode(&sample_gif(6, 250), &output));
        let video = crate::video_metadata::read(&output).expect("AVFoundation reads it back");
        assert_eq!((video.width, video.height), (98, 60));
        // Six 250 ms frames, the last one included.
        assert_eq!(video.seconds, 2);
        assert!(video.thumbnail.is_some());
        let header = std::fs::read(&output).unwrap();
        assert_eq!(&header[4..8], b"ftyp");
        let moov = header.windows(4).position(|w| w == b"moov").unwrap();
        let mdat = header.windows(4).position(|w| w == b"mdat").unwrap();
        assert!(moov < mdat, "optimized for streaming");
    }

    #[test]
    fn stills_and_other_files_are_left_to_their_usual_path() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("still.mp4");
        assert!(!encode(&sample_gif(1, 100), &output));
        assert!(!output.exists());
        let mut png = Vec::new();
        RgbaImage::from_pixel(32, 32, Rgba([1, 2, 3, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        assert!(!encode(&png, &output));
        assert!(!encode(b"GIF89a broken", &output));
        assert!(!output.exists());
    }

    #[test]
    fn containers_are_recognised_by_their_signature() {
        assert!(is_animation_container(b"GIF89a...."));
        assert!(is_animation_container(b"GIF87a...."));
        assert!(is_animation_container(b"RIFF\0\0\0\0WEBPVP8X"));
        assert!(!is_animation_container(b"RIFF\0\0\0\0WAVEfmt "));
        assert!(!is_animation_container(b"\x89PNG\r\n\x1a\n"));
    }
}
