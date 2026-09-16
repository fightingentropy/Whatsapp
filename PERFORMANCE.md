# Apple Silicon performance work

## Rendering and video: 16 September 2026

Apple M4 Pro, macOS 27.0 (26A428), Rust 1.98.0, release profile. Raw samples are
in [the media measurement file](benchmarks/apple-silicon-media-2026-09-16.json).
All input was synthetic or offline demo data. No linked account was used.

### H.264 preview decoding

Each clip contains 60 frames of FFmpeg's `testsrc2` pattern at 30 fps, encoded
with libx264 (`-preset fast -pix_fmt yuv420p -bf 2 -g 30`). Timings include MP4
reading, session creation, decoding, scaling and CPU RGBA output, excluding
texture upload. Each backend gets one warmup and five measured decodes in
alternating order; these are medians, with a new decoder session every time.
The probe requires a hardware session and cannot silently substitute software.

- 320×180: OpenH264 **17.69 ms**, VideoToolbox **21.88 ms**.
- 640×360: OpenH264 **93.54 ms**, VideoToolbox **31.79 ms** (2.9× faster).
- 360×640: OpenH264 **94.95 ms**, VideoToolbox **30.88 ms** (3.1× faster).
- 1280×720: OpenH264 **317.53 ms**, VideoToolbox **48.96 ms** (6.5× faster).
- 1920×1080: OpenH264 **665.37 ms**, VideoToolbox **92.13 ms** (7.2× faster).

The default therefore uses hardware for clips with at least 230,400 source
pixels (640×360, regardless of orientation) and eight frames. This is a
conservative policy, not a universal crossover point for every M-series chip.
Small/short clips stay on OpenH264. Any native failure retries software, then
the existing ffmpeg fallback for unsupported input. Hardware output is requested
at preview size, submissions are drained in batches of four, and frames are
sorted by presentation time. Both paths retain the 150-frame preview limit.

Tests found and fixed an existing software end-of-stream bug that lost two
delayed B frames. Two committed colour fixtures verify all 12 frames, their
order, duration, size and final colours, exercising both the automatic path and
software independently. Native and software output are not pixel-identical:
VideoToolbox uses system colour conversion/scaling; the test-pattern RGB mean
absolute differences were 3.3–10.2 out of 255. Both first frames were inspected.
GIF/WebP decoders are unchanged; ordinary videos still open externally. This
is not streaming playback, zero-copy rendering or an energy measurement.

### Metal comparison

The optional `metal` feature enables only wgpu's Apple backend. Five runs per
renderer used the same release/demo binary, alternating renderer order, with
two seconds of warmup then 240 continuous repaint frames. This is a fixed-chat
stress test, not typical idle use. Metrics below are medians across runs.
CPU time is eframe's UI/render submission time, excluding vsync waiting; it is
not a GPU timestamp. The log confirmed **Metal / Apple M4 Pro**.

- CPU per frame: OpenGL **0.357 ms**, Metal **0.898 ms**.
- 95th-percentile CPU per frame: OpenGL **0.438 ms**, Metal **1.448 ms**.
- Peak resident memory: OpenGL **447.5 MiB**, Metal **428.9 MiB**.
- First UI callback: OpenGL **235.4 ms**, Metal **228.3 ms**; this is not the
  first screen presentation or a cold filesystem-cache measurement.

Metal varied between runs and did not improve CPU frame time in this scene.
**OpenGL remains the default**, with no wgpu dependency in the normal build.
Metal stays opt-in for broader testing. Demo memory includes bundled sample
fonts and must not be presented as the normal release app's footprint. No
battery or whole-app speed claim follows from these measurements.

### Validation for this batch

- Default features: **186 tests passed**, 7 ignored.
- All features: **188 tests passed** (187 library + 1 binary), 7 ignored.
- Formatting, both Clippy variants with warnings denied, and rustdoc passed.
- Real native Metal and OpenGL demo windows rendered successfully on the M4 Pro.
  Hardware decoding was executed locally; hardware availability is not assumed
  for hosted CI, which can exercise the software fallback.
- No live pairing, message delivery, microphone, or notification tests were run.

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

### Validation on 15 September

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

- Broaden Metal measurements to scrolling, multiple media previews and energy
  use on other M-series chips before considering a default change.
- Test VideoToolbox with representative user-supplied clips and varied colour
  metadata; the current measurements use synthetic H.264 only.
- Stream a bounded queue of decoded frames, and measure memory pressure with
  several animated stickers visible at once.
- Virtualize conversation layout while preserving cross-message selection,
  scroll anchors and transcript copying.
- Profile remaining chat sorting, font discovery and background maintenance
  before adding more caches or changing protocol timing.

Platform decisions follow [Rust's Apple target documentation](https://doc.rust-lang.org/rustc/platform-support/apple-darwin.html),
[SQLite's trigram tokenizer documentation](https://www.sqlite.org/fts5.html#the_trigram_tokenizer)
and [GitHub's ARM64 runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).
