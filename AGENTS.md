# Whatsapp agent guide

Whatsapp is an independent Apple Silicon Mac client derived from ZapFast: Rust, egui, and the
[whatsapp-rust](https://github.com/oxidezap/whatsapp-rust) library for the
protocol. These notes are for coding agents and new contributors.

`ios/` adds a personal iPhone companion in SwiftUI, using the same Rust backend
and archive. This is an explicit additional target; keep the Mac build intact.

## Product boundaries

- Keep it a small native client. No browser engine, no telemetry, no
  hosted backend, no second account system.
- The protocol comes from whatsapp-rust. Do not reimplement pieces of it
  here, and do not advertise a capability merely because a protobuf field
  for it exists.
- Do not broaden a task into adjacent features or a general refactor.
  Preserve existing user behaviour unless the task changes it.

## Architecture

- `ios/rust` compiles the shared worker, archive, models and codecs into a static
  library, exposing only validated UI commands and versioned JSON events through
  `CoreBridge.h`. Native sources are reused by path, not copied. The iOS Waker
  schedules coalesced main-queue drains. All FFI calls run on one serial engine
  queue; shutdown must never block Swift's main actor. Pairing payloads and event
  JSON must never be logged. iOS uses a separate container and linked session,
  with no Mac migration, telemetry, browser or push relay. A bounded UIKit
  background task finishes active work before stopping the connection; foreground
  activation restarts it. Do not claim background delivery. Test with `--demo`
  and fixtures, never send live messages as an automated check.

- The iPhone uses Observation for property-scoped view updates. Recent chat
  snapshots are bounded to four chats / approximately 16 MiB and invalidated on
  inactive message mutations, identity merges, worker restart and memory pressure.
  Active history uses bidirectional SQLite windows (600 messages / approximately
  8 MiB), protecting visible rows and selection/quote/edit/in-flight operations.
  iPhone windows order equal timestamps by message ID in both Swift and SQLite;
  desktop paging retains its existing ordering. Quote jumps replace the window
  around their target. Never join a live message onto an older window across an
  unloaded gap. Attachment paths are repaired on access, not by a startup scan.
  Animated sticker reservations share a 32 MiB cap; pause/release them for
  backgrounding, Low Power Mode and Reduce Motion.
- `src/ui/` draws views and pushes `model::Action`s; `src/app.rs` applies
  them after the frame. Never mutate application state from inside a view
  beyond the view's own fields (composer text, search text, flags).
- `src/backend.rs` is the interface's handle to a tokio runtime on its own
  thread; `src/backend/worker.rs` runs there. It owns the whatsapp-rust
  `Bot`, the message archive, downloads, and profile pictures. The two
  sides talk only through `Command` (interface to runtime) and `Event`
  (runtime to interface); every event wakes the window through `Waker`.
- `src/archive.rs` is the SQLite store of chats, messages, contacts, and
  privacy-id mappings. WhatsApp replays history once, at link time, so the
  archive is the only copy. It keeps each message's raw protobuf because
  the keys to fetch an attachment live in it.
- `src/app/cache.rs` limits inactive conversation payloads to eight chats and
  approximately 32 MiB of owned messages/layouts. The selected chat, dialog source
  and active local/phone loads, sends and downloads are protected. Eviction keeps
  lightweight phone-history exhaustion/backoff state and drafts, resets local
  paging, and releases egui thumbnails and unshared file images without deleting
  files. Unopened/evicted chats do not absorb background message payloads; the
  worker has already archived them. `Event::Messages.requested` distinguishes
  local query replies from live/phone events, so a live message cannot complete
  an outstanding first-page query. Local read failures clear their loading flags
  and allow reopening/reconnection to retry. `Conversation::message_mut` also
  invalidates cached memory size and pending-operation state.
- `src/model.rs` holds the app's own types. Views never touch a protobuf;
  the worker translates in `classify()` and `parse_conversation()`.
- Chat ids are canonical strings: a chat behind a privacy id (`@lid`) is
  filed under its phone number once the mapping is known. Use
  `Worker::canonical` for anything that arrives as a `Jid`. Learning a mapping
  transactionally merges existing chat/message/contact rows; startup also repairs
  aliases left by older builds. `Event::ChatMerged` moves open UI state and drafts.
  Queued commands and asynchronous results must resolve their ids again before
  touching the archive, since a mapping can arrive while they are in flight.
- `src/theme.rs` owns colours, fonts, and icons; `src/ui/widgets.rs` the
  shared controls. New icons go in `assets/icons/` as 24px Lucide-style SVGs
  and in the `icons!` table.
- `src/markup.rs` turns WhatsApp's text markup, links, and mentions into an
  egui `LayoutJob`; `src/emoji.rs` swaps every emoji for a placeholder
  glyph at layout time and paints the desktop's colour emoji bitmap over
  it afterwards (resolving sequences through the font's GSUB ligatures).
  Any text that can hold an emoji goes through `widgets::line` /
  `widgets::rich_text` or `markup::layout`, never a bare `Label`.
- `src/animation.rs` plays animated stickers and GIFs. WebP/GIF decode
  in-process; `mp4` demuxes H.264 previews for VideoToolbox hardware decoding
  with OpenH264 as the software fallback (AVCC converted to Annex B).
  VideoToolbox requests preview-sized output, limits in-flight submissions to
  four and sorts output by presentation time. Both paths flush delayed frames.
  `ffmpeg` is only a fallback for other codecs. `openh264` compiles
  its C++ from source with the C++ compiler of the host; `nasm` is
  optional and only adds the SIMD paths (the AUR recipes leave it out,
  the build works without it). Frames share one playback texture per clip on the interface
  thread. The CPU frame cache is bounded by bytes and expires unseen clips.
  Decoders publish ordered frames through an eight-frame mailbox with backpressure.
  The worker blocks outside native callbacks; dropping the receiver cancels work.
  A stalled mailbox times out so an abandoned egui context cannot hold a decoder
  slot forever. Pending streams hold their last frame until more arrive, then
  loop only after EOF. A failed backend resets published frames before fallback.
- `src/ui/conversation/rows.rs` caches exact settled row heights. Invalidate a
  message and its next neighbour when changing content/grouping; use
  `Conversation::message_mut` rather than mutating an existing message directly.
  Contact/name changes invalidate layouts too. Unsettled image rows keep measuring.
  Explicit per-message UI IDs survive prepended history. During selection, an
  offscreen bubble can skip layout only if its cached glyph geometry and transcript
  are registered in full and in order. This optional cache has a 16 MiB per-chat
  budget included in inactive-history accounting; misses use full layout. Cached
  galleys have no paint meshes and must never render visible text. Preserve the
  first visible row when earlier content changes height. Never truncate the rows
  supplied to egui during cross-message selection or copy.
- `src/renderer.rs` configures the native renderer. OpenGL remains the default;
  `--features metal` adds a Metal-only wgpu backend selected with `--renderer metal`.
  Keep the OpenGL option for comparison/recovery. Both are event-driven; the
  offline demo benchmark measures CPU frame time, not GPU time or battery life.
- `src/video.rs` plays one ordinary video through AVFoundation on the main thread.
  Audio and seeking share its native clock; egui draws one current texture bounded
  to 640 pixels per side, honoring the track rotation/mirroring. No whole-movie
  frame cache or native overlay window. Paused players stop scheduling repaints;
  scrolling away pauses, while leaving the chat or hiding the window releases it.
  Only an explicit Play can autoplay after a download. Test with synthetic media
  and `inline_video_probe`, including decoded pixels and the audio track.
- Message bodies paint through `markup::paint_selectable` and single lines
  through `widgets::selectable_rich_text`: both hand the galley to
  `egui::text_selection::LabelSelectionState` (which paints it) and only
  overlay the colour emoji, so text can be swept and copied while
  `style.interaction.selectable_labels` stays false for every other label.
  The response must sense clicks and drags. `SelectionLeash` (an egui
  `input_hook` plugin) clamps a drag that started in the message view to
  just inside its edge once the pointer strays out (the platform keeps
  reporting a grabbed pointer beyond the window), and drops mid-drag
  `PointerGone`, so the selection keeps a row under it while the edge
  scroll brings more past. A copy that sweeps across
  messages is rebuilt by `src/transcript.rs` with `[time, date] Name:`
  per message (the phone's sharing format): every drawn body lands in
  `App::copy_rows` each frame, and the `CopyAnnotator` egui plugin
  rewrites the queued `CopyText` in `output_hook`, the only hook that
  runs after the selection plugin's own end-of-pass flush (plugins run
  in registration order and the built-ins come first, so end-pass
  callbacks fire too early).
  Selection galleys share the message viewport's horizontal bounds while
  retaining their glyph positions: otherwise egui considers short incoming
  and outgoing messages separate columns and will not sweep across them.
- Group names and members come from `groups().get_metadata`, asked one
  turn at a time (two per 5 s tick, `pump_group_info`): dozens of unnamed
  groups arrive with history sync and a burst of queries hits the
  server's rate limit, which once left groups called "Group" forever.
  Failures back off (30 s doubling, seven tries); item-not-found,
  forbidden and not-authorized are final and stop the asking.
- A download that answers 403/404/410 goes through
  `client.media_reupload().request(..)` (a server-error receipt; WhatsApp
  has the phone re-upload and answers with a fresh `direct_path`) and is
  fetched once more before the bubble reports "No longer on WhatsApp's
  servers". Download failures never toast; they live in the bubble as
  "... · click to retry". Copied text is refined by
  `transcript::refine`: emoji placeholders map back through each row's
  `placements`.
- History sync can bring a chat with a name and no messages at all; a
  history request for such a chat is anchored at the present with an
  empty message id (`worker::fetch_older`), and the app asks the phone
  as soon as such a chat loads or opens, instead of never.
- `eframe`'s `glow_options` turn vsync off: a Wayland compositor stops
  sending frame callbacks to a window on a hidden workspace, a vsync wait
  there blocks the event loop and its ping replies, and Hyprland then
  calls the app unresponsive. Repaints are event-driven, so nothing spins.
- `src/voice.rs` is the codec for voice messages: OGG/Opus in and out
  (the `ogg` crate for the container, `opus` with libopus bundled and
  built by cmake for the codec, so cmake is a build dependency), plus
  the 64-bar waveform WhatsApp draws and a mono/48 kHz resampler.
  `src/audio.rs` is the sound: `Player` plays one clip at a time through
  rodio (OGG/Opus through `voice`, MP3/M4A/WAV through rodio's decoders,
  decoded on a thread, the device opened on demand and released when the
  clip ends) and `Recorder` reads the default microphone through rodio's
  `Microphone` on a thread, keeping a loudness per 50 ms for the live bars.
  Linux needs ALSA headers to build (`libasound2-dev` on Debian,
  `alsa-lib` on Arch). `Action::PlayVoice/SeekVoice` drive the player from
  the bubble; `StartRecording/CancelRecording/SendRecording` the
  microphone from the composer (the send button is a microphone when there
  is nothing to send); `Command::SendVoice` normalizes
  (`voice::normalize`, quiet takes up to just under full scale, gain
  capped), encodes and sends push-to-talk with the waveform and the reply
  quote if one was open; `Command::MarkPlayed` sends the played receipt
  once per incoming voice message. Own bubbles lay out right-aligned,
  where egui turns `ui.horizontal` right to left: rows like the voice
  player must use an explicit `Layout::left_to_right` at their own width.
  `src/ui/picker.rs` is the emoji/GIF/sticker panel. GIF search uses the
  key from Settings, else one baked in at build time from
  `WHATSAPP_GIPHY_KEY` (`option_env!`); the repository carries none. The
  phone's recently used stickers arrive in `HistorySync.recent_stickers`
  when the device links and live in the archive's `stickers` table as raw
  `StickerMetadata`, fetched when the picker opens; favourite stickers sync
  through app state (`FavoriteSticker`), which whatsapp-rust does not
  surface, so they are not shown.
- Whatsapp uses `org.erlin.whatsapp` storage and bundle identity and the
  fightingentropy/Whatsapp release endpoint. Startup migrates only our previous
  `org.erlin.zapfast-silicon` profile after acquiring the single-instance guard;
  move whole directories with SQLite journals, never merge or overwrite an
  existing destination, and stop on errors. Never adopt an upstream session.
  The guard also understands our previous `zapfast-silicon:` protocol so the
  two app versions cannot open the same linked account during an upgrade.
- `assets/brand/whatsapp-mark.svg` owns the handset-and-chat mark. Run
  `cargo run --locked --example render_icon` to regenerate the committed dark
  macOS SVG and PNG. The menu-bar template uses the transparent mark directly.
- The app outlives the window, as in Spotifast: `main` runs
  `eframe::run_native` in a loop; closing the window with "keep running"
  on sets `hide_intent`, the window is destroyed, and a headless loop keeps
  calling `App::background_frame` (the link, the archive, the tray) until
  the tray, a clicked notification, or another launch sets `wants_show`,
  when a new window is made. `src/tray.rs` is the Linux status notifier
  (ksni), `src/tray_native.rs` the Windows and macOS item (tray-icon; on
  macOS made with the first window and pumped by `tray::idle` while none
  exists). `src/single_instance.rs` holds a loopback port so a second
  launch surfaces the first. `src/notify.rs` sends desktop notifications
  for `Event::Incoming` (live messages from others, not history) when the
  reader is away from that chat. macOS has no title bar: the content runs
  to the top. `src/macos.rs` keeps native application menus alive across window
  recreation and aligns traffic lights with the chat header. Linking retains
  `ui::titlebar_strip`; other headers reserve horizontal space for the buttons.
- Group delivery uses `archive::receipts`: save the recipients when filing an
  outgoing message, record each person's receipt, then take the least advanced
  recipient. Never promote a group from one reader, apply a receipt to earlier
  messages, or infer a historical audience from current membership. History
  trusts the phone's aggregate status, not a partial `user_receipt` list.
- The name and icon under the phone's Linked devices come from
  `DevicePropsOverride` in `start_bot` (`os` is the name shown, the
  platform type picks the icon); WhatsApp reads them at pairing only, so a
  change shows after unlinking and linking again.
- QR exhaustion with `disconnected == true` ends whatsapp-rust's client
  permanently. Restart the bot over the same device store; reconnecting that
  client cannot revive it. Scope callbacks and queued pairing work to the bot
  generation, and preserve an outstanding phone-code flow when exhaustion leaves
  its socket open. `LinkStatus`'s debug formatter deliberately omits all payloads
  because status transitions are logged at info level.
- Older history comes from the phone on demand (`Command::FetchOlder` →
  `Client::fetch_message_history` → a `HistorySync` chunk with
  `sync_type == ON_DEMAND`); the archive is paged first, the phone only
  when it is exhausted.
- The desktop package targets `aarch64-apple-darwin` only, including CI and packaging.
  Keep the M1 baseline; do not use `target-cpu=native` for distributable builds.
  Legacy platform branches can remain for upstream comparison, but they are not
  supported desktop build targets. The separate `ios/rust` package adds
  `aarch64-apple-ios` and `aarch64-apple-ios-sim`; do not make the egui app an iOS target.

Three egui pitfalls this code has already hit:

- `consume_key(Modifiers::NONE, key)` also matches the key with Shift held
  (egui only insists on the modifiers you ask for), so the composer
  inspects the events itself to tell Enter from Shift+Enter.
- `with_layout(..., Align::Center)` directly inside a vertical container
  claims the whole available height; wrap it in `ui.horizontal`.
- `ui.horizontal` inside a right-aligned bubble lays out right to left;
  see `mirrored_row`. A bubble's own click target is registered before its
  contents (from last frame's rect) so links and quotes inside win clicks.
- `Popup::context_menu` opens on the *response's* right-click, which those
  inner widgets take for themselves; the bubble reads the right-click from
  the input over its own rect and opens `Popup::menu` itself, so the menu
  comes up anywhere on the message.

## Releasing

Do not cut a release for every fix. Work accumulates on `main` until
there is something substantial to announce: a feature, or a batch of
fixes worth a changelog entry. Five patch releases in a day is what this
rule exists to prevent. The exception is a regression in something just
released, which goes out as soon as it is fixed.

A release is not finished when the tag is pushed. Do these in order:

1. Bump `version` in `Cargo.toml` and update `Cargo.lock` with a build. Run
   the full checks, commit, and push before tagging so the binaries report
   the right version.
2. Tag `vX.Y.Z` and push the tag. Wait for the Apple Silicon DMG and
   `checksums.txt`; verify signing and notarization, not just compilation.
3. Write release notes about the final user-visible behavior and validation.
   Publish only to fightingentropy/Whatsapp. Do not publish the inherited website,
   upstream packages, AUR or Homebrew repositories from this fork.

## Definition of done

- Add focused tests for changed behaviour. The `demo` feature carries sample
  data and a headless layout test of every screen (`src/demo.rs`); extend
  the sample when a new kind of content or state is added, and use
  `--demo-shot` to look at the result.
- Update the README when user-visible behaviour, settings, files, or network
  access changes.
- Run the full checks before finishing:

  ```sh
  cargo fmt --all --check
  cargo clippy --locked --all-targets -- -D warnings
  cargo clippy --locked --all-targets --all-features -- -D warnings
  cargo test --locked --all-targets
  cargo test --locked --all-targets --all-features
  RUSTDOCFLAGS='-D warnings' cargo doc --locked --all-features --no-deps
  ```

  Do not weaken a lint, delete a test, or add an `allow` merely to make
  them pass without explaining why the rule does not apply.
- Report platform coverage honestly: say what was run and what was only
  compiled.
- Never log message contents, phone numbers, keys, or QR payloads at a
  level that ships. The log file is meant to be attached to bug reports.
