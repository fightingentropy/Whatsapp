# Selected ZapFast updates — 24 September 2026

Reviewed the [0.16 announcement](https://x.com/paolino/status/2103210559806017598),
the [0.14](https://github.com/crmne/zapfast/releases/tag/v0.14.0),
[0.15](https://github.com/crmne/zapfast/releases/tag/v0.15.0),
[0.16](https://github.com/crmne/zapfast/releases/tag/v0.16.0),
[0.16.1](https://github.com/crmne/zapfast/releases/tag/v0.16.1) and
[0.16.2](https://github.com/crmne/zapfast/releases/tag/v0.16.2) release notes,
and their implementation through upstream commit `a4d4622`.

## Included

- **Search the open chat, with a calendar day filter.** Adapted the behavior of
  [#149](https://github.com/crmne/zapfast/pull/149) and
  [#170](https://github.com/crmne/zapfast/pull/170) to this fork's indexed archive
  and cached conversation layout. Queries are debounced, replies carry a request
  number, and filtering precedes the 200-result limit. Date ranges use local
  midnight boundaries, including daylight-saving changes. The existing global
  search and its recent-message fast path remain available.
- **Native photo preview.** Ported
  [#115](https://github.com/crmne/zapfast/pull/115) and its later zoom corrections.
  Kept the external opener and protected the preview's image from cache eviction.
  Keyboard and clipboard input cannot reach the composer behind the preview.
  AppKit's Find and zoom commands follow the active page or preview too.
- **Fuller chat previews on hover.** Ported
  [3d06f7f](https://github.com/crmne/zapfast/commit/3d06f7f) from 0.16.2.
  Multiline and elided previews show a bounded tooltip with group sender names
  and the existing emoji/markup handling; typing rows and open menus suppress it.
  The retained preview text is capped at 2,000 characters plus an ellipsis so
  closed conversations do not retain another full copy of long messages.
- **Searchable Settings.** Adapted
  [e4ab365](https://github.com/crmne/zapfast/commit/e4ab365) to the fork's current
  settings. Search matches English labels, descriptions and section names,
  ignoring case. Command-F focuses it; Escape clears it before leaving.
- **Connection recovery.** Ported
  [8f43318](https://github.com/crmne/zapfast/commit/8f43318)'s sleep/silence watch
  and updated both Mac and iPhone to the protocol revision used by 0.16.2,
  `f7468ae2920a2b0aedbd0848cf121de56a07f93c`. This includes the library's
  [stalled-write keepalive fix](https://github.com/oxidezap/whatsapp-rust/pull/1547).
  Adopted upstream's device-store compatibility repair for the legacy
  `created_at` column, and updated group metadata/routing calls to the new API.
- **Playable audio attachments.** Ported
  [#162](https://github.com/crmne/zapfast/pull/162). WAV, FLAC, AIFF, WMA and other
  unsupported inline audio travel as documents; supported audio MIME types are
  normalized. This shared worker change also applies to the iPhone companion.

The upstream work is by Carmine Paolino, Fabio Motta, Lisandro Nahuel and the
contributors credited in those commits. Local adaptations preserve Whatsapp's
identity, M1 baseline, OpenGL/Metal choices, iPhone bridge, history cache, and
custom archive-pull behavior. No public release tag or marketing-version bump
accompanies these ports.

## Deferred or already covered differently

- Archive encryption and the new updater require deliberate migration and
  signing work for this fork and the separate iPhone container.
- Poll creation/voting, synchronized favorites, labels, profile/privacy editing,
  group membership actions and the redesigned sticker picker span protocol,
  archive, Mac UI and iPhone bridge models. They remain separate feature work.
- Inline video playback needs integration with this fork's bounded VideoToolbox
  decoder and audio lifecycle. Ordinary videos still open in the system player;
  upstream's 0.16.2 inline-video audio regression therefore does not apply here.
- Upstream's long-chat layout/cache rewrite overlaps this fork's existing
  measured-row and bounded-history work. It was not substituted wholesale.
- The rounded composer overlaps the custom Mac controls and recent iPhone
  composer work. Theme packs, localization, Linux/Windows packaging and their
  platform-specific fixes were not imported.
- Chat locks, synchronized deletion, richer receipt recovery, strict quote
  preservation and bidirectional-text changes warrant their own complete ports
  with migration/privacy and native-bridge tests; this batch does not claim them.

## Validation

All messaging tests and visual checks use local fixtures or `--demo`.
Regression coverage includes indexed scoped search, date boundaries, stale search
responses, search/preview keyboard behavior, preview routing and zoom, tooltip
content, sleep/silence detection, attachment MIME handling, and device-store
compatibility. The repository's full Rust checks cover the Mac app; shared-core
host tests and iOS target builds check the companion separately. Compilation is
not a claim of live delivery, physical-device testing or installation.

- All six required Rust formatting, Clippy, test and documentation commands pass.
  Default features pass 270 tests; all features pass 272. Seven opt-in tests
  remain ignored in each suite.
- Shared iPhone core host tests pass: 108 tests, with two opt-in tests ignored.
- Release builds of the shared core pass for `aarch64-apple-ios` and
  `aarch64-apple-ios-sim`. The Swift app was not run or installed on an iPhone.
- Offline OpenGL screenshots were inspected for chat search, the day filter,
  Settings search, and photo preview; coverage includes light/dark themes and
  the 720-by-480 minimum Mac window. Metal was compiled and unit-tested through
  the all-features checks, not run for these screenshots.
- The existing exact-layout regression test now waits for its asynchronous
  fixture photo to decode before comparing settled positions. Its equality
  assertion is unchanged.
