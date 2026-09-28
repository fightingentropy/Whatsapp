//! Bounded native voice preparation, independent of the serialized control API.
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

static TEMPORARY_ID: AtomicU64 = AtomicU64::new(0);

struct Job {
    request: String,
    source: PathBuf,
    cache: PathBuf,
}

pub(super) struct AudioJobs {
    sender: mpsc::SyncSender<Job>,
    results: mpsc::Receiver<Value>,
    cancelled: Arc<AtomicBool>,
    wake: Arc<Mutex<Option<extern "C" fn()>>>,
}

impl AudioJobs {
    pub(super) fn new(callback: Option<extern "C" fn()>) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<Job>(1);
        let (output, results) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let wake = Arc::new(Mutex::new(callback));
        let stopped = cancelled.clone();
        let signal = wake.clone();
        std::thread::spawn(move || {
            while let Ok(job) = receiver.recv() {
                if stopped.load(Ordering::Acquire) {
                    break;
                }
                let path = convert(&job.source, &job.cache, &stopped).ok();
                if stopped.load(Ordering::Acquire) {
                    break;
                }
                let _ = output.send(json!({"type":"audio","id":job.request,"path":path}));
                if let Some(wake) = *signal.lock().unwrap_or_else(|p| p.into_inner()) {
                    wake();
                }
            }
        });
        Self {
            sender,
            results,
            cancelled,
            wake,
        }
    }

    pub(super) fn submit(&self, request: String, source: PathBuf, cache: PathBuf) -> bool {
        self.sender
            .try_send(Job {
                request,
                source,
                cache,
            })
            .is_ok()
    }

    pub(super) fn poll(&self) -> Vec<Value> {
        self.results.try_iter().collect()
    }
}

impl Drop for AudioJobs {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        // No callback may outlive wa_stop, even if a decoder is being cancelled.
        *self.wake.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }
}

