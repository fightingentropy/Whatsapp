//! Hardware H.264 preview decoding. All CF objects and callback state belong to
//! one decoder job; teardown waits for callbacks before freeing their storage.

use std::{
    ffi::c_void,
    path::Path,
    ptr::{self, NonNull},
    sync::Mutex,
    time::Duration,
};

use objc2_core_foundation::{CFBoolean, CFDictionary, CFNumber, CFRetained};
use objc2_core_media::{
    CMBlockBuffer, CMFormatDescription, CMSampleBuffer, CMSampleTimingInfo, CMTime, CMTimeFlags,
    CMVideoFormatDescriptionCreateFromH264ParameterSets,
};
use objc2_core_video::{
    CVImageBuffer, CVPixelBuffer, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow,
    CVPixelBufferGetHeight, CVPixelBufferGetPixelFormatType, CVPixelBufferGetWidth,
    CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
    kCVPixelBufferHeightKey, kCVPixelBufferPixelFormatTypeKey, kCVPixelBufferWidthKey,
    kCVPixelFormatType_32BGRA,
};
use objc2_video_toolbox::{
    VTDecodeFrameFlags, VTDecodeInfoFlags, VTDecompressionOutputCallbackRecord,
    VTDecompressionSession, kVTVideoDecoderSpecification_RequireHardwareAcceleratedVideoDecoder,
};

#[cfg(any(test, feature = "demo"))]
use super::{Decoded, collect};
use super::{MAX_FRAMES, Sink, Update, to_color_image};

#[derive(Default)]
struct Output {
    frames: Vec<(i64, egui::ColorImage, Duration)>,
    failed: bool,
}

struct Session {
    inner: CFRetained<VTDecompressionSession>,
    output: Box<Mutex<Output>>,
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: the session is live and output is still allocated. Invalidation
        // ends its callbacks before fields (including the output box) are dropped.
        unsafe {
            self.inner.wait_for_asynchronous_frames();
            self.inner.invalidate();
        }
    }
}

/// Returns None on any native decode failure, letting the caller retry software.
pub(super) fn stream(path: &Path, emit: &mut Sink<'_>) -> Option<()> {
    decode_impl(path, true, emit)
}

#[cfg(test)]
pub(super) fn decode(path: &Path) -> Option<Decoded> {
    collect(|emit| stream(path, emit))
}

/// Benchmarks must exercise hardware even when the normal size policy avoids it.
#[cfg(feature = "demo")]
pub(super) fn decode_for_probe(path: &Path) -> Option<Decoded> {
    collect(|emit| decode_impl(path, false, emit))
}

fn hardware_worthwhile(width: u16, height: u16, samples: u32) -> bool {
    // Conservative policy from the local comparison: tiny previews lose time
    // setting up a session. Keep those, and very short clips, on OpenH264.
    u32::from(width) * u32::from(height) >= 640 * 360 && samples >= 8
}

