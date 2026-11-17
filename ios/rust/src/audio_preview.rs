//! Bounded native voice preparation, independent of the serialized control API.
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, mpsc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

static TEMPORARY_ID: AtomicU64 = AtomicU64::new(0);

struct Job {
    request: String,
    source: PathBuf,
    cache: PathBuf,
    cancel: Arc<AtomicBool>,
}

/// One conversion runs and only the latest request waits: a newer tap
/// supersedes the waiting job and cancels the running one.
#[derive(Default)]
struct Slot {
    pending: Option<Job>,
    running: Option<Arc<AtomicBool>>,
    stopped: bool,
}

type Wake = Arc<Mutex<Option<extern "C" fn()>>>;

pub(super) struct AudioJobs {
    shared: Arc<(Mutex<Slot>, Condvar)>,
    output: mpsc::Sender<Value>,
    results: mpsc::Receiver<Value>,
    wake: Wake,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|p| p.into_inner())
}

fn signal(wake: &Wake) {
    // Held while calling, so Drop cannot return while a callback is running.
    if let Some(wake) = *lock(wake) {
        wake();
    }
}

impl AudioJobs {
    pub(super) fn new(callback: Option<extern "C" fn()>) -> Self {
        let shared = Arc::new((Mutex::new(Slot::default()), Condvar::new()));
        let (output, results) = mpsc::channel();
        let wake = Arc::new(Mutex::new(callback));
        let worker = shared.clone();
        let sender = output.clone();
        let wakeup = wake.clone();
        std::thread::spawn(move || {
            let (slot, ready) = &*worker;
            loop {
                let job = {
                    let mut state = lock(slot);
                    loop {
                        if state.stopped {
                            return;
                        }
                        if let Some(job) = state.pending.take() {
                            state.running = Some(job.cancel.clone());
                            break job;
                        }
                        state = ready.wait(state).unwrap_or_else(|p| p.into_inner());
                    }
                };
                // A cancelled conversion fails and removes its own partial file.
                let path = convert(&job.source, &job.cache, &job.cancel).ok();
                {
                    let mut state = lock(slot);
                    state.running = None;
                    if state.stopped {
                        return;
                    }
                }
                let _ = sender.send(json!({"type":"audio","id":job.request,"path":path}));
                signal(&wakeup);
            }
        });
        Self {
            shared,
            output,
            results,
            wake,
        }
    }

    /// Accepts every request while running. A superseded waiting request
    /// is answered at once without a file, so its callback is released.
    pub(super) fn submit(&self, request: String, source: PathBuf, cache: PathBuf) -> bool {
        let (slot, ready) = &*self.shared;
        let superseded = {
            let mut state = lock(slot);
            if state.stopped {
                return false;
            }
            if let Some(running) = &state.running {
                running.store(true, Ordering::Release);
            }
            let previous = state.pending.replace(Job {
                request,
                source,
                cache,
                cancel: Arc::new(AtomicBool::new(false)),
            });
            ready.notify_one();
            previous
        };
        if let Some(job) = superseded {
            let _ = self
                .output
                .send(json!({"type":"audio","id":job.request,"path":Value::Null}));
            signal(&self.wake);
        }
        true
    }

    pub(super) fn poll(&self) -> Vec<Value> {
        self.results.try_iter().collect()
    }
}

impl Drop for AudioJobs {
    fn drop(&mut self) {
        let (slot, ready) = &*self.shared;
        {
            let mut state = lock(slot);
            state.stopped = true;
            state.pending = None;
            if let Some(running) = &state.running {
                running.store(true, Ordering::Release);
            }
        }
        ready.notify_all();
        // No callback may outlive wa_stop, even if a decoder is being cancelled.
        *lock(&self.wake) = None;
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
    fn rapid_requests_are_all_answered_and_the_latest_is_converted() {
        let root = tempfile::tempdir().unwrap();
        let cache = root.path().join("preview");
        let clips: Vec<_> = [8 * 48_000, 48_000, 24_000]
            .into_iter()
            .enumerate()
            .map(|(index, samples)| {
                let source = root.path().join(format!("voice-{index}.ogg"));
                fs::write(&source, crate::voice::encode(&vec![0.2; samples]).unwrap()).unwrap();
                source
            })
            .collect();
        let jobs = AudioJobs::new(None);
        for (index, source) in clips.iter().enumerate() {
            assert!(jobs.submit(format!("request-{index}"), source.clone(), cache.clone()));
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut results = Vec::new();
        while results.len() < 3 && std::time::Instant::now() < deadline {
            results.extend(jobs.poll());
            std::thread::sleep(Duration::from_millis(10));
        }
        let mut ids: Vec<_> = results
            .iter()
            .map(|value| value["id"].as_str().unwrap().to_owned())
            .collect();
        ids.sort();
        assert_eq!(ids, ["request-0", "request-1", "request-2"]);
        let latest = results
            .iter()
            .find(|value| value["id"] == "request-2")
            .unwrap();
        let wave = fs::read(latest["path"].as_str().unwrap()).unwrap();
        assert_eq!(&wave[..4], b"RIFF");
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
