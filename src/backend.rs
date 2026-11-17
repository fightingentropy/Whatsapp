//! Channel bridge between the UI and asynchronous runtime.
//!
//! A dedicated tokio runtime owns the WhatsApp connection, archive, and media
//! work. Commands and events cross channels, and events wake the UI.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use crate::model::{Chat, ChatId, Contact, Gif, GifError, Message, StickerPack};
use crate::paths::AppDirs;

// Shared helpers for native pickers and future mobile attachment importers.
#[path = "backend/sticker_import.rs"]
pub mod sticker_import;
#[path = "backend/worker.rs"]
mod worker;

/// Phone-link state.
#[derive(Clone, PartialEq)]
pub enum LinkStatus {
    Starting,
    /// Waiting for QR scanning or pairing-code acceptance.
    Unlinked {
        qr: Option<String>,
        pair_code: Option<String>,
        pairing_phone: Option<String>,
    },
    Connecting,
    Connected,
    /// Connection dropped and automatic reconnection is active.
    Disconnected {
        reason: String,
    },
    /// Device unlinked by the phone.
    LoggedOut,
    Failed(String),
}

// Status transitions ship in the log. Never format QR payloads, pairing codes,
// phone numbers or server-provided error details into that log.
impl std::fmt::Debug for LinkStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Starting => "Starting",
            Self::Unlinked { .. } => "Unlinked",
            Self::Connecting => "Connecting",
            Self::Connected => "Connected",
            Self::Disconnected { .. } => "Disconnected",
            Self::LoggedOut => "LoggedOut",
            Self::Failed(_) => "Failed",
        })
    }
}

impl LinkStatus {
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }
}

/// Oldest loaded message timestamp and id used as a page boundary.
pub type PageKey = (i64, String);

/// Maximum number of messages returned by one archive page.
pub const PAGE: usize = 60;

