# Repeatable performance baselines

`scripts/performance-fixtures.py` generates deterministic EPUBs for 3,000 chapters,
a long single chapter, illustrations and tables. Text and artwork are generated
locally; no private or externally downloaded books are needed. The manifest records
the expected chapter counts and byte fingerprints. ZIP timestamps are fixed.

`scripts/check-performance.py` runs the production-core Release probe at least
three times per book and reports medians. It rejects wrong book fingerprints,
wrong chapter counts, non-Release reports, eager chapter-body loading, non-finite
metrics and a retained chapter count above eight. Raw reports include sampled
resident memory and peak RSS on supported systems; unavailable memory is `null`,
not a measured zero. macOS and Linux memory implementations are separate.

Timing comparisons require the same corpus fingerprint, device identifier,
architecture, system version, toolchain and metric coverage. The default noise
allowance is 25% plus 5 ms for timings, or 25% plus 8 MiB for memory. These are
initial regression thresholds, not product latency targets. A first collection
explicitly records `baseline_compared: false`. Different machines or corpus
versions require separate baselines; they cannot establish a speedup.

```sh
python3 scripts/performance-fixtures.py /absolute/new/fixtures
cargo build --locked --release -p reader_core --example chapter_release_probe
python3 scripts/check-performance.py \
  --probe target/release/examples/chapter_release_probe \
  --fixtures /absolute/new/fixtures --device explicit-local-hardware-id \
  --output /absolute/new/baseline
python3 scripts/check-performance.py \
  --probe target/release/examples/chapter_release_probe \
  --fixtures /absolute/new/fixtures --device explicit-local-hardware-id \
  --baseline /absolute/new/baseline/summary.json --output /absolute/new/comparison
```

The `Release performance samples` workflow supports manual runs and a weekly run.
It records shared-core samples and checks the structural invariants. It does not
silently adopt a timing baseline or claim a successful timing comparison before
a compatible baseline has been reviewed. Existing normal CI tests continue to
check lazy loading, native pagination, cache behavior and large-shelf responsiveness.

Local macOS/Apple M4 collection and a second compatible comparison passed. This
does not measure application first paint, actual display/GPU presentation, reader
menu responsiveness, cancellation latency, thousands of covers or device battery
use. The separate native collector below measures client-specific timings. Native
platforms and physical devices cannot inherit the shared-core timing result as
their own acceptance.

## Native client collection

`scripts/check-native-performance.py` now binds an opt-in build to a fingerprint
of the dirty source tree and the complete application artifact, then collects
three fresh-process cold/indexed pairs using `many-chapters.epub`. Each pair uses
separate local data. Cold means no application chapter index, **not** a cold OS
filesystem cache. iOS cold timing includes import; Android starts from a supplied
file; desktop starts at the diagnostic reader request after app initialization.
These different boundaries must not be compared across clients.

Builds must explicitly enable the existing diagnostic harness. Normal release
packages do not include the performance entry points. For example:

```sh
python3 scripts/check-native-performance.py build \
  --app target/release/rust_epub_reader --output /absolute/new/desktop-build \
  --command cargo build --locked --release -p rust_epub_reader --features performance-checks
python3 scripts/check-native-performance.py collect desktop \
  --app target/release/rust_epub_reader \
  --build-record /absolute/new/desktop-build/build.json \
  --fixtures /absolute/fixtures --device apple-m4-local \
  --output /absolute/new/desktop-samples
```

Android: build `:app:assembleRelease` with `-PREADER_PERFORMANCE_CHECKS` and
`-PREADER_DIAGNOSTIC_APPLICATION_ID=local.reader.performance`. Supply the matching
ABI APK to `build --app ... --command android/gradlew -p android ...`, then use
`collect android --serial emulator-5556` with `ANDROID_HOME` pointing at the SDK.
The runner checks APK identity and non-debuggable status before installation or
clearing the **isolated** package. A physical-device serial is rejected. It does
not boot or stop the caller's Android emulator. GLES backend, emulator version,
Android build, dimensions, density, refresh rate and font scale are recorded.

For iOS, build the Release simulator app with
`PRODUCT_BUNDLE_IDENTIFIER=local.reader.performance` and
`SWIFT_ACTIVE_COMPILATION_CONDITIONS=READER_PERFORMANCE_CHECK`, preserving the
project's Rust build phase. Pass the `.app` to `build` and `collect ios`. The
runner creates a compatible iPad simulator, installs only the isolated bundle,
and shuts down/deletes that owned simulator on success or failure. It never
installs to a physical iPhone/iPad or modifies an existing simulator's data.

