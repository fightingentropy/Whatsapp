//! Bounded AVFoundation posters for local videos, off the UI thread.
use std::ffi::{CString, c_char, c_void};
use std::path::Path;

#[derive(Debug)]
pub struct VideoMetadata {
    pub seconds: u32,
    pub width: u32,
    pub height: u32,
    pub thumbnail: Option<Vec<u8>>,
}

unsafe extern "C" {
    fn whatsapp_video_poster(
        path: *const c_char,
        seconds: *mut f64,
        width: *mut u32,
        height: *mut u32,
        jpeg: *mut *mut c_void,
        length: *mut usize,
    ) -> bool;
    fn whatsapp_native_free(bytes: *mut c_void);
    #[cfg(target_os = "macos")]
    fn whatsapp_video_preview(
        path: *const c_char,
        seconds: *mut f64,
        width: *mut u32,
        height: *mut u32,
        jpeg: *mut *mut c_void,
        length: *mut usize,
    ) -> bool;
}

/// Called from a bounded upload worker, never from the main/FFI control queue.
pub fn read(path: &Path) -> Option<VideoMetadata> {
    read_native(path, false)
}

/// Higher quality, local-only desktop poster; called by the serial preview worker.
#[cfg(target_os = "macos")]
pub fn preview(path: &Path) -> Option<Vec<u8>> {
    read_native(path, true)?.thumbnail
}

fn read_native(path: &Path, preview: bool) -> Option<VideoMetadata> {
    let native = whatsapp_video_poster;
    #[cfg(target_os = "macos")]
    let native = if preview {
        whatsapp_video_preview
    } else {
        native
    };
    let byte_limit = if preview { 512 * 1024 } else { 128 * 1024 };
    let path = CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    let (mut seconds, mut width, mut height) = (0.0, 0, 0);
    let (mut jpeg, mut length) = (std::ptr::null_mut(), 0);
    // SAFETY: C receives live output pointers; it returns a malloc-owned buffer
    // bounded by byte_limit. All native completions finish before copying output
    // or own their captures without retaining any Rust address on timeout.
    let success = unsafe {
        native(
            path.as_ptr(),
            &mut seconds,
            &mut width,
            &mut height,
            &mut jpeg,
            &mut length,
        )
    };
    let thumbnail = if jpeg.is_null() {
        None
    } else {
        let bytes = (success && length > 0 && length <= byte_limit)
            .then(|| unsafe { std::slice::from_raw_parts(jpeg.cast::<u8>(), length).to_vec() });
        unsafe { whatsapp_native_free(jpeg) };
        bytes
    };
    (success && seconds.is_finite() && seconds > 0.0 && width > 0 && height > 0).then_some(
        VideoMetadata {
            seconds: seconds.ceil().min(f64::from(u32::MAX)) as u32,
            width,
            height,
            thumbnail,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synthetic_video_has_bounded_decodable_poster_and_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.mp4");
        std::fs::write(
            &path,
            include_bytes!("../ios/Whatsapp/Fixtures/inline-video.mp4"),
        )
        .unwrap();
        let meta = read(&path).expect("synthetic video");
        assert!(meta.width > 0 && meta.height > 0 && meta.seconds > 0);
        let image = image::load_from_memory(meta.thumbnail.as_deref().expect("poster")).unwrap();
        assert!(image.width() <= 96 && image.height() <= 96);
        assert!(
            image
                .to_rgb8()
                .pixels()
                .any(|pixel| pixel.0.iter().any(|v| *v > 30))
        );
        std::fs::write(&path, b"invalid media").unwrap();
        assert!(read(&path).is_none());
        assert!(read(&dir.path().join("missing.mp4")).is_none());
    }
}
