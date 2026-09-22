# Whatsapp for iPhone

A native SwiftUI companion for a personal iPhone, backed by this repository's
existing Rust WhatsApp worker and SQLite archive. No web view, hosted backend,
telemetry or additional account service. Requires iOS 18 or later and an Apple
Silicon Mac to build. This is a personal-device build, not an App Store release.

## Included

- Link with a phone-number code, or scan a QR code from another primary phone.
- Native chat list, unread counts, profile pictures, pinning, archive/unarchive,
  mute durations and mark-as-read. Pull down the chat list to reveal Archived.
- Compact, content-sized message bubbles group consecutive messages from the
  same sender within five minutes. Sender names start a group, portraits finish
  it, and day changes always separate groups. Reaction pills and unframed stickers
  keep the conversation light. Chat dates distinguish today, yesterday and older
  conversations; both light and dark appearances use matching native controls.
- Search chat names and the downloaded message archive; open a result or quoted
  reply at its original message, loading earlier local history as needed.
- Text messages, quoted replies, group mentions and typing/presence indicators.
  Long-press a message to react, edit, forward, copy, inspect delivery/reaction
  details, select several messages for a dated transcript, or delete. Editing
  and deletion for everyone use the same time windows as desktop. Deletion asks
  for confirmation; forwarding requires choosing a destination and pressing Forward.
- Bold, italic, strike-through, monospace, lists, quotes, links and resolved
  mentions. Native previews for links, locations, contact cards and read-only polls.
- Photos, videos and files from the system pickers, plus explicit Paste photo,
  with captions. Up to 30 files per send, each up to 100 MB. Adding attachments
  clears a quoted reply: the shared file sender does not support quoted attachments.
- Voice recording, playback, seeking, waveform and played receipts. Recordings
  and OGG/Opus playback are limited to ten minutes to bound mobile memory use.
  Leaving the app pauses playback and stops the microphone, retaining an unsent
  recording in its original chat until it is sent or discarded. Recording asks
  for microphone permission only when you tap the microphone.
- The desktop's 1,914-entry emoji catalog, searchable names/shortcodes, recent
  emoji, `:shortcode:` expansion and `@` group-member suggestions. Command-Return
  sends from a hardware keyboard; the iPhone Return key inserts a new line.
- GIF search and sending through GIPHY with your API key in Settings. Saved and
  recent stickers, animated stickers/GIFs, and pack imports from signal.art links
  or .wastickers/ZIP files. Imported pack deletion asks for confirmation.
- New conversations by phone number, synced contact names, optional saving to the
  primary phone's address book, full profile pictures and group member details.
- Page local history, then request older history from the primary phone.
- Download and share attachments; preview images and files supported by iOS Quick
  Look. Visible attachments up to 64 MB can download automatically. Failed
  downloads remain in their message with a retry action.
- Dark/light/system appearance, message text size, sender pictures, contact-name
  preference, read receipts, typing indicators, automatic downloads and optional
  local notifications. Read and played receipts respect the shared privacy rules.
  Settings groups these controls under Appearance, Privacy & contacts, Media &
  downloads and Notifications. Appearance includes a live message-size preview.
- Local SQLite history and device keys in Application Support/Whatsapp, excluded
  from backup with iOS data protection. No desktop credentials are copied.
- Reconnect and confirmed unlink controls. Calls, status posts, group
  administration and poll voting are unsupported, as on the desktop client.

Messages arrive while the app is open. UIKit provides a short, bounded grace
period for active work when leaving it; the app then stops its connection and
reconnects when reopened. Optional local notifications work only while the
connection is running, including that brief allowance; muted chats do not notify.
**No push delivery or always-on background connection.** Desktop tray behavior,
desktop window shortcuts and the Mac updater do not apply to iOS; install a new
signed build to update this personal companion.
The app requests that allowance before leaving the foreground and watches the
remaining time, reserving time to close the connection safely. An interrupted
pairing clears its expired code and explains how to retry. The last 32 connection
and lifecycle stages are saved locally in app preferences for troubleshooting;
they contain timestamps and background time only, never codes, phone numbers,
account identifiers, messages or error payloads. They are not transmitted.
No background audio workaround, background mode or remote notification server is used.
Removing the app removes its local history. Unlink this companion in Settings or
from the official app's Linked Devices settings when finished using it.