fn decode_impl(path: &Path, apply_size_policy: bool, emit: &mut Sink<'_>) -> Option<()> {
    let file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    let mut mp4 = mp4::Mp4Reader::read_header(std::io::BufReader::new(file), size).ok()?;
    let track = mp4
        .tracks()
        .values()
        .find(|track| track.track_type().ok() == Some(mp4::TrackType::Video))?;
    let avcc = &track.trak.mdia.minf.stbl.stsd.avc1.as_ref()?.avcc;
    if apply_size_policy
        && !hardware_worthwhile(track.width(), track.height(), track.sample_count())
    {
        return None;
    }
    let nal_length = (avcc.length_size_minus_one & 3) + 1;
    if !matches!(nal_length, 1 | 2 | 4) {
        return None;
    }
    let sps = track.sequence_parameter_set().ok()?;
    let pps = track.picture_parameter_set().ok()?;
    if sps.is_empty() || pps.is_empty() {
        return None;
    }
    let track_id = track.track_id();
    let timescale = i32::try_from(track.timescale()).ok().filter(|t| *t > 0)?;
    let count = track.sample_count().min(MAX_FRAMES as u32);
    let largest = u32::from(track.width().max(track.height())).max(super::MAX_WIDTH);
    let width = (u32::from(track.width()) * super::MAX_WIDTH / largest).max(1);
    let height = (u32::from(track.height()) * super::MAX_WIDTH / largest).max(1);

    // SAFETY: the two arrays and their nonempty byte slices live through the
    // call. CoreMedia copies the parameter sets into the retained description.
    let format = unsafe {
        let mut pointers = [
            NonNull::new(sps.as_ptr().cast_mut())?,
            NonNull::new(pps.as_ptr().cast_mut())?,
        ];
        let mut lengths = [sps.len(), pps.len()];
        let mut raw = ptr::null();
        let status = CMVideoFormatDescriptionCreateFromH264ParameterSets(
            None,
            2,
            NonNull::from(&mut pointers[0]),
            NonNull::from(&mut lengths[0]),
            i32::from(nal_length),
            NonNull::from(&mut raw),
        );
        if status != 0 {
            return None;
        }
        CFRetained::from_raw(NonNull::new(raw.cast_mut())?)
    };
    let output = Box::new(Mutex::new(Output::default()));
    let callback = VTDecompressionOutputCallbackRecord {
        decompressionOutputCallback: Some(deliver),
        decompressionOutputRefCon: (&*output as *const Mutex<Output>).cast_mut().cast(),
    };
    // SAFETY: documented framework keys have the correct CFBoolean/CFNumber
    // values. RequireHardware ensures a successful session really uses hardware.
    // Callback state is at a stable boxed address until Session::drop completes.
    let session = unsafe {
        let specification = CFDictionary::from_slices(
            &[kVTVideoDecoderSpecification_RequireHardwareAcceleratedVideoDecoder],
            &[CFBoolean::new(true)],
        );
        let pixel_format = CFNumber::new_i32(kCVPixelFormatType_32BGRA as i32);
        let width = CFNumber::new_i32(width as i32);
        let height = CFNumber::new_i32(height as i32);
        let attributes = CFDictionary::from_slices(
            &[
                kCVPixelBufferPixelFormatTypeKey,
                kCVPixelBufferWidthKey,
                kCVPixelBufferHeightKey,
            ],
            &[&*pixel_format, &*width, &*height],
        );
        let mut raw = ptr::null_mut();
        let status = VTDecompressionSession::create(
            None,
            &format,
            Some(specification.as_opaque()),
            Some(attributes.as_opaque()),
            &callback,
            NonNull::from(&mut raw),
        );
        if status != 0 {
            return None;
        }
        Session {
            inner: CFRetained::from_raw(NonNull::new(raw)?),
            output,
        }
    };

    // Know presentation order before publishing. B frames can arrive after a
    // later P frame; sorting each four-frame callback batch is insufficient.
    // Keep only timestamps, never a second copy of the compressed clip.
    let mut expected = Vec::with_capacity(count as usize);
    for sample_id in 1..=count {
        let sample = mp4.read_sample(track_id, sample_id).ok()??;
        expected.push(
            i64::try_from(sample.start_time)
                .ok()?
                .checked_add(i64::from(sample.rendering_offset))?,
        );
    }
    expected.sort_unstable();
    let mut expected = std::collections::VecDeque::from(expected);
    let mut pending = Vec::new();
    for sample_id in 1..=count {
        let sample = mp4.read_sample(track_id, sample_id).ok()??;
        if !valid_sample(&sample.bytes, nal_length) {
            return None;
        }
        let dts = i64::try_from(sample.start_time).ok()?;
        let pts = dts.checked_add(i64::from(sample.rendering_offset))?;
        let timing = CMSampleTimingInfo {
            duration: time(i64::from(sample.duration), timescale),
            presentationTimeStamp: time(pts, timescale),
            decodeTimeStamp: time(dts, timescale),
        };
        let buffer = sample_buffer(&format, &sample.bytes, &timing)?;
        // SAFETY: the sample owns its copied compressed bytes, and CoreMedia
        // retains them if decoding outlives this call. The session owns callback state.
        let status = unsafe {
            session.inner.decode_frame(
                &buffer,
                VTDecodeFrameFlags::Frame_EnableTemporalProcessing
                    | VTDecodeFrameFlags::Frame_EnableAsynchronousDecompression,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        if status != 0 {
            return None;
        }
        // Bound work in flight while allowing the hardware pipeline to overlap
        // a few frames. Native decoder buffers are outside the app's CPU cache.
        if sample_id % 4 == 0 {
            if unsafe { session.inner.wait_for_asynchronous_frames() } != 0 {
                return None;
            }
            drain(&session, &mut pending, &mut expected, emit)?;
        }
    }
    // SAFETY: session and callback storage remain live while delayed B frames
    // are flushed. The wait establishes that output can now be taken.
    unsafe {
        if session.inner.finish_delayed_frames() != 0
            || session.inner.wait_for_asynchronous_frames() != 0
        {
            return None;
        }
    }
    drain(&session, &mut pending, &mut expected, emit)?;
    (count > 0 && expected.is_empty() && pending.is_empty()).then_some(())
}

/// Bound the native reorder buffer too. H.264 allows at most 16 reference
/// frames, plus a four-frame submission batch. Unusual/malformed timing falls
/// back to software instead of buffering an entire clip behind a missing frame.
fn drain(
    session: &Session,
    pending: &mut Vec<(i64, egui::ColorImage, Duration)>,
    expected: &mut std::collections::VecDeque<i64>,
    emit: &mut Sink<'_>,
) -> Option<()> {
    {
        let mut output = session.output.lock().unwrap_or_else(|p| p.into_inner());
        if output.failed {
            return None;
        }
        pending.append(&mut output.frames);
    }
    pending.sort_by_key(|(pts, _, _)| *pts);
    while pending.first().map(|frame| frame.0) == expected.front().copied() && !pending.is_empty() {
        let (_, image, delay) = pending.remove(0);
        expected.pop_front();
        // Backpressure happens on the worker, never in a VideoToolbox callback.
        emit(Update::Frame(image, delay))?;
    }
    (pending.len() <= 24).then_some(())
}

fn time(value: i64, timescale: i32) -> CMTime {
    CMTime {
        value,
        timescale,
        flags: CMTimeFlags::Valid,
        epoch: 0,
    }
}

fn sample_buffer(
    format: &CMFormatDescription,
    bytes: &[u8],
    timing: &CMSampleTimingInfo,
) -> Option<CFRetained<CMSampleBuffer>> {
    // SAFETY: passing null asks CoreMedia to allocate/own the compressed storage.
    // The bytes are copied before creating a sample; no Rust slice escapes.
    unsafe {
        let mut raw = ptr::null_mut();
        if CMBlockBuffer::create_with_memory_block(
            None,
            ptr::null_mut(),
            bytes.len(),
            None,
            ptr::null(),
            0,
            bytes.len(),
            0,
            NonNull::from(&mut raw),
        ) != 0
        {
            return None;
        }
        let block = CFRetained::from_raw(NonNull::new(raw)?);
        if CMBlockBuffer::replace_data_bytes(
            NonNull::new(bytes.as_ptr().cast_mut().cast())?,
            &block,
            0,
            bytes.len(),
        ) != 0
        {
            return None;
        }
        let mut raw_sample = ptr::null_mut();
        let length = bytes.len();
        if CMSampleBuffer::create_ready(
            None,
            Some(&block),
            Some(format),
            1,
            1,
            timing,
            1,
            &length,
            NonNull::from(&mut raw_sample),
        ) != 0
        {
            return None;
        }
        Some(CFRetained::from_raw(NonNull::new(raw_sample)?))
    }
}

/// Reject truncated AVCC input before handing a sample to the framework.
fn valid_sample(mut bytes: &[u8], length_size: u8) -> bool {
    if !matches!(length_size, 1 | 2 | 4) || bytes.is_empty() {
        return false;
    }
    let prefix = usize::from(length_size);
    while !bytes.is_empty() {
        if bytes.len() < prefix {
            return false;
        }
        let size = bytes[..prefix]
            .iter()
            .fold(0usize, |n, b| (n << 8) | usize::from(*b));
        bytes = &bytes[prefix..];
        if size == 0 || size > bytes.len() {
            return false;
        }
        bytes = &bytes[size..];
    }
    true
}

unsafe extern "C-unwind" fn deliver(
    context: *mut c_void,
    _: *mut c_void,
    status: i32,
    _: VTDecodeInfoFlags,
    image: *mut CVImageBuffer,
    pts: CMTime,
    duration: CMTime,
) {
    // SAFETY: context points to the Session-owned box; VideoToolbox may call from
    // another thread, so all access is serialized with the mutex.
    let output = unsafe { &*context.cast::<Mutex<Output>>() };
    let mut output = output.lock().unwrap_or_else(|p| p.into_inner());
    if status != 0 || image.is_null() {
        output.failed = true;
        return;
    }
    if output.frames.len() >= MAX_FRAMES {
        return;
    }
    let converted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: a non-null callback image stays valid for the callback duration.
        pixels(unsafe { &*image })
    }));
    match converted {
        Ok(Some(image)) => {
            let millis = if duration.timescale > 0 && duration.value > 0 {
                (i128::from(duration.value) * 1000 / i128::from(duration.timescale)).clamp(20, 1000)
                    as u64
            } else {
                66
            };
            output
                .frames
                .push((pts.value, image, Duration::from_millis(millis)));
        }
        _ => output.failed = true,
    }
}