Each native report includes the actual EPUB SHA-256, a unique process-run ID,
viewport and harness version. Missing/failed/old/debug reports, incomplete mode
or chapter cases, no-op turns, eager iOS body loading and unbounded caches fail.
macOS now waits for native animation completion and a committed position change,
rather than assuming a fixed delay means a successful turn. Android awaits the
existing suspendable page-turn command and also asserts a changed page. iOS
waits for the UIKit committed anchor. Eight turns are required per case.

`--baseline /previous/summary.json` uses the same median/threshold comparison as
the core probe. Environment, build command, corpus and metric coverage must
match. Artifact/source fingerprints are retained for attribution but can differ
between a baseline and the candidate being evaluated. Raw native frame metrics
keep their original names: desktop update callbacks/CPU, Android FrameMetrics,
and iOS CADisplayLink intervals are different measurements, **not** equivalent
GPU presentation latency or a common FPS score. RSS, PSS and iOS physical footprint
also remain distinct metrics. Unknown memory is omitted, never reported as zero.

The collector explicitly lists cancellation latency, large-shelf cover pacing,
physical-device measurements and direct GPU presentation latency as unmeasured.
The native runner is currently a local diagnostic tool; hosted native timing
baselines and physical-device thresholds still require independent collection
and review. Local samples are not evidence of an application speedup.


## Media and short-publication diagnostics (schema 3)

The native collector also accepts `--book-name illustrated.epub`, `tables.epub`
or `long-chapter.epub` (default: `many-chapters.epub`). It derives distinct, valid
chapter targets from the publication's actual size. A single-chapter publication
has two cases, not six duplicate cases. The chapter walk is bounded by both 200
and the remaining chapter count, and every report records the actual walk count.

Corpus version 2 interleaves twelve distinct procedural PNGs with text, including
images on the early leaves being measured. The generated coloured-block PNGs
carry representative compressed data instead of tiny repeated placeholders.
Tables contain 160 body rows, so eight forward/back turns remain inside the last
chapter at wide tablet dimensions. Existing schema-2 reports and original corpus
files remain historical evidence; the changed protocol/corpus requires a new
baseline and must not be compared as if it were unchanged.

Schema-3 reports sample memory after every 25 walked chapters and at the walk's
end, then five times after a 750 ms settling period. These are sampled curves
and a short settling window, not proof of leak-free hours of reading. Desktop
now reports kernel peak RSS through a shared read-only core diagnostics helper;
iOS reports the task's peak resident size; Android retains its kernel VmHWM.
Physical footprint/PSS/RSS remain distinct counters. The iOS/Android body walk
runs through the production lazy source and style bridge in the background;
desktop also lays out each walked chapter. Native continuous-scroll longevity,
loading cancellation, dense shelves and physical-device calibration remain
separate checks. These additions are implemented; media/short-book Release runs
must still pass before this stage is marked verified.

## Opening cancellation

Opening work uses `reader_core::open_operation::Cancellation`. Register mobile
operation IDs before dispatching work. `cancel` uses a separate small registry,
never the publication, data-mutation, or chapter-decoding lock. Finish an ID only
after its worker returns. A late request for a finished ID is rejected.

A synchronous `run` scope checks cancellation at entry and exit (including error
exits), during hashing, during ZIP resource writes, and between chapter-prefix
probes. The thread-local scope is restored on return and unwind. A cached book
never owns its opening token; later ordinary chapter reads remain valid. Scopes
must not cross an async suspension or move to another worker thread.

Desktop closes drop the result receiver and cancel the matching worker. Android
also cancels its coroutine and rejects obsolete generations. iOS cancels directly
through the C ABI rather than queueing behind its `CoreStore` actor; transition
identities reject a late UI result. Initial iOS chapter preparation passes the
operation ID explicitly; that ID is not retained for subsequent reading.

```sh
cargo build --locked --release -p reader_core --example open_cancel_probe
target/release/examples/open_cancel_probe /absolute/path/many-chapters.epub
```

This probe records three repetitions of cancel-before-dispatch and two delayed
requests with fresh index directories. It distinguishes workers that finished
before cancellation and validates an ordinary reopen afterwards. Its delay-to-
quiescence measurements are synchronous-core measurements, not UI responsiveness
or a hard real-time promise. The platform smoke checks separately exercise the
real JNI/C ABI cancel-before-dispatch boundary and verify normal reading can
resume. ZIP directory parsing, HTML parsing and filesystem operations that are
already in flight can finish before their next cooperative checkpoint. Large
import copying, visible cancellation animations, and physical-device latency need
separate measurements; do not infer them from the core probe.

## Visible reading endurance

`collect --scenario visible-endurance` uses three independent cold processes
rather than cold/indexed pairs. Each process performs 64 forward and 64 backward
committed turns in both realistic and sliding modes. It verifies an exact return
to the starting page, records memory after every 16 completed turns, and retains
frame pacing and post-run settling samples. This is separate from the background
chapter-body walk. Native mobile probes prepare only current and nearby chapters
before another turn, matching the bounded preparation window of the readers;
neighbour-preparation times are kept in raw reports rather than hidden as zero.