GIPHY searches and previews contact GIPHY only when that picker is used; importing
a signal.art link contacts Signal's pack service. The API key and UI preferences
remain in app preferences. Attachment copies, decoded audio previews and imported
stickers stay in the private app container. Drafts and unsent recording controls
are held for this app session; they are not a durable offline outbox. A queued
message is not proof of server delivery: check its delivery indicator.

## Build and install

Install Xcode, Rust via rustup, CMake, Ninja and XcodeGen. The Rust version is
pinned by the root toolchain file; all dependencies share the root Cargo.lock.

```sh
rustup target add --toolchain 1.98.0 aarch64-apple-ios aarch64-apple-ios-sim
xcodegen generate --spec ios/project.yml
xcodebuild -project ios/Whatsapp.xcodeproj -scheme Whatsapp \
  -configuration Debug -destination 'platform=iOS Simulator,name=iPhone 17 Pro' \
  -derivedDataPath ios/DerivedData build
```

The Xcode prebuild phase calls `ios/scripts/build-rust.sh` and links the optimized
static library. It uses ARM64 for both device and simulator. The generated Xcode
project, `build/` and `DerivedData/` are ignored; `project.yml` is the source of truth.

For a physical iPhone, select your own Apple development team in `project.yml`,
connect/unlock the device, and build with automatic provisioning:

```sh
xcodebuild -project ios/Whatsapp.xcodeproj -scheme Whatsapp \
  -configuration Release -destination 'generic/platform=iOS' \
  -derivedDataPath ios/DerivedData -allowProvisioningUpdates build
xcrun devicectl device install app --device YOUR_DEVICE_UDID \
  ios/DerivedData/Build/Products/Release-iphoneos/Whatsapp.app
```

Bundle identity is `org.erlin.whatsapp.ios`, independent of the Mac app and the
official WhatsApp app. First, in official WhatsApp, open Settings → Linked
Devices → Link a Device → Link with phone number instead. Return to this app,
enter your number including country code, and copy the new code. Switch back to
official WhatsApp to submit it, then return here straight away to finish linking. Pairing
needs to finish within iOS's background allowance when using the same phone;
request a fresh code if it expires. The linked device is named “Whatsapp for iPhone”.

## Validation

```sh
cargo clippy --locked -p whatsapp-ios-core --all-targets -- -D warnings
cargo test --locked -p whatsapp-ios-core
cargo clippy --locked -p whatsapp-ios-core --target aarch64-apple-ios-sim -- -D warnings
xcodebuild -project ios/Whatsapp.xcodeproj -scheme Whatsapp \
  -destination 'platform=iOS Simulator,name=iPhone 17 Pro' \
  -derivedDataPath ios/DerivedData test
```

Also run all six root Mac checks in `AGENTS.md` after shared-source changes.
Swift unit tests cover message actions, mention boundaries, archive search/jumps,
formatting, emoji lookup, attachment limits, drafts, identity merges, receipt
privacy, cancelled audio, logout, paging, sender/day grouping, relative dates and
lifecycle/pairing recovery. Rust tests
check command validation, symlink/file boundaries, GIF hosts, sticker deletion
scope, finite voice samples, bounded Opus decoding and native WAV conversion.
UI tests launch a Debug-only `--demo` preview with fictional chats and rich-content
fixtures, exercise editing/forwarding, the composer, emoji search, group details
and interrupted pairing, check the latest message remains visible on opening a
chat, and save dark/light screenshots including large text and multiline drafts.
They never link an account or send a real
message. Simulator tests and a signed installation do not establish live account
pairing, sending, history sync or physical-device visual behavior; verify those
separately with the account owner.

Regenerate the bundled emoji catalog after changing the desktop emoji dependency:

```sh
cargo run --locked --example export_ios_emoji > ios/Whatsapp/emoji_catalog.json
```
