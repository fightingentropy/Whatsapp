<p align="center"><img src="packaging/macos/icon-1024.png" width="112" alt="Whatsapp icon"></p>

# Whatsapp

A small native WhatsApp companion for **Apple Silicon Macs (M1 and newer)**.
Built with Rust, [egui](https://github.com/emilk/egui) and
[whatsapp-rust](https://github.com/oxidezap/whatsapp-rust). Charcoal surfaces,
green accents, a handset-and-chat icon, and a dark theme by default.

The standalone project lives at [fightingentropy/Whatsapp](https://github.com/fightingentropy/Whatsapp).
There is no browser engine, hosted backend, telemetry or extra account.
Build from source below; signed, notarized public downloads will appear under
[Releases](https://github.com/fightingentropy/Whatsapp/releases).

## Apple Silicon changes

- ARM64-only builds, CI, and DMG packaging. The compiler targets the M1 baseline
  so newer M-series Macs can run the same app. The deployment target is macOS 11;
  current validation runs on newer macOS versions, not every supported OS release.
- A main-run-loop wake source replaces the closed-window 150 ms polling loop.
  Backend messages and native events wake it; pending settings, audio and other
  real deadlines still run. The protocol worker retains its 5-second maintenance
  tick and the protocol library's own network timers.
- Message search waits 180 ms after typing and uses an FTS5 trigram index for
  literal substrings of three or more characters. A bounded recent-message check
  handles common terms without sorting a large match set. Short searches retain the
  original matching rules. The index is derived from the local archive and
  follows edits, replays, deletions and chat merges. Its initial build and disk
  space are additional costs.
- History is saved in transactions of up to 256 messages with cached SQL
  statements, preserving raw attachment keys and delivery state.
- Long conversations reuse measured row heights and skip offscreen bubble layout.
  Search jumps, history insertion, edits and resizing preserve the reading position.
  Rows are remeasured when their content or layout changes; text selection keeps
  the full message range registered for cross-message copying.
- The main and archived chat lists copy only visible rows for drawing, avoiding
  repeated copies of offscreen chat previews and group membership on every repaint.
- Inactive conversation history keeps up to eight chats within an estimated
  32 MiB message/layout budget. Open chats and active loads, sends and downloads
  are protected and can exceed that budget. Evicted histories reload from SQLite;
  drafts and phone-history backoff survive. Associated thumbnail and unshared
  image-loader allocations are released, while downloaded files stay on disk.
  Renderer, codec and protected-chat allocations are additional memory.
- Animated media keeps one GPU texture per clip and uploads the current frame
  as needed. Decoded pixels have a 128 MiB cache budget, unseen clips expire
  after 20 seconds, and preview width and height are bounded at 320 pixels.
  Playback starts as ordered frames arrive through a bounded queue of eight frames
  per decoder. Invisible unfinished jobs are cancelled; finished clips keep their
  loop cache (up to 150 frames) to avoid decoding on every loop. Decoder buffers,
  unpublished frames and GPU textures are additional memory outside that CPU cache.
- H.264 animated MP4 previews with at least 230,400 source pixels
  (equivalent to 640×360) and eight frames use Apple's VideoToolbox decoder, with a software
  fallback. Smaller clips avoid hardware setup costs. Decoding requests preview-sized output, limits work in
  flight, and preserves reordered frames at the end of a clip. Ordinary videos
  still open in your external player.
- System font collections share their bytes across faces. Apple Color Emoji
  supplies emoji by default; `--features bundled-emoji` restores the Noto
  compatibility fallback. Demo builds still include Noto for their sample art.
- App identity, data directories, single-instance signalling and update notices
  belong to Whatsapp. Existing ZapFast Silicon users keep their linked device,
  settings, messages and downloads through a one-time local migration. An
  upstream ZapFast installation's session is never adopted.

An optional Metal renderer is available for comparison with OpenGL (see below).
See [PERFORMANCE.md](PERFORMANCE.md) for measurements, remaining costs and validation details.

## What it does

- **Links to your phone.** Scan a QR code or link with your phone number.
  Recent history is copied to this computer after linking and stored here.
- **Chats.** See pinned, unread, muted, and archived chats, typing indicators,
  and message status. Search chats, saved messages, and contacts.
- **Read state across devices.** Reading a chat syncs its unread badge with
  your phone and other linked devices, including when read receipts are off.
  Replies from another device clear preceding unread messages. The read-receipt
  toggle also controls voice-message played receipts; account privacy is checked
  before sending receipts in direct chats. A hidden window does not read messages.
- **Conversations.** See replies, reactions, edits, deleted messages, read
  receipts, sender names, and group pictures. Older messages load as you
  scroll up, first from the local archive and then from your phone.
  Group messages show two gray checks after every recipient has received
  them, and blue checks after every recipient has read them. The recipient
  list and individual receipts are saved locally; later membership changes
  do not change that list. If the original recipients are unknown, Whatsapp
  waits for the phone's aggregate status instead of guessing from one reader.
- **WhatsApp formatting.** Bold, italic, strikethrough, code, lists, quotes,
  mentions, and link previews are supported. Links are clickable. Emoji use
  Apple Color Emoji (an optional bundled fallback is available), and emoji-only
  messages are larger.
- **Send attachments with captions.** Paste a picture, drop files, or use the
  file picker. They stay in the composer until you send them or press Escape.
- **Mute chats** for eight hours, one week, or indefinitely. The setting also
  applies on your phone and to desktop notifications.
- **Voice messages.** Play, seek, record, reply with, and send voice messages
  in the chat. The app normalizes quiet recordings and handles OGG/Opus
  without external tools.
- **Send messages.** Press Enter to send text and Shift+Enter for a new line.
  You can swap these keys in Settings. The composer is focused when you open
  or return to a conversation; invoking search keeps focus in search, and
  Escape clears search and returns to the composer. Type `:name` to autocomplete
  an emoji without leaving the composer, or `@` in a group to mention a member.
  Reply, react, edit, forward, delete, and check when a message was sent,
  delivered, or read.
- **View attachments.** Whatsapp downloads files up to 64 MB automatically or
  on click. Photos, stickers, GIFs, voice messages, audio, locations, contacts,
  polls, and link previews appear in the chat. Videos and documents open in
  their default desktop apps. If an attachment has expired, Whatsapp asks your
  phone to upload it again.
- **Emoji, GIF, and sticker picker.** Search emoji and GIFs, use recent emoji
  and stickers, and save stickers with a right-click. Emoji autocomplete and
  picker search select their first match; use the arrow keys and Enter to
  choose it. GIF search needs a free GIPHY API key unless the build includes
  one.
- **Sticker packs.** Import a pack from a `signal.art` link or `.wastickers`
  file. Animated packs remain animated. Packs are stored as WebP files on your
  computer.
- **Consistent names.** Use names from your address book or public WhatsApp
  profile names across chats, replies, mentions, and notifications.
- **Groups.** See members, sender names, and sender pictures. Announcement
  groups are read-only for non-admins.
- **Presence.** See online, last-seen, and typing status, and send your typing
  status.
- **Runs in the background.** Closing the window keeps Whatsapp linked in the
  system tray. Reopen it from the tray or by launching it again. Quit from the
  tray or with `⌘Q`, or disable this behavior in Settings.
- **Desktop notifications.** Get notifications with the chat picture when you
  are away from the open chat. Muted chats do not notify you.
- **Update notices.** Whatsapp checks GitHub once a day and shows a download
  link when a newer release is available. You can turn this off in Settings.
- **Dark by default**, with light and system options. Zoom with ⌘plus and
  ⌘minus.
- **Copy text.** Select part of a message or copy across messages in
  WhatsApp's `[time, date] Name:` format. Contact names and numbers are also
  selectable.
- **Keyboard shortcuts.** `⌘K` searches, `Alt+↑/↓` switches chats and
  keeps the active chat visible in the list, `Esc` cancels the current action,
  and `⌘/` lists all shortcuts.
- **Local storage.** Messages are stored in one SQLite file and attachments
  in the cache directory. Unlinking deletes both and removes this device from
  your phone.

## What it does not do yet

- Play ordinary videos in the app (they open in your player), or reply to
  a message with an attachment.
- Calls, status posts, communities, newsletters, and group administration.

## Build and run

Use an Apple Silicon Mac, Xcode Command Line Tools, CMake and Rust via rustup.
`rust-toolchain.toml` pins Rust 1.98.0; `.cargo/config.toml` selects
`aarch64-apple-darwin`. Intel, Windows and Linux builds are unsupported.

```sh
cargo build --locked --release
./target/aarch64-apple-darwin/release/whatsapp
```

To build the experimental Metal backend, add `--features metal`, then launch
with `--renderer metal`. `--renderer open-gl` selects the existing renderer in
the same build. OpenGL remains the default. The Metal feature enables only the
Apple backend of wgpu. Both renderers repaint on demand.

Offline measurements (no linked account required):

```sh
cargo build --locked --release --features demo,metal
./target/aarch64-apple-darwin/release/whatsapp --renderer metal --demo-benchmark /tmp/metal.json
./target/aarch64-apple-darwin/release/whatsapp --renderer open-gl --demo-benchmark /tmp/opengl.json
./target/aarch64-apple-darwin/release/whatsapp --demo --demo-page long --demo-benchmark /tmp/scroll.json --demo-benchmark-scroll
# Compare the same 10,000 messages with offscreen layout enabled:
./target/aarch64-apple-darwin/release/whatsapp --demo --demo-page long --demo-benchmark /tmp/full-scroll.json --demo-benchmark-scroll --demo-full-layout
cargo run --locked --release --features demo --example conversation_probe > /tmp/layout.json
cargo run --locked --release --features demo --example video_probe -- tests/fixtures/h264-bframes.mp4
```

The video probe requires an actual hardware decoder and fails when unavailable;
it never silently measures the software fallback as hardware. It also reports time
to the first ordered CPU frame; this excludes window scheduling and GPU upload.
The conversation probe runs the same scrolling UI without a window. Its CPU timings
exclude native rendering, tessellation and GPU work; they are not frame-rate measurements.

For a local test DMG:

```sh
bash packaging/macos/build-local.sh
open "dist/Whatsapp-0.13.1-local-arm64.dmg"
```

The helper signs the app ad hoc in a temporary directory outside iCloud, then
copies the finished DMG back to `dist/`. It does not install or notarize the app.
Public releases require Developer ID signing and notarization; see
[PACKAGING.md](PACKAGING.md).

On first start, use WhatsApp on your phone: **Linked devices → Link a device**.
Scan the QR code, or choose phone-number linking. History arrives from the phone
and remains in this app's local archive. Settings is available with `⌘,`.
When the QR session expires, the app requests a fresh session automatically.
If no code is available, **Get a new QR code** retries without restarting the app
or deleting its data. An outstanding phone-number pairing code is preserved.
The pairing card scrolls in short windows so every linking option stays reachable.

## Files

Settings, `session.db`, `archive.db`, saved stickers and logs are under
`~/Library/Application Support/org.erlin.whatsapp/`.
Downloaded media and avatars are under
`~/Library/Caches/org.erlin.whatsapp/`.
Window state uses eframe's independent `whatsapp` app id.
On first launch after upgrading from our previous **ZapFast Silicon** build,
Whatsapp moves only the `org.erlin.zapfast-silicon` profile and cache to these
new locations. Existing destination directories are never overwritten or
merged. SQLite databases move with their journal files; a failed move stops
startup so it cannot silently replace your account with an empty profile.
Quit the previous app before upgrading. Upstream ZapFast data is never moved.
WhatsApp caches a linked device's display name at pairing, so an existing
link may keep its previous name on the phone; the account remains linked.
Connection-status logs record only the state name, omitting QR payloads,
pairing codes, phone numbers and server error details.
The device database and archived raw messages contain account and attachment
keys. Clearing the cache does not remove them; unlinking removes the local account.

## Developing

```sh
cargo run --locked --features demo -- --demo
cargo run --locked --features demo -- --demo-page login
cargo run --locked --features demo -- --demo-shot shot.png --demo-page chat,light
cargo run --locked --features demo -- --demo-tour
cargo run --locked --example background_probe
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets
cargo test --locked --all-targets --all-features
RUSTDOCFLAGS='-D warnings' cargo doc --locked --all-features --no-deps
```

The demo uses offline sample chats in a fresh temporary directory. It does not
open a linked account, connect to WhatsApp, or register a tray icon. Space starts
or replays its scripted tour. Demo screenshots verify layout, not live messaging.

GIF search uses a key entered in Settings or `WHATSAPP_GIPHY_KEY` at build time.
The source includes no key. `AGENTS.md` describes the architecture and invariants.
The repository contains only the Mac packaging and CI workflows. Icon sources
live in `assets/brand/` and are rebuilt with `cargo run --locked --example render_icon`.
Historical performance reports retain the product name used when measured.

## Disclaimer

Whatsapp is an unofficial client and is not affiliated with WhatsApp or
Meta. Using an unofficial client may be against WhatsApp's terms of service
and could get an account suspended. Use it at your own risk.

## License and credit

Whatsapp is independently maintained by Erlin Hoxha. It began from
[ZapFast by Carmine Paolino](https://github.com/crmne/zapfast) and retains the
original MIT copyright and license in [LICENSE](LICENSE). Inter and Noto
Color Emoji are under the SIL Open Font License; icons are from
[Lucide](https://lucide.dev) (ISC). The handset-and-chat artwork is maintained in this repository. WhatsApp is a trademark of Meta.
