//! Local video posters: one background decoder, eight resident textures and a
//! bounded disk cache. Loading a poster never opens a player or uses the network.

use std::collections::HashMap;
use std::fs::{self, File, FileTimes, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak, mpsc};
use std::time::{SystemTime, UNIX_EPOCH};

use egui::load::{
    LoadError, SizeHint, SizedTexture, TextureLoadResult, TextureLoader, TexturePoll,
};
use egui::{ColorImage, TextureHandle, TextureOptions};
use sha2::{Digest, Sha256};

use crate::backend::Waker;

const PREFIX: &str = "video-poster://";
const ID: &str = concat!(module_path!(), "::Loader");
const ENTRIES: usize = 8;
const MAX_JPEG: u64 = 512 * 1024;
const DISK_BYTES: u64 = 64 * 1024 * 1024;

pub fn uri(path: &Path) -> String {
    format!("{PREFIX}{}", path.display())
}

pub fn install(ctx: &egui::Context, directory: PathBuf, waker: Waker) {
    if !ctx.is_loader_installed(ID) {
        ctx.add_texture_loader(Arc::new(Loader {
            directory,
            cache: Arc::default(),
            waker,
        }));
    }
}

type Key = (String, TextureOptions);

#[derive(Clone)]
enum State {
    Pending,
    Decoded(Arc<ColorImage>),
    Ready(TextureHandle),
    Failed,
}

struct Entry {
    generation: u64,
    seen: u64,
    state: State,
}

#[derive(Default)]
struct Cache {
    next: u64,
    entries: HashMap<Key, Entry>,
}

struct Loader {
    directory: PathBuf,
    cache: Arc<Mutex<Cache>>,
    waker: Waker,
}

impl TextureLoader for Loader {
    fn id(&self) -> &str {
        ID
    }

    fn load(
        &self,
        ctx: &egui::Context,
        uri: &str,
        options: TextureOptions,
        _: SizeHint,
    ) -> TextureLoadResult {
        let Some(path) = uri.strip_prefix(PREFIX) else {
            return Err(LoadError::NotSupported);
        };
        let key = (uri.to_owned(), options);
        let pass = ctx.cumulative_pass_nr();
        let cached = {
            let mut cache = self.cache.lock().unwrap_or_else(|p| p.into_inner());
            cache.entries.get_mut(&key).map(|entry| {
                entry.seen = pass;
                (entry.generation, entry.state.clone())
            })
        };
        match cached {
            Some((_, State::Ready(texture))) => Ok(TexturePoll::Ready {
                texture: SizedTexture::from_handle(&texture),
            }),
            Some((generation, State::Decoded(image))) => {
                // GPU uploads belong to the requesting UI pass, outside the cache lock.
                let texture =
                    ctx.load_texture("video poster", egui::ImageData::Color(image), options);
                let mut cache = self.cache.lock().unwrap_or_else(|p| p.into_inner());
                if let Some(entry) = cache
                    .entries
                    .get_mut(&key)
                    .filter(|entry| entry.generation == generation)
                {
                    entry.state = State::Ready(texture.clone());
                    Ok(TexturePoll::Ready {
                        texture: SizedTexture::from_handle(&texture),
                    })
                } else {
                    Ok(TexturePoll::Pending { size: None })
                }
            }
            Some((_, State::Failed)) => {
                Err(LoadError::Loading("Local video preview unavailable".into()))
            }
            Some((_, State::Pending)) => Ok(TexturePoll::Pending { size: None }),
            None => {
                let generation = {
                    let mut cache = self.cache.lock().unwrap_or_else(|p| p.into_inner());
                    if cache.entries.len() >= ENTRIES {
                        let oldest = cache
                            .entries
                            .iter()
                            .filter(|(_, entry)| entry.seen < pass)
                            .min_by_key(|(_, entry)| entry.seen)
                            .map(|(key, _)| key.clone());
                        let Some(oldest) = oldest else {
                            return Ok(TexturePoll::Pending { size: None });
                        };
                        cache.entries.remove(&oldest);
                    }
                    cache.next += 1;
                    let generation = cache.next;
                    cache.entries.insert(
                        key.clone(),
                        Entry {
                            generation,
                            seen: pass,
                            state: State::Pending,
                        },
                    );
                    generation
                };
                let job = Job {
                    source: PathBuf::from(path),
                    directory: self.directory.clone(),
                    cache: Arc::downgrade(&self.cache),
                    key: key.clone(),
                    generation,
                    waker: self.waker.clone(),
                };
                match worker().map(|worker| worker.try_send(job)) {
                    Some(Ok(())) => {}
                    Some(Err(mpsc::TrySendError::Full(_))) => {
                        self.cache
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .entries
                            .remove(&key);
                        // Old window jobs may still occupy the shared queue. They
                        // cannot wake this context, so retry once capacity clears.
                        ctx.request_repaint_after(std::time::Duration::from_millis(100));
                    }
                    _ => {
                        if let Some(entry) = self
                            .cache
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .entries
                            .get_mut(&key)
                        {
                            entry.state = State::Failed;
                        }
                    }
                }
                Ok(TexturePoll::Pending { size: None })
            }
        }
    }