fn convert(source: &Path, cache: &Path, cancelled: &AtomicBool) -> Result<PathBuf, String> {
    let io = |error: std::io::Error| error.to_string();
    let file = File::open(source).map_err(io)?;
    let metadata = file.metadata().map_err(io)?;
    if metadata.len() > 64 * 1024 * 1024 {
        return Err("Audio exceeds the playback limit".into());
    }
    let revision = metadata
        .modified()
        .map_err(io)?
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let key = format!("{}-{}-{revision}", source.display(), metadata.len());
    let hash = Sha256::digest(key.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let output = cache.join(format!("{hash}.wav"));
    if output.is_file() {
        if let Ok(file) = File::options().write(true).open(&output) {
            let _ = file.set_modified(SystemTime::now());
        }
        prune(cache, &output);
        return Ok(output);
    }
    fs::create_dir_all(cache).map_err(io)?;
    // A cancelled generation can briefly finish a packet while the next starts.
    // Its cleanup must never remove the next generation's partial WAV.
    let nonce = TEMPORARY_ID.fetch_add(1, Ordering::Relaxed);
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let temporary = output.with_extension(format!("wav.{time}.{nonce}.tmp"));
    let result = (|| {
        let mut writer = BufWriter::new(
            File::options()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(io)?,
        );
        writer.write_all(&[0; 44]).map_err(io)?;
        let samples =
            crate::voice::decode_chunks(BufReader::new(file), 48_000 * 600 + 960, |samples| {
                if cancelled.load(Ordering::Acquire) {
                    return Err("Cancelled".into());
                }
                let pcm: Vec<u8> = samples
                    .iter()
                    .flat_map(|sample| ((sample.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
                    .collect();
                writer.write_all(&pcm).map_err(io)?;
                Ok(())
            })?;
        let length = u32::try_from(samples * 2).map_err(|e| e.to_string())?;
        let mut header = Vec::with_capacity(44);
        header.extend_from_slice(b"RIFF");
        header.extend_from_slice(&(length + 36).to_le_bytes());
        header.extend_from_slice(b"WAVEfmt ");
        header.extend_from_slice(&16_u32.to_le_bytes());
        header.extend_from_slice(&1_u16.to_le_bytes());
        header.extend_from_slice(&1_u16.to_le_bytes());
        header.extend_from_slice(&48_000_u32.to_le_bytes());
        header.extend_from_slice(&96_000_u32.to_le_bytes());
        header.extend_from_slice(&2_u16.to_le_bytes());
        header.extend_from_slice(&16_u16.to_le_bytes());
        header.extend_from_slice(b"data");
        header.extend_from_slice(&length.to_le_bytes());
        writer.seek(SeekFrom::Start(0)).map_err(io)?;
        writer.write_all(&header).map_err(io)?;
        writer.flush().map_err(io)?;
        if cancelled.load(Ordering::Acquire) {
            return Err("Cancelled".into());
        }
        fs::rename(&temporary, &output).map_err(io)?;
        prune(cache, &output);
        Ok(output)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn prune(cache: &Path, current: &Path) {
    let Ok(entries) = fs::read_dir(cache) else {
        return;
    };
    let mut files: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension()?.to_str()? == "tmp" {
                if entry.metadata().ok()?.modified().ok()?.elapsed().ok()?
                    > Duration::from_secs(7 * 86_400)
                {
                    let _ = fs::remove_file(path);
                }
                return None;
            }
            if path.extension()?.to_str()? != "wav" {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            Some((path, metadata.modified().ok()?, metadata.len()))
        })
        .collect();
    files.sort_by_key(|(path, time, _)| (path != current, std::cmp::Reverse(*time)));
    let mut bytes = 0;
    for (index, (path, _, size)) in files.into_iter().enumerate() {
        bytes += size;
        // Keep the current and preceding player files; only one player is active.
        if index >= 2 && path != current && (index >= 16 || bytes > 192 * 1024 * 1024) {
            let _ = fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_pruning_bounds_files_and_removes_only_abandoned_partials() {
        let root = tempfile::tempdir().unwrap();
        for index in 0..20 {
            let file = File::create(root.path().join(format!("{index}.wav"))).unwrap();
            file.set_modified(UNIX_EPOCH + Duration::from_secs(index))
                .unwrap();
        }
        let old = root.path().join("abandoned.wav.tmp");
        File::create(&old)
            .unwrap()
            .set_modified(UNIX_EPOCH)
            .unwrap();
        let fresh = root.path().join("active.wav.tmp");
        File::create(&fresh).unwrap();
        let current = root.path().join("0.wav");
        prune(root.path(), &current);
        assert!(current.exists());
        assert!(fresh.exists());
        assert!(!old.exists());
        assert!(!root.path().join("1.wav").exists());
        assert_eq!(
            fs::read_dir(root.path())
                .unwrap()
                .flatten()
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "wav"))
                .count(),
            16
        );
    }

    #[test]
    fn cached_revision_and_cancelled_conversion_are_safe() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("voice.ogg");
        let cache = root.path().join("preview");
        fs::write(&source, crate::voice::encode(&vec![0.2; 48_000]).unwrap()).unwrap();
        assert!(convert(&source, &cache, &AtomicBool::new(true)).is_err());
        assert_eq!(fs::read_dir(&cache).unwrap().count(), 0);
        let output = convert(&source, &cache, &AtomicBool::new(false)).unwrap();
        let wave = fs::read(&output).unwrap();
        assert_eq!(&wave[..4], b"RIFF");
        assert_eq!(
            wave.len() - 44,
            u32::from_le_bytes(wave[40..44].try_into().unwrap()) as usize
        );
        assert_eq!(
            convert(&source, &cache, &AtomicBool::new(false)).unwrap(),
            output
        );
        fs::write(&source, crate::voice::encode(&vec![0.1; 96_000]).unwrap()).unwrap();
        assert_ne!(
            convert(&source, &cache, &AtomicBool::new(false)).unwrap(),
            output
        );
    }
}
