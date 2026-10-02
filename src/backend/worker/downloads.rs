//! Shared media-download admission and atomic streaming to private files.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::Semaphore;
use whatsapp_rust::Client;
use whatsapp_rust::download::Downloadable;

const MAX_PENDING: usize = 256;

pub(super) struct Downloads {
    pending: HashMap<(String, String), HashSet<u64>>,
    next: u64,
    pub(super) slots: Arc<Semaphore>,
}

impl Default for Downloads {
    fn default() -> Self {
        Self {
            pending: HashMap::new(),
            next: 0,
            slots: Arc::new(Semaphore::new(if cfg!(target_os = "ios") { 2 } else { 4 })),
        }
    }
}

impl Downloads {
    /// None joins an existing job; Err asks the bubble to offer an explicit retry.
    pub(super) fn admit(&mut self, chat: &str, id: &str) -> Result<Option<u64>, ()> {
        let key = (chat.to_owned(), id.to_owned());
        if self.pending.contains_key(&key) {
            return Ok(None);
        }
        if self.pending.len() >= MAX_PENDING {
            return Err(());
        }
        self.next += 1;
        self.pending.insert(key, HashSet::from([self.next]));
        Ok(Some(self.next))
    }

    /// Publish the first success or final failure when identities merge jobs.
    /// A late failure must never erase a file that another alias downloaded.
    pub(super) fn finish(&mut self, chat: &str, id: &str, token: u64, success: bool) -> bool {
        let key = (chat.to_owned(), id.to_owned());
        let Some(tokens) = self.pending.get_mut(&key) else {
            return false;
        };
        if !tokens.remove(&token) {
            return false;
        }
        if success || tokens.is_empty() {
            self.pending.remove(&key);
            return true;
        }
        false
    }

    pub(super) fn rekey(&mut self, from: &str, into: &str) {
        let mut merged = HashMap::<_, HashSet<_>>::new();
        for ((chat, id), tokens) in self.pending.drain() {
            merged
                .entry((if chat == from { into.to_owned() } else { chat }, id))
                .or_default()
                .extend(tokens);
        }
        self.pending = merged;
    }

    pub(super) fn clear(&mut self) {
        self.pending.clear();
    }
}

/// Unverified data never replaces a previously downloaded attachment. A failed
/// or cancelled attempt removes only its own staging file, including after retry.
struct PartialFile(PathBuf);
impl Drop for PartialFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

impl PartialFile {
    async fn create(destination: &Path) -> Result<(Self, std::fs::File), String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = destination.parent().ok_or("Missing media directory")?;
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
        let path = parent.join(format!(
            ".download-{}-{}.part",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let file = tokio::fs::OpenOptions::new()
            .write(true)
            .read(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .await
            .map_err(|e| e.to_string())?
            .into_std()
            .await;
        Ok((Self(path), file))
    }

    async fn commit(self, destination: &Path) -> Result<PathBuf, String> {
        tokio::fs::rename(&self.0, destination)
            .await
            .map_err(|e| e.to_string())?;
        Ok(destination.to_owned())
    }
}

pub(super) async fn to_file(
    client: &Client,
    media: &dyn Downloadable,
    path: &Path,
) -> Result<PathBuf, String> {
    let (partial, file) = PartialFile::create(path).await?;
    let file = client
        .download_to_writer(media, file)
        .await
        .map_err(|e| e.to_string())?;
    drop(file);
    partial.commit(path).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn jobs_are_bounded_deduplicated_and_survive_identity_merges() {
        let mut downloads = Downloads::default();
        let first = downloads.admit("alias", "message").unwrap().unwrap();
        assert_eq!(downloads.admit("alias", "message"), Ok(None));
        let second = downloads.admit("phone", "message").unwrap().unwrap();
        downloads.rekey("alias", "phone");
        assert!(!downloads.finish("phone", "message", first, false));
        assert_eq!(downloads.admit("phone", "message"), Ok(None));
        assert!(downloads.finish("phone", "message", second, true));
        let first = downloads.admit("alias", "message").unwrap().unwrap();
        let second = downloads.admit("phone", "message").unwrap().unwrap();
        downloads.rekey("alias", "phone");
        assert!(downloads.finish("phone", "message", first, true));
        assert!(!downloads.finish("phone", "message", second, false));
        for n in 0..MAX_PENDING {
            assert!(downloads.admit("chat", &n.to_string()).unwrap().is_some());
        }
        assert_eq!(downloads.admit("chat", "overflow"), Err(()));
        downloads.clear();
        let fresh = downloads.admit("phone", "message").unwrap().unwrap();
        assert!(!downloads.finish("phone", "message", first, true));
        assert!(downloads.finish("phone", "message", fresh, true));
        let slots = downloads.slots.available_permits();
        let permits = downloads
            .slots
            .clone()
            .acquire_many_owned(slots as u32)
            .await
            .unwrap();
        assert!(downloads.slots.try_acquire().is_err());
        drop(permits);
        assert_eq!(downloads.slots.available_permits(), slots);
    }

    #[tokio::test]
    async fn failed_stream_preserves_existing_file_and_success_is_atomic() {
        use std::io::Write;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("video.mp4");
        std::fs::write(&path, b"existing").unwrap();
        let (partial, mut file) = PartialFile::create(&path).await.unwrap();
        file.write_all(b"unverified").unwrap();
        let staging = partial.0.clone();
        drop((partial, file));
        assert!(!staging.exists());
        assert_eq!(std::fs::read(&path).unwrap(), b"existing");
        let (partial, mut file) = PartialFile::create(&path).await.unwrap();
        file.write_all(b"verified").unwrap();
        drop(file);
        partial.commit(&path).await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"verified");
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }
}