#[derive(Clone, Debug)]
pub enum Command {
    SendText {
        chat: ChatId,
        text: String,
        quoting: Option<String>,
        mentions: Vec<String>,
    },
    /// Forwards an archived message to another chat.
    Forward {
        from_chat: ChatId,
        message: String,
        to_chat: ChatId,
    },
    /// Updates our typing state in a chat.
    Composing {
        chat: ChatId,
        composing: bool,
    },
    /// Marks a visible chat read and optionally sends receipts.
    MarkRead {
        chat: ChatId,
        receipts: bool,
    },
    /// Result of a private read-state update to the other linked devices.
    ReadSyncFinished {
        chat: ChatId,
        through: i64,
        success: bool,
    },
    /// Loads archived chat messages before an optional boundary.
    LoadChat {
        chat: ChatId,
        before: Option<PageKey>,
    },
    /// Loads an iPhone page with stable timestamp/id boundaries.
    LoadWindow {
        chat: ChatId,
        before: Option<PageKey>,
    },
    /// Loads a bounded iPhone window around a quoted/search message.
    LoadWindowAround {
        chat: ChatId,
        id: String,
    },
    /// Loads newer local rows for the iPhone's bounded history window.
    LoadNewer {
        chat: ChatId,
        after: PageKey,
    },
    /// Requests messages before the archive's earliest message.
    FetchOlder(ChatId),
    Download {
        chat: ChatId,
        message: String,
    },
    /// Requests a profile picture; `full` selects the info-dialog size.
    FetchAvatar {
        id: String,
        full: bool,
    },
    /// Loads archived messages from `id` through the current page.
    LoadUntil {
        chat: ChatId,
        id: String,
        before: PageKey,
    },
    /// Searches visible archived message text.
    SearchMessages {
        query: String,
    },
    /// Local archive search scoped to a chat and optional day.
    SearchChat {
        chat: ChatId,
        query: String,
        day: Option<(i64, i64)>,
        request: u64,
    },
    /// Creates an archive chat before its first message is sent.
    EnsureChat {
        chat: ChatId,
        name: String,
    },
    /// Internal result for a failed phone-history request.
    OlderFailed {
        chat: ChatId,
        error: String,
    },
    /// Internal result for a presence subscription that was not sent.
    PresenceFailed {
        chat: ChatId,
        /// The bot generation the subscription was sent on.
        generation: u64,
    },
    /// Internal group-metadata failure.
    GroupInfoFailed {
        chat: ChatId,
        /// Whether the server refusal is permanent.
        permanent: bool,
    },
    EditText {
        chat: ChatId,
        id: String,
        text: String,
        mentions: Vec<String>,
    },
    Revoke {
        chat: ChatId,
        id: String,
    },
    DeleteLocal {
        chat: ChatId,
        id: String,
    },
    /// Selects and sends files with the desktop picker.
    PickFiles(ChatId),
    /// Sends files with the caption on the first.
    SendFiles {
        chat: ChatId,
        paths: Vec<PathBuf>,
        caption: Option<String>,
        mentions: Vec<String>,
    },
    /// One native attachment retained by the caller until it reaches the archive.
    SendAttachment {
        chat: ChatId,
        path: PathBuf,
        caption: Option<String>,
        mentions: Vec<String>,
        request: String,
    },
    /// Retry an archived, failed outgoing message using its original message ID.
    RetrySend {
        chat: ChatId,
        id: String,
    },
    /// Internal native upload result; the UI owns the source until acknowledged.
    AttachmentPrepared {
        chat: ChatId,
        request: String,
        result: Result<(Box<Message>, Vec<u8>), String>,
    },
    /// Sends a clipboard image as straight-alpha RGBA.
    SendImage {
        chat: ChatId,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        /// HTTPS address of a copied web GIF or WebP whose still this is.
        /// An animated original is sent as a GIF instead of the still.
        source: Option<String>,
        caption: Option<String>,
        mentions: Vec<String>,
    },
    /// Syncs chat mute state. `Some(0)` is indefinite and `None` unmutes.
    SetMuted(ChatId, Option<i64>),
    /// Normalizes, encodes, and sends mono 48 kHz push-to-talk audio.
    SendVoice {
        chat: ChatId,
        samples: Vec<f32>,
        quoting: Option<String>,
    },
    /// Native recording retained until the archive acknowledges its upload.
    SendVoiceFile {
        chat: ChatId,
        path: PathBuf,
        quoting: Option<String>,
        request: String,
    },
    /// Sends a played receipt for a voice message.
    MarkPlayed {
        chat: ChatId,
        message: String,
        sender: String,
        receipts: bool,
    },
    /// Sends a WebP sticker.
    SendSticker {
        chat: ChatId,
        path: PathBuf,
    },
    /// Saves a sticker file.
    SaveSticker {
        path: PathBuf,
    },
    /// Removes a saved sticker.
    ForgetSticker {
        path: PathBuf,
    },
    /// Imports a pack from a signal.art link.
    ImportStickerUrl {
        url: String,
    },
    /// Selects and imports a .wastickers or zip archive.
    PickStickerArchive,
    /// Imports a file copied into private storage by a native picker.
    ImportStickerArchive {
        path: PathBuf,
    },
    /// Deletes an imported pack directory.
    DeleteStickerPack {
        dir: PathBuf,
    },
    /// Internal pack-import result. An empty error means the picker was canceled.
    StickerPackImported {
        result: Result<String, String>,
    },
    /// Saves a name through contact sync. `first_name` is the short display
    /// name; `to_phone` also adds it to the phone's address book.
    SaveContact {
        id: String,
        full_name: String,
        first_name: Option<String>,
        to_phone: bool,
    },
    /// Internal contact-save result.
    ContactSaved {
        id: String,
        name: String,
        error: Option<String>,
    },
    /// Checks a number, optionally saves it, and opens its chat.
    NewContact {
        phone: String,
        full_name: Option<String>,
        first_name: Option<String>,
        to_phone: bool,
    },
    /// Internal number-lookup result.
    ContactChecked {
        phone: String,
        full_name: Option<String>,
        first_name: Option<String>,
        to_phone: bool,
        registered: bool,
    },
    /// Downloads and sends a GIF as a short looping video.
    SendGif {
        chat: ChatId,
        gif: Gif,
    },
    /// Searches GIPHY or lists trending results for an empty query.
    SearchGifs {
        query: String,
        key: String,
    },
    /// Loads recent and saved stickers for the picker.
    RecentStickers,
    React {
        chat: ChatId,
        message: String,
        emoji: String,
    },
    SetArchived(ChatId, bool),
    SetPinned(ChatId, bool),
    PairWithPhone(String),
    /// Unlinks the device remotely and locally.
    Unlink,
    Reconnect,
    /// Internal QR-expiry recovery, scoped to the client that ran out of codes.
    RestartPairing {
        generation: u64,
    },
    Shutdown,
    /// Internal: an edit or delete-for-everyone failed to send, so the message
    /// goes back to what it was, unless it has changed since.
    RestoreMessage {
        chat: ChatId,
        id: String,
        previous: Box<crate::model::Message>,
        applied: crate::model::Content,
        error: String,
    },
    /// Internal send result.
    Sent {
        chat: ChatId,
        id: String,
        error: Option<String>,
    },
    /// Internal attachment-download result.
    Downloaded {
        chat: ChatId,
        id: String,
        token: u64,
        result: Result<PathBuf, String>,
    },
    /// Internal recent-sticker download result.
    StickerFetched {
        hash: String,
        result: Result<PathBuf, String>,
    },
    /// Internal profile-picture result.
    AvatarFetched {
        id: String,
        full: bool,
        path: Option<PathBuf>,
        /// False when a refresh downloaded the picture already on disk.
        changed: bool,
    },
    /// Internal retryable profile-picture failure.
    AvatarFailed {
        id: String,
        full: bool,
        attempts: u32,
    },
    /// Internal account about-text result.
    MeInfo {
        about: Option<String>,
    },
    /// Internal GIPHY result.
    GifResults {
        query: String,
        results: Result<Vec<Gif>, GifError>,
    },
    /// Internal file-picker result.
    Picked {
        chat: ChatId,
        paths: Vec<PathBuf>,
    },
    /// Internal uploaded attachment ready for archiving and sending.
    Outbound {
        chat: ChatId,
        row: Box<Message>,
        raw: Vec<u8>,
    },
    /// Resolve the destination timer and durably prepare the actual wire body.
    PrepareSend {
        chat: ChatId,
        id: String,
        generation: u64,
        raw: Vec<u8>,
        previous_timer: Option<crate::archive::DisappearingTimer>,
        group_timer: Option<crate::archive::DisappearingTimer>,
        retry: bool,
        prepared: tokio::sync::mpsc::UnboundedSender<Result<Vec<u8>, String>>,
    },
    /// Internal send audience. The sender waits for it to be archived.
    GroupRecipients {
        chat: ChatId,
        id: String,
        recipients: Vec<String>,
        lids: Vec<(String, String)>,
        stored: tokio::sync::mpsc::UnboundedSender<bool>,
    },
    /// Internal group metadata result.
    GroupInfo {
        chat: ChatId,
        name: Option<String>,
        participants: Vec<String>,
        read_only: bool,
    },
    /// Internal pairing-code result.
    PairCode {
        generation: u64,
        result: Result<String, String>,
    },
    /// Internal account read-receipt setting.
    ReceiptsPrivacy {
        disabled: bool,
    },
    /// Ask GitHub whether a newer release exists.
    CheckForUpdates,
}