The Android styled chapter view now has its own eight-entry LRU: a bounded source
list alone does not bound a wrapper that keeps every styled/fallback chapter it
has accessed. A 3,000-chapter regression checks eviction and reloading. This count
is a cache-ownership bound, not an exact heap-size estimate.

```sh
python3 scripts/check-native-performance.py collect desktop \
  --scenario visible-endurance --book-name long-chapter.epub \
  --app /absolute/immutable/reader --build-record /absolute/build.json \
  --fixtures /absolute/fixtures --device local-device \
  --output /absolute/new/endurance-results
```

By default a collection still requires the current workspace to match its build
record. `--pinned-build` explicitly measures an immutable previously recorded
artifact while source editing continues. It validates the build source manifest,
artifact identity and harness scenario, checks the artifact again at the end,
and records workspace fingerprints separately. `source_sha256` always names the
**built** source. Such a result does not validate later workspace changes. Keep
copies of pinned binaries/apps outside paths that subsequent builds overwrite.
Do not run builds or other heavy tests concurrently with performance collection:
pinning source provenance does not remove resource contention.

## Dense native shelves

`scripts/shelf-performance-fixtures.py OUTPUT --count 2000` creates deterministic
EPUBs with distinct procedural covers, a source manifest and stable UUIDs. The
catalog installer writes separate mutable JSON configurations with platform-local
absolute paths. Only immutable synthetic EPUB bytes may be hard linked. Existing
directories, duplicate identities, escaped paths, altered payloads and unsupported
counts are rejected. The small tooling tests generate only a few books; they do
not stand in for the full 2,000-book native run.

`check-shelf-performance.py` measures the actual desktop library renderer, Android
MainActivity/library grid/view model, and iOS AppHome with its normal library view.
The iOS library accepts an injected diagnostic storage root; the default remains
the normal application root. Android's fixture installer is a separate process,
so setup allocations do not become the measured application peak RSS. The
installer and reflective observer hook exist only in opt-in performance builds;
shipping builds never attach the observer. Android/iOS collectors reject user app
identities and physical devices. Desktop requires a provisioned fixture marker,
uses an isolated working directory and mock credentials.

```sh
python3 scripts/shelf-performance-fixtures.py /absolute/new/shelf --count 2000
python3 scripts/check-shelf-performance.py desktop \
  --app /absolute/immutable/reader --build-record /absolute/build.json \
  --fixtures /absolute/new/shelf --device local-device \
  --output /absolute/new/shelf-results
```

Each of at least three fresh processes records metadata layout readiness, the
first prepared cover group, 30 native scroll movements, frame pacing, six scroll
memory points and five settling samples. Readiness follows layout and subsequent
native frame callbacks, not direct GPU presentation timestamps. Desktop starts at
process main, iOS at App initialization before font restoration, and Android at
the isolated launch request for the real MainActivity after fixture installation.
Android baseline memory is captured after view-model construction. These timing
boundaries differ; compare each platform only against its own matching baseline.
Fixture installation and OS file-cache flushing are not part of the measurement.

The desktop probe drives ScrollArea offsets, Android sends native touch events,
and iOS animates UIScrollView offsets. Input type is recorded. The probes reject
whole-shelf eager cover requests and overgrown measured desktop/Android cover
caches/queues. iOS records its configured NSCache limits and real request/prepare
counts; a configured limit is not reported as a measured residency count. Kernel
peaks, current resident/physical memory and Android PSS remain distinct counters.
Screenshots are captured after metrics have been finalized so screenshot image
buffers do not become steady-state reading/shelf memory. Pinned-build provenance
rules also apply here. Native compilation and large fixture generation must finish
before timing begins.

### iOS shelf counter protocol

The iOS shelf harness is now version3; Android/desktop shelf harnesses remain
version1. It records `ranking_evaluations` and `geometry_updates` separately so a
scroll-triggered coordinate update is not mistaken for a rebuilt book ranking.
Version3 also retains the complete display-timestamp and actual main-thread
callback interval traces. The collector requires matching sample counts and
recomputes both clocks' percentiles from the raw traces. Neither clock measures
GPU presentation. Older iOS reports remain historical data and are not silently
compared as version3. The environment/protocol identity must match for a regression claim.
Optional `--diagnose-shelf-scroll` runs wait for the owned fixture root's
`scroll-ready`/`scroll-go` handshake before scrolling. These externally sampled
stack-diagnostic timings are excluded from the uninstrumented baseline.

### Simulator startup and repeatability

Both iOS native collectors accept `--ios-device-type` with an exact compatible
iPad Simulator type identifier. A missing/incompatible requested model fails
instead of silently switching to a smaller viewport. Keep the model, viewport,
runtime, font scale and accessibility settings fixed across comparisons.