struct LockedPixels<'a>(&'a CVPixelBuffer);
impl Drop for LockedPixels<'_> {
    fn drop(&mut self) {
        // SAFETY: this guard exists only after a successful read-only lock.
        unsafe {
            CVPixelBufferUnlockBaseAddress(self.0, CVPixelBufferLockFlags::ReadOnly);
        }
    }
}

fn pixels(buffer: &CVPixelBuffer) -> Option<egui::ColorImage> {
    if CVPixelBufferGetPixelFormatType(buffer) != kCVPixelFormatType_32BGRA {
        return None;
    }
    let width = CVPixelBufferGetWidth(buffer);
    let height = CVPixelBufferGetHeight(buffer);
    let stride = CVPixelBufferGetBytesPerRow(buffer);
    let row = width.checked_mul(4)?;
    if width == 0 || height == 0 || stride < row || width.checked_mul(height)? > 16_777_216 {
        return None;
    }
    // SAFETY: the callback holds the pixel buffer alive; reads occur only during
    // a successful lock. Rows include platform padding, which we do not copy.
    if unsafe { CVPixelBufferLockBaseAddress(buffer, CVPixelBufferLockFlags::ReadOnly) } != 0 {
        return None;
    }
    let _lock = LockedPixels(buffer);
    let address = NonNull::new(CVPixelBufferGetBaseAddress(buffer))?;
    let bytes = unsafe {
        std::slice::from_raw_parts(address.as_ptr().cast::<u8>(), stride.checked_mul(height)?)
    };
    let mut rgba = Vec::with_capacity(row.checked_mul(height)?);
    for scanline in bytes.chunks_exact(stride).take(height) {
        for bgra in scanline[..row].as_chunks::<4>().0 {
            rgba.extend_from_slice(&[bgra[2], bgra[1], bgra[0], bgra[3]]);
        }
    }
    Some(to_color_image(&image::RgbaImage::from_raw(
        width as u32,
        height as u32,
        rgba,
    )?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiny_and_short_previews_avoid_hardware_setup_in_either_orientation() {
        assert!(!hardware_worthwhile(320, 180, 60));
        assert!(!hardware_worthwhile(1920, 1080, 1));
        assert!(hardware_worthwhile(640, 360, 60));
        assert!(hardware_worthwhile(360, 640, 60));
    }

    #[test]
    fn validates_all_supported_avcc_prefix_widths_and_rejects_truncation() {
        for n in [1, 2, 4] {
            let mut sample = vec![0; n];
            sample[n - 1] = 2;
            sample.extend_from_slice(&[0x65, 0]);
            assert!(valid_sample(&sample, n as u8));
            sample.pop();
            assert!(!valid_sample(&sample, n as u8));
        }
        assert!(!valid_sample(&[], 4));
        assert!(!valid_sample(&[0, 0, 0, 0], 4));
        assert!(!valid_sample(&[0, 0, 1, 0x65], 3));
        assert!(!valid_sample(&[1, 0x65, 2, 0x65], 1));
    }

    #[test]
    fn preview_keeps_b_frame_order_colours_and_tail_with_software_fallback() {
        for (name, size) in [
            ("h264-bframes.mp4", [96, 64]),
            ("h264-preview.mp4", [320, 180]),
        ] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(name);
            // Exercise automatic hardware selection and the fallback separately.
            // This must also pass on CI hosts without hardware decode access.
            for decoded in [
                collect(|emit| super::super::decode_video(&path, emit)),
                super::super::decode_mp4(&path),
            ] {
                let decoded = decoded.expect("H.264 preview");
                assert_eq!(decoded.frames.len(), 12);
                for (index, (image, duration)) in decoded.frames.iter().enumerate() {
                    assert_eq!(image.size, size);
                    assert_eq!(*duration, Duration::from_millis(100));
                    let rgb = image.pixels[size[0] * (size[1] / 2) + size[0] / 2].to_array();
                    let channel = index / 4;
                    assert!(rgb[channel] > 230, "frame {index}: {rgb:?}");
                    assert!(rgb[(channel + 1) % 3] < 20, "frame {index}: {rgb:?}");
                    assert!(rgb[(channel + 2) % 3] < 20, "frame {index}: {rgb:?}");
                }
            }
        }
        assert!(decode(Path::new("does-not-exist.mp4")).is_none());
    }
}