    fn forget(&self, uri: &str) {
        self.cache
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entries
            .retain(|(known, _), _| known != uri);
    }

    fn forget_all(&self) {
        self.cache
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entries
            .clear();
    }

    fn byte_size(&self) -> usize {
        self.cache
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entries
            .values()
            .map(|entry| match &entry.state {
                State::Ready(texture) => texture.size().into_iter().product::<usize>() * 4,
                State::Decoded(image) => image.pixels.len() * 4,
                State::Pending => 640 * 640 * 4,
                State::Failed => 0,
            })
            .sum()
    }
}

struct Job {
    source: PathBuf,
    directory: PathBuf,
    cache: Weak<Mutex<Cache>>,
    key: Key,
    generation: u64,
    waker: Waker,
}

impl Job {
    fn run(self) {
        let Some(cache) = self.cache.upgrade() else {
            return;
        };
        let current = |cache: &Cache| {
            cache.entries.get(&self.key).is_some_and(|entry| {
                entry.generation == self.generation && matches!(entry.state, State::Pending)
            })
        };
        if !current(&cache.lock().unwrap_or_else(|p| p.into_inner())) {
            return;
        }
        let image = cached_poster(&self.directory, &self.source).map(Arc::new);
        let published = {
            let mut cache = cache.lock().unwrap_or_else(|p| p.into_inner());
            if current(&cache) {
                cache.entries.get_mut(&self.key).unwrap().state =
                    image.map_or(State::Failed, State::Decoded);
                true
            } else {
                false
            }
        };
        if published {
            self.waker.wake();
        }
    }
}

fn worker() -> Option<&'static mpsc::SyncSender<Job>> {
    static WORKER: OnceLock<Option<mpsc::SyncSender<Job>>> = OnceLock::new();
    WORKER
        .get_or_init(|| {
            let (send, receive) = mpsc::sync_channel::<Job>(ENTRIES);
            std::thread::Builder::new()
                .name("video-posters".into())
                .spawn(move || {
                    for job in receive {
                        job.run();
                    }
                })
                .ok()
                .map(|_| send)
        })
        .as_ref()
}

fn cache_file(directory: &Path, source: &Path) -> Option<PathBuf> {
    let metadata = source.metadata().ok()?;
    if !metadata.is_file() {
        return None;
    }
    let modified = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    let mut hash = Sha256::new();
    hash.update(source.as_os_str().as_encoded_bytes());
    hash.update(metadata.len().to_le_bytes());
    hash.update(modified.as_nanos().to_le_bytes());
    let mut name = String::with_capacity(68);
    for byte in hash.finalize() {
        use std::fmt::Write;
        write!(name, "{byte:02x}").ok()?;
    }
    name.push_str(".jpg");
    Some(directory.join(name))
}

fn decode(bytes: &[u8]) -> Option<ColorImage> {
    if bytes.len() as u64 > MAX_JPEG {
        return None;
    }
    let reader =
        image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Jpeg);
    let (width, height) = reader.into_dimensions().ok()?;
    if width == 0 || height == 0 || width > 640 || height > 640 {
        return None;
    }
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg)
        .ok()?
        .to_rgba8();
    Some(ColorImage::from_rgba_unmultiplied(
        [width as usize, height as usize],
        &image,
    ))
}

fn cached_poster(directory: &Path, source: &Path) -> Option<ColorImage> {
    cached_poster_with(directory, source, crate::video_metadata::preview)
}

fn cached_poster_with(
    directory: &Path,
    source: &Path,
    generate: impl FnOnce(&Path) -> Option<Vec<u8>>,
) -> Option<ColorImage> {
    let file = cache_file(directory, source)?;
    if file
        .metadata()
        .ok()
        .is_some_and(|metadata| metadata.len() <= MAX_JPEG)
        && let Ok(bytes) = fs::read(&file)
        && let Some(image) = decode(&bytes)
        && cache_file(directory, source).as_ref() == Some(&file)
    {
        // Recently used posters survive the disk cache's next trim.
        if let Ok(file) = File::open(&file) {
            let _ = file.set_times(FileTimes::new().set_modified(SystemTime::now()));
        }
        return Some(image);
    }
    let bytes = generate(source)?;
    let image = decode(&bytes)?;
    if cache_file(directory, source).as_ref() != Some(&file) {
        return None;
    }
    if fs::create_dir_all(directory).is_ok() {
        let _ = fs::set_permissions(directory, fs::Permissions::from_mode(0o700));
        let temporary = file.with_extension("tmp");
        let stored = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)
            .and_then(|mut output| output.write_all(&bytes))
            .and_then(|_| fs::rename(&temporary, &file));
        if stored.is_err() {
            let _ = fs::remove_file(temporary);
        }
        prune(directory, DISK_BYTES);
    }
    Some(image)
}