Fresh Simulator boot can continue launching system extensions after `bootstatus`
returns. Before each measured app process, the iOS runner now requires ten
consecutive measured one-second host CPU samples with at least 80% idle. The
first cumulative `top` reading is discarded. Preflight has a 300-second deadline;
failure preserves `host-idle-samples.json` and fails the attempt without launching
the timed app. Successful observations are retained too. The policy is part of
the environment identity, so uncontrolled and controlled collections cannot be
silently compared. Waiting is diagnostic setup, not a change to product startup.
This reduces startup interference; it cannot guarantee zero contention during a
run, so raw traces, repeated processes and independent repeated collections remain
necessary. Keep compilers and other heavy tests out of timed collection windows.

On Apple M4 / Xcode 27.2 / iOS 27.2 Simulator, an unchanged Release app reproduced
the 2,000-book, 1032×1376-point shelf's approximately 49ms median-run P95 when
launched amid fresh Simulator startup work. With the CPU preflight, the same
artifact's median-run P95 was 16.67ms. This establishes an environment confound,
not an application speedup or a proof that all historical slow frames had that
cause. Independent version3 samples additionally check actual callback cadence.
Local evidence is under `target/stage03-autonomous-20261020/`; final repeatability
results and exact build/corpus fingerprints are indexed in its verification report.
Physical devices, other graphics backends and product latency targets still need
their own baselines. Do not reuse simulator thresholds as physical-device promises.

### iOS shelf comparison policy

Policy 1 keeps the 25% + 5ms allowance for median readiness and P50/P95 latency,
and 25% + 8MiB for sampled/steady memory. Ranking evaluations, geometry updates,
cover request counts and single-run frame extrema remain in every report, with
changes surfaced as `warnings`. They do not individually establish a sustained
latency regression: faster scrolling can produce more geometry callbacks, while
one delayed display-link wakeup can double a run's maximum interval.

Both clocks additionally gate the *missed cadence-slot share*. With the fixed
Simulator maximum refresh rate as the nominal slot reference, an interval beyond
1.5 slots contributes `round(interval / slot_period) - 1` missed slots. Thus one
long stall consumes more budget than a single extra-frame delay. Each process
retains its observed/missed/total counts. A candidate's pooled missed-slot share
must stay at or below `1.25 * worst_baseline_run_share + 1 / smallest_baseline_slot_count`.
The last term is one refresh slot's resolution per process. This is an initial
local noise budget, not a statistical independence claim or physical dropped GPU
frame count. The same number of processes, viewport and refresh configuration is
required. Consistent cadence deterioration and rare long stalls still fail even
when P95 is unchanged; regression tests exercise both cases. Raw worst intervals
are always retained and can still require investigation despite a budget pass.

The policy is explicit report metadata. A previous summary is rejected until the
operator explicitly uses `--reanalyse ORIGINAL_COLLECTION`, with the original
fixtures, immutable app and build record. Reanalysis validates every raw trace,
reconstructs the original medians byte-for-value, checks the artifact again and
writes a new directory. It records the original summary/report hashes, original
pass/failure and any unavailable end-of-collection source check. It never changes
the measured files or promotes their original status. An independent new collection
must then validate the calibrated policy; reanalysis alone is not that validation.

The isolated CPU/runloop diagnosis for this local shelf recorded 17 long callbacks.
Those approximately 33ms gaps contained 5.08–8.14ms of main-thread CPU work; the
longest continuous active runloop stretch was 5.57ms. The remaining gap was mostly
waiting for callbacks, not one uninterrupted 33ms layout operation. This diagnosis
used a separate instrumented artifact whose source/patch is retained; the normal
harness was restored byte-for-byte. Its timings are excluded from the baseline.

The reviewed local reference is
`docs/performance-baselines/apple-m4-ios27.2-pro13-synthetic-2000.json`.
It uses six processes and is specific to the recorded Apple M4, Xcode/runtime,
large iPad viewport, build command and harness hashes. For a compatible future
candidate (finish the recorded Release build before collecting):

```sh
python3 scripts/check-shelf-performance.py ios \
  --app /absolute/immutable/ReaderPreview.app --build-record /absolute/build.json \
  --fixtures /absolute/shelf-fixtures-2000-v1 --device apple-m4-ios-pro13-local \
  --ios-device-type com.apple.CoreSimulator.SimDeviceType.iPad-Pro-13-inch-M5-16GB \
  --runs 6 --pinned-build \
  --baseline docs/performance-baselines/apple-m4-ios27.2-pro13-synthetic-2000.json \
  --output /absolute/new/shelf-comparison
```

An incompatible environment fails rather than adopting this machine's numbers.
The periodic core workflow exercises the shared mobile-runner regressions too;
hosted core timing collection remains separate from these local iOS thresholds.
