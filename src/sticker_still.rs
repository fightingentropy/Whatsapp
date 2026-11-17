//! Sticker stills: the first frame of a sticker, decoded on one background
//! worker at most 320 pixels per side, with at most 64 resident textures.
//!
//! egui's own loader decodes every frame of a `.webp`/`.gif` on the interface
//! thread, keeps them all and animates the image; a sticker that is not playing
//! (picker tiles, offscreen or loading stickers in a chat) only needs this still.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak, mpsc};

use egui::load::{
    LoadError, SizeHint, SizedTexture, TextureLoadResult, TextureLoader, TexturePoll,
};
use egui::{ColorImage, TextureHandle, TextureOptions};

use crate::backend::Waker;

const PREFIX: &str = "sticker-still://";
/// Keeps egui from reading the URI as an animated `.webp`/`.gif` one.
const SUFFIX: &str = "?still";
const ID: &str = concat!(module_path!(), "::Loader");
const ENTRIES: usize = 64;
const SIDE: usize = 320;

pub fn uri(path: &Path) -> String {
    format!("{PREFIX}{}{SUFFIX}", path.display())
}

pub fn install(ctx: &egui::Context, waker: Waker) {
    if !ctx.is_loader_installed(ID) {
        ctx.add_texture_loader(Arc::new(Loader {
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
        let Some(path) = uri
            .strip_prefix(PREFIX)
            .and_then(|rest| rest.strip_suffix(SUFFIX))
        else {
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
                    ctx.load_texture("sticker still", egui::ImageData::Color(image), options);
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
            Some((_, State::Failed)) => Err(LoadError::Loading("Sticker unavailable".into())),
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
                State::Pending => SIDE * SIDE * 4,
                State::Failed => 0,
            })
            .sum()
    }
}

struct Job {
    source: PathBuf,
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
        let image = crate::animation::still_frame(&self.source).map(Arc::new);
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
                .name("sticker-stills".into())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A 64 px two-frame animated WebP: a red square, then a green one.
    fn moving_sticker(path: &Path) {
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
        let mut encoder = Encoder::new((side, side)).unwrap();
        encoder
            .add_frame(&square(0, 0, [255, 0, 0, 255]), 0)
            .unwrap();
        encoder
            .add_frame(&square(40, 40, [0, 255, 0, 255]), 100)
            .unwrap();
        std::fs::write(path, encoder.finalize(200).unwrap()).unwrap();
    }

    #[test]
    fn a_still_is_the_first_frame_and_at_most_the_preview_size() {
        let root = tempfile::tempdir().unwrap();
        let moving = root.path().join("moving.webp");
        moving_sticker(&moving);
        let still = crate::animation::still_frame(&moving).unwrap();
        assert_eq!(still.size, [64, 64]);
        let first = still.pixels[8 * 64 + 8];
        assert!(first.r() > 200 && first.a() > 200, "{first:?}");
        assert_eq!(still.pixels[48 * 64 + 48].a(), 0, "not the second frame");
        let large = root.path().join("large.webp");
        image::RgbaImage::from_pixel(1024, 512, image::Rgba([0, 0, 255, 255]))
            .save(&large)
            .unwrap();
        assert_eq!(
            crate::animation::still_frame(&large).unwrap().size,
            [320, 160]
        );
        assert!(crate::animation::still_frame(&root.path().join("missing.webp")).is_none());
    }

    #[test]
    fn egui_does_not_treat_a_still_as_an_animation() {
        let ctx = egui::Context::default();
        let still = uri(Path::new("/stickers/pack/001.webp"));
        let image = egui::Image::new(still.clone());
        assert_eq!(image.source(&ctx).uri(), Some(still.as_str()));
    }

    #[test]
    fn a_sticker_loads_once_in_the_background() {
        let root = tempfile::tempdir().unwrap();
        let moving = root.path().join("moving.webp");
        moving_sticker(&moving);
        let ctx = egui::Context::default();
        install(&ctx, Waker::default());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let texture = loop {
            ctx.begin_pass(egui::RawInput::default());
            let poll =
                ctx.try_load_texture(&uri(&moving), TextureOptions::LINEAR, SizeHint::default());
            ctx.end_pass().textures_delta.clear();
            match poll {
                Ok(TexturePoll::Ready { texture }) => break texture,
                Ok(TexturePoll::Pending { .. }) => {}
                Err(error) => panic!("{error}"),
            }
            assert!(std::time::Instant::now() < deadline, "the still decodes");
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert_eq!(texture.size, egui::vec2(64.0, 64.0));
        assert!(
            ctx.loaders()
                .texture
                .lock()
                .iter()
                .any(|loader| loader.id() == ID && loader.byte_size() == 64 * 64 * 4),
            "one resident still"
        );
    }

    #[test]
    fn resident_stills_are_bounded_and_forgotten_jobs_cannot_reappear() {
        let root = tempfile::tempdir().unwrap();
        let loader = Loader {
            cache: Arc::default(),
            waker: Waker::default(),
        };
        let ctx = egui::Context::default();
        ctx.begin_pass(egui::RawInput::default());
        let first = uri(&root.path().join("first.webp"));
        for _ in 0..3 {
            let _ = loader.load(&ctx, &first, TextureOptions::LINEAR, SizeHint::default());
        }
        assert_eq!(loader.cache.lock().unwrap().next, 1, "deduplicated");
        ctx.end_pass().textures_delta.clear();
        ctx.begin_pass(egui::RawInput::default());
        for index in 0..ENTRIES + 3 {
            let _ = loader.load(
                &ctx,
                &uri(&root.path().join(format!("{index}.webp"))),
                TextureOptions::LINEAR,
                SizeHint::default(),
            );
        }
        // A busy shared queue may refuse a job and drop its entry, never add one.
        assert!(loader.cache.lock().unwrap().entries.len() <= ENTRIES);
        assert!(loader.cache.lock().unwrap().next >= ENTRIES as u64);
        assert!(loader.byte_size() <= ENTRIES * SIDE * SIDE * 4);
        let kept = uri(&root.path().join("0.webp"));
        loader.forget(&kept);
        Job {
            source: root.path().join("0.webp"),
            cache: Arc::downgrade(&loader.cache),
            key: (kept.clone(), TextureOptions::LINEAR),
            generation: 2,
            waker: Waker::default(),
        }
        .run();
        assert!(
            !loader
                .cache
                .lock()
                .unwrap()
                .entries
                .contains_key(&(kept, TextureOptions::LINEAR))
        );
        loader.forget_all();
        assert_eq!(loader.byte_size(), 0);
        ctx.end_pass().textures_delta.clear();
    }
}
