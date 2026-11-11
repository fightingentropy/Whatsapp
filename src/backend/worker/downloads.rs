//! Shared media-download admission and atomic streaming to private files.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::Semaphore;
use whatsapp_rust::Client;
use whatsapp_rust::ErrorChainExt;
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
        // Marked before the rename, so the file never appears under its final
        // name without the mark.
        quarantine(&self.0);
        tokio::fs::rename(&self.0, destination)
            .await
            .map_err(|e| e.to_string())?;
        Ok(destination.to_owned())
    }
}

/// Marks a downloaded file as coming from the internet, the way browsers do.
/// Its name and extension were chosen by the sender, and with this mark macOS
/// checks the file and asks the user before it runs or opens anything risky.
/// Best effort: the download still succeeds if the mark cannot be written.
fn quarantine(path: &Path) {
    use std::ffi::{CString, c_char, c_int, c_void};
    use std::os::unix::ffi::OsStrExt;

    unsafe extern "C" {
        fn setxattr(
            path: *const c_char,
            name: *const c_char,
            value: *const c_void,
            size: usize,
            position: u32,
            options: c_int,
        ) -> c_int;
    }

    let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
        return;
    };
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    // flags;time;agent;event id. 0081 is a download the user has not approved yet.
    let value = format!("0081;{seconds:08x};Whatsapp;");
    // SAFETY: the path and the attribute name are NUL-terminated C strings and
    // `value` is a live buffer of `value.len()` bytes; all three outlive the call.
    let result = unsafe {
        setxattr(
            c_path.as_ptr(),
            c"com.apple.quarantine".as_ptr(),
            value.as_ptr().cast(),
            value.len(),
            0,
            0,
        )
    };
    if result != 0 {
        log::warn!(
            "could not quarantine a downloaded file: {}",
            std::io::Error::last_os_error()
        );
    }
}

pub(super) async fn to_file(
    client: &Client,
    media: &dyn Downloadable,
    path: &Path,
) -> Result<PathBuf, String> {
    to_file_classified(client, media, path)
        .await
        .map_err(|failure| failure.text)
}

/// A failed download: its message, and whether the server said the media
/// reference has expired.
pub(super) struct Failure {
    pub(super) text: String,
    pub(super) expired: bool,
}

/// Shown in the bubble when the media is gone from WhatsApp's servers.
pub(super) const EXPIRED_NOTICE: &str = "No longer available on WhatsApp's servers";

impl Failure {
    /// The text to show: the fixed notice for expired media, else the raw error.
    pub(super) fn into_text(self) -> String {
        if self.expired {
            EXPIRED_NOTICE.to_owned()
        } else {
            self.text
        }
    }
}

/// Whether the CDN refused the media reference as gone or no longer allowed.
/// This reads the typed HTTP status, never the error text, which can contain
/// these digits for unrelated reasons (paths, ids, ports).
fn is_expired(error: &anyhow::Error) -> bool {
    matches!(
        <dyn std::error::Error + Send + Sync + 'static>::http_status(error.as_ref()),
        Some(403 | 404 | 410)
    )
}

pub(super) async fn to_file_classified(
    client: &Client,
    media: &dyn Downloadable,
    path: &Path,
) -> Result<PathBuf, Failure> {
    let plain = |text: String| Failure {
        text,
        expired: false,
    };
    let (partial, file) = PartialFile::create(path).await.map_err(plain)?;
    let file = client
        .download_to_writer(media, file)
        .await
        .map_err(|error| Failure {
            expired: is_expired(&error),
            text: error.to_string(),
        })?;
    drop(file);
    partial.commit(path).await.map_err(plain)
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

    #[test]
    fn expiry_comes_from_the_http_status_not_the_error_text() {
        use whatsapp_rust::http::HttpStatusError;
        let rejected =
            |status: u16| anyhow::Error::new(HttpStatusError { status }).context("rejected");
        for status in [403, 404, 410] {
            assert!(is_expired(&rejected(status)), "{status}");
        }
        assert!(!is_expired(&rejected(401)));
        assert!(!is_expired(&rejected(500)));
        // Digits in a path or port are not a status.
        let text =
            anyhow::anyhow!("connect to media-404.example:4030 failed, file /cache/403-410.jpg");
        assert!(!is_expired(&text));
    }

    #[test]
    fn only_expired_failures_get_the_notice() {
        let local = Failure {
            text: "cannot write /cache/403-404/410.jpg".to_owned(),
            expired: false,
        };
        assert_eq!(local.into_text(), "cannot write /cache/403-404/410.jpg");
        let gone = Failure {
            text: "rejected".to_owned(),
            expired: true,
        };
        assert_eq!(gone.into_text(), EXPIRED_NOTICE);
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

    #[tokio::test]
    async fn committed_downloads_are_quarantined() {
        use std::io::Write;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("invoice.terminal");
        let (partial, mut file) = PartialFile::create(&path).await.unwrap();
        file.write_all(b"from a sender").unwrap();
        drop(file);
        partial.commit(&path).await.unwrap();

        let output = std::process::Command::new("/usr/bin/xattr")
            .args(["-p", "com.apple.quarantine"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(output.status.success(), "the quarantine mark is missing");
        let value = String::from_utf8(output.stdout).unwrap();
        let fields: Vec<&str> = value.trim().split(';').collect();
        assert_eq!(fields.len(), 4, "{value}");
        assert_eq!(fields[0], "0081");
        assert!(u64::from_str_radix(fields[1], 16).unwrap() > 0);
        assert_eq!(fields[2], "Whatsapp");
    }
}