#[derive(Debug)]
pub enum Event {
    /// Native upload is archived, or failed before creating a message.
    Attachment {
        chat: ChatId,
        request: String,
        message: Option<String>,
        error: Option<String>,
    },
    Link(LinkStatus),
    /// Linked account identity.
    Me {
        id: String,
        name: Option<String>,
        about: Option<String>,
    },
    /// Full chat list, newest first.
    Chats(Vec<Chat>),
    ChatUpdated(Box<Chat>),
    /// Chat messages in ascending order. `older` prepends them; `complete`
    /// means the archive has no earlier rows.
    Messages {
        chat: ChatId,
        messages: Vec<Message>,
        older: bool,
        complete: bool,
        /// Completes a local LoadChat/LoadUntil query, not a live or phone-sync event.
        requested: bool,
    },
    /// Replaces the iPhone window around a requested message.
    WindowAround {
        chat: ChatId,
        messages: Vec<Message>,
        older_complete: bool,
        newer_complete: bool,
    },
    /// Ascending local rows after the iPhone history window.
    NewerMessages {
        chat: ChatId,
        messages: Vec<Message>,
        complete: bool,
    },
    /// A local query failed; release its loading state so reopening can retry.
    ChatLoadFailed {
        chat: ChatId,
        initial: bool,
        error: String,
    },
    MessageUpdated(Box<Message>),
    /// Files selected for the composer.
    Picked {
        chat: ChatId,
        paths: Vec<PathBuf>,
    },
    /// Live incoming message for desktop notification.
    Incoming {
        chat: ChatId,
        message: Box<Message>,
    },
    Contacts(Vec<Contact>),
    /// A learned privacy-id mapping merged the archive into one conversation.
    ChatMerged {
        from: ChatId,
        into: ChatId,
    },
    /// Message search results with their query, newest first.
    SearchHits {
        query: String,
        messages: Vec<Message>,
    },
    ChatSearchHits {
        chat: ChatId,
        request: u64,
        result: Result<Vec<Message>, String>,
        truncated: bool,
    },
    Typing {
        chat: ChatId,
        sender: String,
        composing: bool,
    },
    Presence {
        id: String,
        online: bool,
        last_seen: Option<i64>,
    },
    Avatar {
        id: String,
        full: bool,
        path: Option<PathBuf>,
    },
    MessageDeleted {
        chat: ChatId,
        id: String,
    },
    /// GIF search results or failure.
    Gifs {
        query: String,
        results: Result<Vec<Gif>, GifError>,
    },
    /// Saved stickers, imported packs, and recent stickers for the picker.
    Stickers {
        saved: Vec<PathBuf>,
        packs: Vec<StickerPack>,
        recent: Vec<PathBuf>,
    },
    Media {
        chat: ChatId,
        message: String,
        result: Result<PathBuf, String>,
    },
    /// Link-time history sync state.
    Syncing(bool),
    /// Reported history-sync percentage.
    SyncProgress(u32),
    /// Phone-history result. `more` indicates whether another request may help.
    OlderFetched {
        chat: ChatId,
        more: bool,
    },
    /// Whether account privacy disables direct-chat read receipts.
    ReceiptsPrivacy {
        disabled: bool,
    },
    /// Number lookup succeeded and its chat can open.
    ContactReady {
        id: String,
        name: Option<String>,
    },
    /// Number lookup failed; ends the New contact dialog's busy state.
    ContactFailed(String),
    /// A sticker pack import ended; `error` is None on success or cancel.
    StickerImportFinished {
        error: Option<String>,
    },
    /// Informational toast message.
    Info(String),
    /// A newer release than this build exists.
    UpdateAvailable {
        version: String,
        url: String,
    },
    Error(String),
}