/// Touch only derived posters in our dedicated directory, never media originals.
fn prune(directory: &Path, budget: u64) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut files: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let stem = path.file_stem()?.to_str()?;
            if path.extension()? != "jpg"
                || stem.len() != 64
                || !stem.bytes().all(|byte| byte.is_ascii_hexdigit())
                || !entry.file_type().ok()?.is_file()
            {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            Some((
                metadata.modified().unwrap_or(UNIX_EPOCH),
                metadata.len(),
                path,
            ))
        })
        .collect();
    files.sort_by_key(|(time, _, _)| *time);
    let mut total: u64 = files.iter().map(|(_, bytes, _)| bytes).sum();
    for (_, bytes, path) in files {
        if total <= budget {
            break;
        }
        if fs::remove_file(path).is_ok() {
            total = total.saturating_sub(bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sharp_rotated_posters_survive_reopening_and_invalidate_when_the_source_changes() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("portrait.mp4");
        let directory = root.path().join("posters");
        fs::write(
            &source,
            include_bytes!("../tests/fixtures/inline-video-portrait.mp4"),
        )
        .unwrap();
        let original = crate::video_metadata::read(&source).unwrap();
        let small = decode(original.thumbnail.as_deref().unwrap()).unwrap();
        assert!(
            small.size.into_iter().max().unwrap() <= 96,
            "outgoing thumbnails stay compact"
        );
        let image = cached_poster(&directory, &source).unwrap();
        assert_eq!(image.size, [360, 640]);
        assert!(
            image
                .pixels
                .iter()
                .any(|pixel| pixel.r() > 200 && pixel.g() < 30 && pixel.b() < 30)
        );
        let reopened = cached_poster_with(&directory, &source, |_| {
            panic!("a cached poster must not decode the video again")
        })
        .unwrap();
        assert_eq!(image, reopened);
        let old = cache_file(&directory, &source).unwrap();
        fs::write(&source, b"invalid replacement video").unwrap();
        assert_ne!(cache_file(&directory, &source).unwrap(), old);
        assert!(cached_poster(&directory, &source).is_none());
        assert!(cached_poster(&directory, &root.path().join("missing.mp4")).is_none());
    }

    #[test]
    fn requests_are_deduplicated_bounded_and_forgotten_jobs_cannot_reappear() {
        let root = tempfile::tempdir().unwrap();
        let loader = Loader {
            directory: root.path().to_owned(),
            cache: Arc::default(),
            waker: Waker::default(),
        };
        let ctx = egui::Context::default();
        ctx.begin_pass(egui::RawInput::default());
        let first = uri(&root.path().join("first.mp4"));
        for _ in 0..3 {
            let _ = loader.load(&ctx, &first, TextureOptions::LINEAR, SizeHint::default());
        }
        assert_eq!(loader.cache.lock().unwrap().next, 1);
        for index in 0..ENTRIES + 3 {
            let _ = loader.load(
                &ctx,
                &uri(&root.path().join(format!("{index}.mp4"))),
                TextureOptions::LINEAR,
                SizeHint::default(),
            );
        }
        assert_eq!(loader.cache.lock().unwrap().entries.len(), ENTRIES);
        assert!(loader.byte_size() <= ENTRIES * 640 * 640 * 4);
        loader.forget(&first);
        Job {
            source: root.path().join("first.mp4"),
            directory: root.path().to_owned(),
            cache: Arc::downgrade(&loader.cache),
            key: (first.clone(), TextureOptions::LINEAR),
            generation: 1,
            waker: Waker::default(),
        }
        .run();
        assert!(
            !loader
                .cache
                .lock()
                .unwrap()
                .entries
                .contains_key(&(first, TextureOptions::LINEAR))
        );
        loader.forget_all();
        assert_eq!(loader.byte_size(), 0);
        ctx.end_pass().textures_delta.clear();
    }

    #[test]
    fn disk_trimming_only_removes_old_derived_posters() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join(format!("{}.jpg", "a".repeat(64)));
        let recent = root.path().join(format!("{}.jpg", "b".repeat(64)));
        let original = root.path().join("original.mp4");
        for file in [&old, &recent, &original] {
            fs::write(file, vec![1; 80]).unwrap();
        }
        File::open(&old)
            .unwrap()
            .set_times(FileTimes::new().set_modified(UNIX_EPOCH))
            .unwrap();
        prune(root.path(), 80);
        assert!(!old.exists());
        assert!(recent.exists() && original.exists());
    }
}
