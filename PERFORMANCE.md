# Apple Silicon performance work

## Measured on 15 September 2026

Apple M4 Pro, macOS 27.0 (26A428), Rust 1.98.0 (88d9e12ae), bundled SQLite 3.53.2.
The compiler, Cargo, Clippy and rustdoc were selected from the same rustup toolchain.
These are local synthetic benchmarks, not a linked WhatsApp account.
Raw results are in [the measurement file](benchmarks/apple-silicon-2026-09-15.json).

### Search

100,000 text messages across 100 chats; 50-result limit; median of 21 warmed
runs with alternating query order. The baseline uses the original six-field
JSON/LIKE expression and forces its previous full-scan access pattern. Both
sides return the same ordered IDs and are also checked against the public
Archive API. Timings cover SQL and ID collection, excluding UI rendering,
message deserialization, the 180 ms typing delay and network traffic.

- Rare `parcelreference`: **77.111 ms → 0.740 ms**.
- Missing `no-such-phrase`: **79.629 ms → 0.367 ms**.
- Common `ordinary`: **84.127 ms → 0.318 ms**.
- Short `or`: **83.017 ms → 0.110 ms**.

The implementation combines an FTS5 trigram index with a global timestamp
index. For a normal page of results, it first checks at most the newest 256
messages. A complete page there is already the correct answer; otherwise it
uses FTS across the whole archive. This avoids the common-term regression
found during the initial benchmark. Queries shorter than three characters
retain the LIKE fallback.

Performance depends on history size, message lengths, term distribution and
storage. The search index adds disk space and a one-time migration/rebuild cost.
The benchmark runs in the development profile with dependencies optimized at
level 2. It does not measure whole-app speed or predict every user's latency.

### History writes

10,000 messages, 100 existing chats, 256 raw bytes per message, file-backed WAL
with synchronous NORMAL. Median of three runs using the same current indexed
schema and the public Archive API on both sides:

- Individual `insert_message` calls: **1,054.161 ms**.
- `insert_messages` in batches of 256: **363.458 ms** (**2.90× faster**).

This isolates transaction batching and per-chat activity updates. It is not an
end-to-end phone history-sync benchmark or a comparison with upstream's
unindexed write schema. The transaction test checks rollback, delivery state,
raw attachment metadata and chat activity.

### Background and animation checks

The closed-window loop no longer wakes every 150 ms. A main-thread
CoreFoundation probe verified a worker wake after **33.04 ms**, including the
worker's deliberate 30 ms delay, survived 100 signal/wait races and respected
an idle deadline. This proves the wake mechanism; it is not a measured battery
percentage. Protocol maintenance and network timers still run.

Animation tests verify that advancing a frame reuses its texture, a repaint
within the same frame produces no new upload, tall/wide previews remain within
320 pixels, and decoded images are retired after leaving a media view. The
128 MiB limit covers retained decoded pixels, not all process or GPU memory.
The implementation still decodes complete bounded clips before playback.

## Validation

- Default features: **182 tests passed**, 7 intentionally ignored.
- All features: **183 tests passed** (182 library + 1 binary), 7 ignored.
- Formatting, both Clippy variants with warnings denied, and rustdoc passed.
- Real native ARM64 demo window opened and saved a screenshot; layout and
  color emoji were visually checked using offline sample chats.
- Mac bundle scripts, plist metadata and the native-packages configuration
  validated. A local release DMG was built, mounted read-only, and its app's
  ad-hoc signature, ARM64 architecture, microphone metadata and binary hash
  verified. The release executable before bundle signing is 28,315,312 bytes (27.00 MiB), with macOS 11.0 recorded
  as its minimum deployment version. This build is not notarized.
- Live account linking, message delivery, physical microphone recording,
  notifications and Developer ID notarization were not tested in this run.

Reproduce the measurements and native wake test:

```sh
cargo run --locked --example archive_benchmark
cargo run --locked --example background_probe
```

## Remaining experiments

- Measure a Metal/wgpu backend against glow for frame time, startup, memory
  and energy before changing the default renderer.
- Prototype VideoToolbox for supported video codecs; GIF and WebP need their
  existing image decoders. Compare CPU, latency and copies using real clips.
- Stream a bounded queue of decoded frames, and measure memory pressure with
  several animated stickers visible at once.
- Virtualize conversation layout while preserving cross-message selection,
  scroll anchors and transcript copying.
- Profile remaining chat sorting, font discovery and background maintenance
  before adding more caches or changing protocol timing.

Platform decisions follow [Rust's Apple target documentation](https://doc.rust-lang.org/rustc/platform-support/apple-darwin.html),
[SQLite's trigram tokenizer documentation](https://www.sqlite.org/fts5.html#the_trigram_tokenizer)
and [GitHub's ARM64 runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).