/// Shortest gap between frames drawn for worker events. Repaints are not
/// vsync-limited, so a burst of events (a reconnect catch-up, a history
/// chunk) would otherwise draw back-to-back frames.
#[cfg(not(target_os = "ios"))]
const EVENT_BATCH: std::time::Duration = std::time::Duration::from_millis(33);

/// When the frame for a worker event should run: at once after a quiet
/// spell, otherwise one [`EVENT_BATCH`] after the previous event frame.
/// Returns the delay from `now` and the time of that frame, which may still
/// lie ahead when a burst has already scheduled it.
#[cfg(not(target_os = "ios"))]
fn event_frame(
    last: Option<std::time::Instant>,
    now: std::time::Instant,
) -> (std::time::Duration, std::time::Instant) {
    match last {
        // A frame is already scheduled; ask again in case another repaint
        // superseded that request.
        Some(last) if last > now => (last - now, last),
        Some(last) if now - last < EVENT_BATCH => {
            let due = last + EVENT_BATCH;
            (due - now, due)
        }
        _ => (std::time::Duration::ZERO, now),
    }
}

/// Cross-thread window wake handle.
#[derive(Clone, Default)]
#[cfg(not(target_os = "ios"))]
pub struct Waker(
    Arc<std::sync::Mutex<Option<egui::Context>>>,
    /// Time of the latest frame drawn or scheduled for a worker event.
    Arc<std::sync::Mutex<Option<std::time::Instant>>>,
);

