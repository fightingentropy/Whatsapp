# Apple Silicon performance work

The project is now named **Whatsapp**. Historical runs below retain the name,
commit and artifact paths used at measurement time.

## Idle conversation repainting: 20 September 2026

Keeping a conversation at its newest message repeatedly requested scrolling
64 points beyond the end. egui clamped the position but scheduled another
repaint, even when the content had stopped changing. The view now uses the
scroll area's exact maximum offset and requests a frame only when the offset
used to draw the view changes. New messages and expanding media still keep the
view pinned; scrolling upward releases it as before.

On the Apple M4 Pro, macOS 27.0 (26A428), release/M1 baseline, the same visible
chat was sampled without input for 15 seconds before and after a signed local
update. CPU usage was **83.13% of one core before**, then **0.36% and 0.02%** in
two consecutive samples after. Interrupt wakeups fell from **836/s** to
**1.27/s and 1.20/s**. An earlier pre-fix sample measured 79.08% CPU. No compiler
ran during the measurements. The installed app was also checked for scrolling
up and returning to the newest message; a further 15-second sample with the
window focused measured 0.01% of one core.

These are short observations of one chat, not a battery-life or whole-app
speedup claim. Sequential offline launches produced inconsistent baseline
results when window visibility was uncontrolled. A native scrolling probe
completed for the baseline but timed out for the fixed build in that desktop
session; there is no valid startup/scrolling timing comparison from this run.
All resource samples, including those inconsistent trials, are retained in
[the idle measurement file](benchmarks/apple-silicon-idle-2026-09-20.json).

The regression test settles six static screens at normal and fractional zoom
with one or two native pixels per point, then checks that none requests an
immediate repaint. Existing tests cover expanding images, cached/full geometry,
history anchors, wheel scrolling and cross-message selection. All six required
checks passed on Apple Silicon with pinned Rust 1.98.0: default features passed
236 tests, all features 238, with seven explicit skips in each suite.

For repeatable resource sampling of an already running process:

```sh
python3 scripts/process-usage.py PID --seconds 15 > usage.json
```

The sampler reads `proc_pid_rusage` counters and converts CPU time with
`mach_timebase_info`; these counters are Mach ticks, not nanoseconds. CPU usage
is expressed relative to one core. It does not measure GPU time or energy.
Keep the window visible, allow it to settle and avoid compiling during samples.

## Inactive conversation memory: 16 September 2026

Inactive histories now retain at most eight chats within an estimated 32 MiB
message/layout budget, evicting the least recently visited first. The open chat,
dialog source and active local/phone loads, sends and downloads are protected
and can exceed the budget. Eviction releases message vectors, row measurements,
registered thumbnails and unshared file-image caches. Downloaded files and
SQLite history remain on disk; drafts and phone-history exhaustion/backoff stay
in memory. Reopening an evicted chat loads a fresh page from SQLite.

On the Apple M4 Pro, macOS 27.0 (26A428), pinned Rust 1.98.0, release/M1 baseline:

- Median final process RSS: **109.672 MiB → 19.891 MiB**, about **82% lower**.
- Retained conversation histories: **96 → 9** (one open plus eight inactive).
- Retained messages: **28,800 → 2,700**.
- Retained text and thumbnail payload: **84.375 MiB → 7.910 MiB**.

Five alternating fresh-process runs per build visited 96 synthetic chats with
300 messages each. Each message had 1,024 bytes of text/caption; every fourth
had an 8,192-byte thumbnail. The baseline is `9b9ac99` with the identical
`memory_probe` fixture added. No compilation ran during measurement. Raw
samples, build details and source hashes are in
[the memory measurement file](benchmarks/apple-silicon-memory-2026-09-16.json).

This measures headless application state: no native window, fonts, renderer,
GPU, codecs, network, linked account or archive. It is not a whole-app memory
or energy comparison. The estimated cache budget includes owned capacities,
but allocator overhead, image-loader memory, renderer/codec allocations and
protected chats remain outside it. Lightweight metadata stays for visited chats.

Background messages are archived before the UI event and do not refill unopened
or evicted histories. A local first-page response is distinguished from a live
update, preventing premature completion of an outstanding load. Failed local
queries clear their loading flags so reopening or reconnecting can retry.

