# Whatsapp for iPhone

A native SwiftUI companion for a personal iPhone, backed by this repository's
existing Rust WhatsApp worker and SQLite archive. No web view, hosted backend,
telemetry or additional account service. Requires iOS 18 or later and an Apple
Silicon Mac to build. This is an initial personal-device build, not an App Store release.

## Included

- Link with a phone-number code, or scan a QR code from another primary phone.
- Native searchable chat list, archived chats, unread counts and profile pictures.
- Text messages and quoted replies, delivery status, reactions and emoji display.
- Page local history, then request older history from the primary phone.
- Download attachments; preview images and files that iOS Quick Look supports.
  Voice-note playback, recording, attachment sending, calls, rich WhatsApp text
  formatting, contact creation and group administration are not included yet.
- Local SQLite history and device keys in Application Support/Whatsapp, excluded
  from backup with iOS data protection. No desktop credentials are copied.

Messages arrive while the app is open. UIKit provides a short, bounded grace
period for active work when leaving it; the app then stops its connection and
reconnects when reopened. **No background notifications or always-on connection.**
No background audio workaround, background mode or remote notification server is used.
Removing the app removes its local history. Unlink this companion from the
official app's Linked Devices settings when finished using it.

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
official WhatsApp app. Open it and enter your number including country code.
In official WhatsApp, open Settings → Linked Devices → Link a Device → Link with
phone number instead. Approve this app's code, then return here to sync. Pairing
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
Swift unit tests cover replay deduplication, paging, live-vs-query events, identity
merges and load failures. Rust bridge tests validate commands and event fields.
The UI test launches a Debug-only `--demo` preview with fictional chats, exercises
the composer and saves a screenshot. It never links an account or sends a real
message. Simulator tests and a signed installation do not establish live account
pairing, sending, history sync or physical-device visual behavior; verify those
separately with the account owner.