#[cfg(not(target_os = "ios"))]
impl Waker {
    pub fn attach(&self, ctx: &egui::Context) {
        *self.0.lock().unwrap_or_else(|p| p.into_inner()) = Some(ctx.clone());
        *self.1.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }

    pub fn detach(&self) {
        *self.0.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }

    pub fn wake(&self) {
        if let Some(ctx) = self.0.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            ctx.request_repaint();
        } else {
            #[cfg(target_os = "macos")]
            crate::background::wake();
        }
    }

    /// Wakes for worker events. The first event after a quiet spell repaints
    /// at once, so a reply to the reader's own click is not held back; the
    /// rest of a burst shares one frame per `EVENT_BATCH`. Input still
    /// repaints at once.
    pub fn wake_batched(&self) {
        if let Some(ctx) = self.0.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            let mut last = self.1.lock().unwrap_or_else(|p| p.into_inner());
            let (delay, frame) = event_frame(*last, std::time::Instant::now());
            *last = Some(frame);
            ctx.request_repaint_after(delay);
        } else {
            #[cfg(target_os = "macos")]
            crate::background::wake();
        }
    }

    /// Schedules a delayed repaint.
    pub fn wake_after(&self, delay: std::time::Duration) {
        if let Some(ctx) = self.0.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            ctx.request_repaint_after(delay);
        }
    }
}

/// iOS delivers events to Swift's main queue without an egui context.
#[cfg(target_os = "ios")]
#[derive(Clone, Default)]
pub struct Waker(Option<Arc<dyn Fn() + Send + Sync>>);

#[cfg(target_os = "ios")]
impl Waker {
    pub fn new(callback: impl Fn() + Send + Sync + 'static) -> Self {
        Self(Some(Arc::new(callback)))
    }

    pub fn wake(&self) {
        if let Some(callback) = &self.0 {
            callback();
        }
    }

    /// The Swift side already coalesces main-queue drains.
    pub fn wake_batched(&self) {
        self.wake();
    }
}

/// UI handle to the backend runtime.
pub struct Backend {
    commands: mpsc::UnboundedSender<Command>,
    events: std::sync::mpsc::Receiver<Event>,
    thread: Option<std::thread::JoinHandle<()>>,
    offline: bool,
    #[cfg(any(test, feature = "demo"))]
    demo_commands: Option<std::sync::Mutex<Vec<Command>>>,
}

impl Backend {
    pub fn spawn(dirs: AppDirs, waker: Waker) -> Self {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = std::sync::mpsc::channel();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("whatsapp-runtime")
            .enable_all()
            .build()
            .expect("unable to start the async runtime");
        let worker_commands = command_tx.clone();
        let thread = std::thread::Builder::new()
            .name("whatsapp-backend".to_string())
            .spawn(move || {
                runtime.block_on(async move {
                    worker::run(dirs, event_tx, worker_commands, command_rx, waker).await;
                });
                runtime.shutdown_timeout(Duration::from_secs(3));
            })
            .expect("unable to start the backend thread");

        Self {
            commands: command_tx,
            events: event_rx,
            thread: Some(thread),
            offline: false,
            #[cfg(any(test, feature = "demo"))]
            demo_commands: None,
        }
    }