Formatting, both strict Clippy variants and warnings-denied rustdoc passed.
Default features passed **202 tests**; all features passed **204 tests**
(203 library + 1 binary), with seven explicit skips in each suite. New tests
cover LRU/byte eviction, protected work, draft and phone-state preservation,
thumbnail re-registration, shared-image retention, durable live events,
SQLite reload after edits/deletion, and failed-query retries. Live account,
sleep/wake and energy tests remain separate validation work.

Reproduce without a linked account:

```sh
cargo run --locked --release --features demo --example memory_probe > memory.json
```

For the baseline, use commit `9b9ac99` in a separate checkout with the same
example file and its `Cargo.toml` example registration.

## Sidebar copies: 16 September 2026

The main and archived lists already draw only the rows in the viewport, but
previously cloned every matching `Chat` first, including every group member ID.
They now sort lightweight indices and clone only the rows requested by egui.
Ordering is still recomputed from current state; no invalidation cache was added.
Pinned order, archive filtering, search matching and keyboard reveal are preserved.
The separate search-results view is unchanged.

On the Apple M4 Pro, macOS 27.0 (26A428), pinned Rust 1.98.0, release/M1 baseline:

- **1,000 chats:** median UI pass **0.612 ms → 0.093 ms**; median run p95
  **0.671 ms → 0.118 ms**.
- **10,000 chats:** median UI pass **5.549 ms → 0.134 ms**; median run p95
  **5.821 ms → 0.166 ms**.

Five runs per size/build used identical synthetic data: the usual demo chats,
then alternating direct chats and groups with 64 members, with every eleventh
added chat archived. Each run used an 1180×780-point view at two pixels/point,
eight warmup passes and 240 unchanged passes. The baseline was `916c198` with
the same fixture/probe added, measured before the changed build. Builds did not
run during measurement; before/after runs were not interleaved. Raw samples and
source hashes are in [the sidebar measurement file](benchmarks/apple-silicon-sidebar-2026-09-16.json).

These are whole-demo headless egui UI-pass CPU timings, excluding native rendering,
tessellation, GPU work and presentation. The app remains event-driven and does
not continuously repaint while idle. The measurements show lower work per repaint
in this synthetic workload, not a whole-app speedup, energy saving or RSS reduction.
Filtering, sorting and archive counting still visit all chats; this is not a
chat-order cache or an inactive-conversation eviction policy.

Formatting, both strict Clippy variants and rustdoc passed locally. Default
features passed **194 tests**, all features **196 tests** (195 library + 1 binary),
with 7 explicitly ignored in each suite. Regression coverage includes clicking
the correct group after reordering/archiving, stable pinned/timestamp order,
case-insensitive and phone search, and keyboard navigation to offscreen rows.
The large-chat demo is included in headless layout coverage and the native macOS
CI screenshot smoke test.

Reproduce the current workload with no linked account:

```sh
cargo run --locked --release --features demo --example sidebar_probe > sidebar.json
```

For the baseline, use the same fixture/probe with `src/app.rs` and
`src/ui/chats.rs` from `916c198` in a separate checkout.

## Scrolling and streaming: 16 September 2026

Apple M4 Pro, macOS 27.0 (26A428), pinned Rust 1.98.0, release profile with
thin LTO and the M1 baseline. Raw samples are in
[the scrolling/streaming measurement file](benchmarks/apple-silicon-scrolling-streaming-2026-09-16.json).
All data is synthetic; these measurements do not touch a linked account.

### Long conversation layout

Five alternating runs per mode used the same executable and 10,000 loaded
synthetic messages (short, wrapping, multiline, links and emoji). Each run used
an 1180×780-point view at two pixels/point, eight warmup passes, then 240 passes
scrolling upward by 80 points per pass. The probe verifies the scroll offset
really changes. The comparison disables only offscreen row skipping in the
same build; it is not a timing of a separate upstream executable.

- Median UI pass: full layout **67.658 ms**, cached rows **0.541 ms**.
- Median of each run's 95th percentile: **72.159 ms** versus **1.204 ms**.
- Rows laid out in the final pass: **10,000** versus **20** (visible + overscan).
- The measured layout cost is about **125× lower** in this workload.

These are headless egui UI-pass CPU timings. They exclude native rendering,
tessellation, GPU work and presentation; they are not FPS or battery results.
Both the previous and current native benchmark executables stopped repainting
in the current desktop session, so no new native renderer timing is reported.

