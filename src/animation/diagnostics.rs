//! Offline decoder comparison. Callers supply synthetic fixtures, never account media.

use super::{Decoded, decode_mp4, videotoolbox};
use std::{path::Path, time::Instant};

/// Time to the first ordered CPU frame versus the full clip. The sink consumes
/// immediately; this isolates decoder startup, not window/GPU presentation.
pub fn streaming(path: &Path, runs: usize) -> anyhow::Result<serde_json::Value> {
    anyhow::ensure!(runs > 0, "at least one run is required");
    let mut samples = Vec::new();
    for run in 0..=runs {
        let started = Instant::now();
        let mut first_ms = None;
        let mut frames = 0usize;
        let mut bytes = 0usize;
        let mut resets = 0usize;
        let ok = super::decode_stream(path, &mut |update| {
            match update {
                super::Update::Frame(image, _) => {
                    first_ms.get_or_insert_with(|| started.elapsed().as_secs_f64() * 1000.0);
                    frames += 1;
                    bytes += image.pixels.len() * 4;
                }
                super::Update::Reset => {
                    first_ms = None;
                    frames = 0;
                    bytes = 0;
                    resets += 1;
                }
                super::Update::Complete(_) => unreachable!(),
            }
            Some(())
        });
        anyhow::ensure!(ok.is_some() && frames > 0, "stream decode failed");
        if run > 0 {
            samples.push(serde_json::json!({
                "first_ordered_cpu_frame_ms": first_ms,
                "complete_ms": started.elapsed().as_secs_f64() * 1000.0,
                "frames": frames, "retained_loop_pixel_bytes": bytes,
                "fallback_resets": resets,
            }));
        }
    }
    Ok(serde_json::json!({
        "runs": samples,
        "queued_frames_per_decoder": super::QUEUED_FRAMES,
        "max_queued_pixel_bytes_per_decoder": super::QUEUED_FRAMES * super::MAX_WIDTH as usize * super::MAX_WIDTH as usize * 4,
        "note": "ordered decoder output, immediate consumer; excludes UI scheduling and GPU upload",
    }))
}

/// Decodes the same file with both implementations, alternating order. Fails if
/// hardware is unavailable; the probe must not silently benchmark software twice.
pub fn compare(path: &Path, runs: usize) -> anyhow::Result<serde_json::Value> {
    anyhow::ensure!(runs > 0, "at least one run is required");
    let mut hardware_ms = Vec::new();
    let mut software_ms = Vec::new();
    let mut result = serde_json::Value::Null;
    for run in 0..=runs {
        let mut decode = |hardware: bool| -> anyhow::Result<Decoded> {
            let start = Instant::now();
            let decoded = if hardware {
                videotoolbox::decode_for_probe(path)
            } else {
                decode_mp4(path)
            }
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "{} decoder unavailable or failed",
                    if hardware { "hardware" } else { "software" }
                )
            })?;
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            if run > 0 {
                if hardware {
                    hardware_ms.push(elapsed);
                } else {
                    software_ms.push(elapsed);
                }
            }
            Ok(decoded)
        };
        let (hardware, software) = if run % 2 == 0 {
            (decode(true)?, decode(false)?)
        } else {
            let software = decode(false)?;
            (decode(true)?, software)
        };
        anyhow::ensure!(
            hardware.frames.len() == software.frames.len(),
            "frame count differs"
        );
        let mut error = 0u64;
        let mut channels = 0u64;
        for ((h, hd), (s, sd)) in hardware.frames.iter().zip(&software.frames) {
            anyhow::ensure!(
                h.size == s.size && hd == sd,
                "dimensions or frame duration differs"
            );
            for (h, s) in h.pixels.iter().zip(&s.pixels) {
                for channel in 0..3 {
                    error += u64::from(h[channel].abs_diff(s[channel]));
                    channels += 1;
                }
            }
        }
        result = serde_json::json!({
            "frames": hardware.frames.len(), "dimensions": hardware.frames[0].0.size,
            "rgb_mean_absolute_difference": error as f64 / channels.max(1) as f64,
            "frame_center_rgb": hardware.frames.iter().map(|(image, _)| {
                image.pixels[image.pixels.len()/2 + image.size[0]/2].to_array()
            }).collect::<Vec<_>>(),
        });
    }
    hardware_ms.sort_by(f64::total_cmp);
    software_ms.sort_by(f64::total_cmp);
    result["hardware_ms"] = serde_json::json!(hardware_ms);
    result["software_ms"] = serde_json::json!(software_ms);
    result["hardware_median_ms"] = serde_json::json!(hardware_ms[runs / 2]);
    result["software_median_ms"] = serde_json::json!(software_ms[runs / 2]);
    result["runs"] = serde_json::json!(runs);
    Ok(result)
}

/// Saves the first frame from each decoder for visual comparison of scaling/colour.
pub fn save_first_frames(path: &Path, directory: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(directory)?;
    for (name, decoded) in [
        ("hardware", videotoolbox::decode_for_probe(path)),
        ("software", decode_mp4(path)),
    ] {
        let decoded = decoded.ok_or_else(|| anyhow::anyhow!("{name} decode failed"))?;
        let image = &decoded.frames[0].0;
        let rgba = image
            .pixels
            .iter()
            .flat_map(|pixel| pixel.to_srgba_unmultiplied())
            .collect();
        let image = image::RgbaImage::from_raw(image.size[0] as u32, image.size[1] as u32, rgba)
            .ok_or_else(|| anyhow::anyhow!("invalid dimensions"))?;
        image.save(directory.join(format!("{name}.png")))?;
    }
    Ok(())
}