    /// Creates a disconnected backend and event sender for demos and tests.
    pub fn detached() -> (Self, std::sync::mpsc::Sender<Event>) {
        let (command_tx, _command_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = std::sync::mpsc::channel();
        (
            Self {
                commands: command_tx,
                events: event_rx,
                thread: None,
                offline: true,
                #[cfg(any(test, feature = "demo"))]
                demo_commands: None,
            },
            event_tx,
        )
    }

    /// Records commands without a runtime or network connection.
    #[cfg(test)]
    pub(crate) fn recording() -> (Self, mpsc::UnboundedReceiver<Command>) {
        let (mut backend, _) = Self::detached();
        let (commands, inbox) = mpsc::unbounded_channel();
        backend.commands = commands;
        backend.offline = false;
        (backend, inbox)
    }

    /// Disables commands except shutdown.
    pub fn set_offline(&mut self, offline: bool) {
        self.offline = offline;
    }

    pub fn is_offline(&self) -> bool {
        self.offline
    }

    pub fn send(&self, command: Command) {
        if self.offline && !matches!(command, Command::Shutdown) {
            #[cfg(any(test, feature = "demo"))]
            if let Some(commands) = &self.demo_commands {
                commands
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .push(command);
            }
            return;
        }
        let _ = self.commands.send(command);
    }

    /// Captures real UI commands for an offline demo's local responder.
    #[cfg(any(test, feature = "demo"))]
    pub(crate) fn record_demo_commands(&mut self) {
        assert!(self.offline && self.thread.is_none());
        self.demo_commands = Some(Default::default());
    }

    #[cfg(any(test, feature = "demo"))]
    pub(crate) fn take_demo_commands(&self) -> Vec<Command> {
        self.demo_commands
            .as_ref()
            .map_or_else(Vec::new, |commands| {
                std::mem::take(&mut *commands.lock().unwrap_or_else(|p| p.into_inner()))
            })
    }

    pub fn poll(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }

    pub fn shutdown(&mut self) {
        self.send(Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod status_tests {
    use super::LinkStatus;

    #[test]
    fn logged_statuses_never_include_pairing_payloads_or_error_details() {
        let status = LinkStatus::Unlinked {
            qr: Some("private-qr-payload".into()),
            pair_code: Some("secret-code".into()),
            pairing_phone: Some("private-phone".into()),
        };
        assert_eq!(format!("link: {status:?}"), "link: Unlinked");
        assert_eq!(
            format!("{:?}", LinkStatus::Failed("private server detail".into())),
            "Failed"
        );
        assert_eq!(
            format!(
                "{:?}",
                LinkStatus::Disconnected {
                    reason: "private detail".into()
                }
            ),
            "Disconnected"
        );
    }
}

#[cfg(all(test, not(target_os = "ios")))]
mod waker_tests {
    use super::{EVENT_BATCH, Waker, event_frame};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    #[test]
    fn a_burst_of_worker_events_draws_at_once_and_then_once_per_batch() {
        let ctx = egui::Context::default();
        // Settle egui's own start-up repaints first.
        for _ in 0..10 {
            ctx.run_ui(egui::RawInput::default(), |_| {})
                .textures_delta
                .clear();
            if !ctx.has_requested_repaint() {
                break;
            }
        }
        assert!(!ctx.has_requested_repaint());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
        ctx.set_request_repaint_callback(move |info| seen.lock().unwrap().push(info.delay));
        let waker = Waker::default();
        waker.attach(&ctx);
        for _ in 0..3 {
            waker.wake_batched();
        }
        let batched = requests.lock().unwrap().clone();
        assert_eq!(
            batched.first(),
            Some(&Duration::ZERO),
            "the first event repaints at once: {batched:?}"
        );
        assert!(
            batched[1..].iter().all(|delay| *delay > Duration::ZERO),
            "the rest of the burst asks for no second immediate frame: {batched:?}"
        );
        waker.wake();
        let all = requests.lock().unwrap().clone();
        assert_eq!(
            all.last(),
            Some(&Duration::ZERO),
            "plain wakes stay immediate"
        );
    }

    #[test]
    fn event_frames_are_immediate_when_quiet_and_one_batch_apart_in_a_burst() {
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        // Nothing drawn yet, or the last event frame is old: draw now.
        assert_eq!(event_frame(None, at(0)), (Duration::ZERO, at(0)));
        assert_eq!(event_frame(Some(at(0)), at(40)), (Duration::ZERO, at(40)));
        // Soon after an event frame: one batch after it.
        let due = at(0) + EVENT_BATCH;
        assert_eq!(event_frame(Some(at(0)), at(5)), (due - at(5), due));
        // That frame is already scheduled: later events share it.
        assert_eq!(event_frame(Some(due), at(9)), (due - at(9), due));
        // After it has run, the next event waits one more batch.
        let next = due + EVENT_BATCH;
        assert_eq!(event_frame(Some(due), due), (EVENT_BATCH, next));
    }
}