The cache uses exact settled heights, never guesses. New/edited rows, changed
names, image-size completion and width/zoom changes trigger measurement.
The first visible message anchors history insertion and earlier height changes.
Explicit message IDs keep interactions stable. Text selection/dragging registers
the entire conversation, preserving offscreen endpoints and transcript copies;
scrollbar drags retain row skipping. Initial measurement, resizing and active
text selection still cost more, and the inexpensive height scan is still O(n).
This is not yet a prefix-sum index or a cache of selected text layouts.

### Time to the first ordered animation frame

The video fixtures contain 60 H.264 frames at 30 fps, with the same synthetic
`testsrc2` generation settings as the earlier media comparison. One warmup and
five measured decodes per clip used the automatic backend policy. The consumer
accepts frames immediately; timings include demux/session setup and conversion,
but exclude UI scheduling and texture upload. Medians:

- 320×180, software: first frame **0.994 ms**, full clip **17.790 ms**.
- 640×360, hardware: first frame **3.941 ms**, full clip **25.631 ms**.
- 1920×1080, hardware: first frame **9.576 ms**, full clip **92.751 ms**.

Previously playback waited for the complete decoded clip. GIF, WebP, OpenH264,
VideoToolbox and ffmpeg now publish frames incrementally. VideoToolbox drains
in presentation order, including delayed B frames; callbacks never block on
UI backpressure. A failed backend resets its published frames before fallback.
A still WebP remains a static image. Until EOF, playback holds the last available
frame if it catches the producer, then loops once the clip is complete.

Each of the two decoder jobs has at most eight unpublished frames: **3.125 MiB
per queue** at the maximum 320×320 RGBA size. A condition variable sleeps while
full. Dropping a receiver cancels its job; offscreen unfinished jobs expire after
one second, and a stalled receiver times out after five seconds to release an
abandoned window context. Native reordered output is separately bounded.
Completed clips keep the existing 150-frame/128 MiB decoded-pixel loop cache and
one texture per clip. This reduces startup buffering; it does not replace the
loop cache with a small playback ring. Codec/native buffers and GPU allocations
remain outside the cache budget, and no whole-process RSS reduction is claimed.

### Validation for this batch

- Default features: **193 tests passed**, 7 explicitly ignored.
- All features: **195 tests passed** (194 library + 1 binary), 7 ignored.
- Formatting, both strict Clippy variants and rustdoc passed on Apple Silicon.
- New regressions cover long-history selection/copy, scrollbar dragging, search
  jumps, prepend/edit/resize anchors, cached/full layout geometry, first-frame
  delivery before EOF, backpressure/cancellation, abandoned-window cleanup,
  partial playback, fallback replacement, texture reuse and cache eviction.
- Real VideoToolbox hardware decoding ran locally for the 640×360 and 1080p probes.
- Native screenshot evidence is produced by the macOS CI smoke test. Local
  window timing was unavailable in this desktop session, as described above.
- No live pairing, message delivery, microphone, notification or energy test.

Reproduce CPU layout and decoder measurements:

```sh
cargo run --locked --release --features demo --example conversation_probe > layout.json
cargo run --locked --release --features demo --example video_probe -- synthetic.mp4
```

## Earlier rendering and video batch: 16 September 2026

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
In that earlier batch, GIF/WebP decoders were unchanged and playback waited for
complete clips; the streaming batch above supersedes that behavior. Ordinary
videos still open externally. Neither batch implements zero-copy rendering or
measures energy use.

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
At that stage, complete bounded clips were decoded before playback; the newer
streaming batch above supersedes that behavior.

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
- Measure process/GPU memory pressure with several animated stickers visible
  at once; consider a smaller replay buffer if it improves the memory/energy tradeoff.
- Reduce initial/resize and active-selection layout costs; a prefix-sum row index
  could also remove the remaining O(n) height scan.
- Profile remaining chat sorting, font discovery and background maintenance
  before adding more caches or changing protocol timing.

Platform decisions follow [Rust's Apple target documentation](https://doc.rust-lang.org/rustc/platform-support/apple-darwin.html),
[SQLite's trigram tokenizer documentation](https://www.sqlite.org/fts5.html#the_trigram_tokenizer)
and [GitHub's ARM64 runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).
