# EPUB pagination and illustration performance

Verification date: 2026-10-15. Pure native desktop, Android and iOS engines;
no browser or JavaScript API expansion. This report describes the local source
snapshot in `target/pagination-performance-review/verification.json`, not a
release or physical-device acceptance.

## Changes

- Desktop measures actual native text lines and illustration bounds rather than
  packing estimated whole blocks. Long paragraphs and basic table rows continue
  on adjacent leaves, including single/dual columns and existing turn animations.
  Canonical block numbers and source strings remain unchanged. Page fragments
  carry scalar character anchors for progress, reflow and cloud/LAN progress.
  Hit testing and selection overlays respect each fragment's clip.
- Desktop decodes images on two workers with an eight-job queue. The reading
  viewport and upcoming leaves schedule work; final image dimensions are reserved
  immediately. Texture resolution follows display size, capped at 2048 pixels.
  Cached textures survive captured curling sheets. Decoder completions refresh
  captured pages without resetting drag/settle progress. Live chapter text and
  transformed native galleys are reused instead of copied every frame.
- Android uses Compose font measurements for prose, headings, lists and basic
  grids. Oversized headings retain their text/weight in split fragments; oversized
  table rows retain their complete native-text fallback. Long paragraph line ends
  are shaped once and reused, with separate measurers for foreground/background
  pagination. Large layouts bypass the small measurement cache. Illustrations
  reserve their final dimensions while decoding; bitmap and header caches account
  for retained encoded keys as well as pixels. Supplemental images are grouped
  once rather than rescanning the chapter for each block.
- iOS retains image dimensions/source in native attachments, decodes only requested
  illustrations on a background queue, warms adjacent pages and redraws on load.
  The decoded-image cache has a 64 MiB target and the pending queue is limited to
  eight resources. Unchanged paginated chapters reuse their CoreText results.
  An unplaceable paragraph is repaired without changing its source coordinates;
  oversized attachment metrics are fitted, with a scrollable complete suffix as
  the final fallback instead of silently ending the chapter.

Cache budgets exclude the parsed book, currently visible/captured textures,
non-illustration table attachments, and transient decoder allocations. iOS now
trims its paging chapter window after completed turns; decoded chapter bodies
also have a separate bounded cache (see the long-publication check below).

## Real Chinese corpus

The original downloaded EPUBs are stored only under ignored `target/`, with
source links, sizes and SHA-256 fingerprints in `books/sources.json`:

- [紅樓夢, Gutenberg 24264](https://www.gutenberg.org/ebooks/24264)
- [阿Ｑ正傳, Gutenberg 25332](https://www.gutenberg.org/ebooks/25332)
- [朝花夕拾, Gutenberg 25271](https://www.gutenberg.org/ebooks/25271)
- [天工開物, Gutenberg 25273](https://www.gutenberg.org/ebooks/25273)

The shared audit checks all 23 chapters in three styles with scripts disabled.
Desktop checks the largest chapter of each book at two widths in all three
styles: 24 reflows, validating all page ranges/anchors and drawing representative
first/middle/last pages. These editions have no HTML tables and only cover media;
illustration/table evidence comes from the existing illustrated English corpus
and independent synthetic fixtures, not from the titles alone.

## Checks

- Shared core: 11 existing illustration, native presentation and typography tests.
- Desktop: 60 automated tests, plus the explicitly run downloaded-corpus test.
  Native line coverage verifies that the long paragraph's lines appear exactly
  once, fully inside the page clip, at both widths. Existing single/dual-column,
  cancelable-drag, cross-chapter and texture-lifetime regressions pass.
- Desktop UI: the pressure book's dual-column long paragraph, illustration,
  continuing grid and final sentence were inspected in the running app.
- Android: 19 unit tests, all three ABI libraries built, debug app/test APKs built.
  Device runner reconstructs canonical content in 20 selected-chapter reflows
  across ten books/fixtures. Additional production Compose measurements check
  45 drawn pages in narrow/wide columns: the complete pressure/typography fixtures
  and first/middle/last pages of 阿Ｑ正傳. All fit their available height.
  This check exposed a real long-heading clipping bug that structural source
  reconstruction alone had missed.
- iOS: simulator build, host layout checks (including an unplaceable font/margin
  case), 20 selected-chapter reflows with contiguous full UTF-16 coverage, and
  representative CoreText/image drawing. The UIKit reading-mode runner passes
  mode changes, reflow, canceled turns, saved anchors and unchanged-chapter cache
  reuse. JSON reports and logs remain under `target/pagination-performance-review/`.

## Image timing

A reproducible 4000 × 3000 JPEG diagnostic uses the debug Rust test build:

| CPU operation | Measured time |
| --- | ---: |
| Synchronous decode/scale reference | 3435.6 ms |
| First egui frame that schedules background decode | 8.65 ms |
| Background completion wait | 2372.1 ms |
| Cached frame mean, 20 frames | 0.038 ms |

Texture identity is unchanged on warm frames. Background work still takes time;
the improvement removes it from the gesture/render thread and avoids repeated
work. The reference and display-sized texture use different output bounds.
These are CPU test timings, excluding GPU upload/presentation; they do not claim
physical-device FPS or a release-profile speedup.

## Ordinary paragraph continuation (2026-10-15)

Desktop now fills the remainder of a leaf with complete lines of the next
paragraph even if that paragraph would fit on an empty leaf. Previously only
oversized blocks split, leaving large blank areas beneath ordinary paragraphs.
Image bounds and overlapping table/ruby bands remain indivisible unless a band
itself exceeds the viewport. Source offsets and selection continue to use the
same canonical block, rather than creating artificial paragraph breaks.

`target/reader-motion-pagination-20261015/` contains the current checks:
27 reader regressions, exact once-only painted-line coverage for 12 ordinary
paragraphs plus an oversized paragraph at two widths, and a separately run
24-reflow check on the four fingerprinted Chinese books. The desktop preview
also shows paragraph 4 continuing from the bottom of the left leaf to the top
of the right leaf. These checks do not replace Windows/Linux acceptance.

## Long-publication check (2026-10-16)

The supplied private novel (SHA-256
`36235c608a5aa87fb9c6468b66a9d3f5c5d286adda9fcff6d2b0622ecd9326c2`)
contains 3,545 canonical chapters, 322,601 blocks and 10,734,336 characters in an
18.36 MiB EPUB. The file stays local and is not checked into the repository.

The app entry points now open a catalogue backed by the EPUB archive. Canonical
body blocks, native DOM/style/script state and referenced images are loaded only
for requested chapters. The Rust LRU targets eight chapters / 32 MiB of estimated
body cost; one larger chapter is permitted. Desktop keeps a separate nearby
mutable window (usually three to five chapters). iOS keeps up to 32 decoded
bodies / 16 MiB, and Android keeps eight. These are body-cache budgets, not limits
on the entire process or on external references held by a renderer.

To preserve existing chapter/block/scalar coordinates, the first index build
reads prefixes of spine documents to identify legacy canonical chapter slots.
Ordinary chapters stop after the first real block, rather than parsing their
remaining text. Blank or media-only resources can require a complete individual
resource read. A hash-keyed disk index avoids these probes on subsequent opens.
This is bounded chapter loading, not a promise that first open never touches an
unread HTML resource. The sample opens with zero body loads; only its 3,545 prefix
probes are required on the first index build.

Search runs sequentially on a worker with cancellation and bounded results;
speech and export request chapters individually. Sync position validation loads
only the target chapter. Shared-resource reads have decompressed-size guards.
The original file remains unchanged, and no private book was uploaded.

Release evidence is in `target/lazy-chapter-release-20261016/`. Its full-book
compatibility audit compares every canonical body digest against the legacy eager
parser: all 3,545 chapters, 322,601 blocks and 10,734,336 characters match. Fixture
checks also compare images and all three native style policies, including blank
chapters and wholly illustrated books. A relocated archive reloads correctly
after eviction. These checks protect existing progress and annotations.

The shared-core Release probe builds its first index in 407 ms and reopens from
that index in 93 ms. Body plus native-style loading across 200 chapters has a
2.29 ms median / 4.28 ms p95. RSS reaches 23.6 MiB after that walk and the core
cache remains at eight chapters. These numbers exclude platform rendering.

Desktop profiling also found unrelated large font allocations: entire TTCs and
unused color bitmap strikes were read repeatedly, then copied by the outline
rasterizer. Desktop now extracts only the selected face, skips bitmap tables the
existing renderer cannot use, reads only small metadata tables during discovery,
and shares live face buffers through weak references. Outline/metric/table
identity, correct Chinese collection faces and whole-font checksums are tested.

Native Release results on the local Apple M4 Mac (macOS 27.2):

| Reader | First index / first content | Indexed reopen / first content | After 200 chapters |
| --- | ---: | ---: | --- |
| macOS native window | 1,156 ms | 712 ms | 320–326 MiB RSS; 3 UI bodies, 8 core bodies |
| Android API 35 arm64 emulator, host GPU | 787 ms | 310 ms | 191 MiB RSS / 86 MiB PSS; 8 core bodies |
| iOS 27.2 arm64 simulator | 1,149 ms (includes import) | 243 ms | 422–439 MiB RSS / 50–64 MiB physical footprint; 32 Swift bodies, 8 core bodies |

These timers begin inside an already started process. Desktop includes its
opening transition and waits for two native frames after content readiness;
Android waits for production Compose content and two frame callbacks; iOS lays
out the production UIKit/CoreText reader and forces a hierarchy draw. iOS cold
includes import/copy/indexing, while the others open an existing file. “Cold”
means a new application process and missing book index, not an OS-cold disk.
RSS includes mapped/shared pages; Android PSS and Apple's physical-footprint
metric have different accounting and must not be compared as identical measures.
The iOS harness checks its unique run directory and the initial zero-body-load
counter to prevent stale measurements.

Each reader flips forward/backward at early, middle and final chapters in curl
and slide modes. Desktop update CPU p95 is at most 6.73 ms, with frame-callback
p95 about 11–13 ms. iOS display-link p95 is 16.67 ms; its worst observed interval
is 119 ms, so occasional emulator hitches remain visible in the evidence.
Android native frame-duration p95 is about 18–20 ms in later cases, and 27–30 ms
in the first curl case. Its curl turns take about 383–385 ms including the
intentional animation. These are simulator/native-window diagnostics, not
physical-device latency or a claim of uniformly perfect frame pacing.

The initial Android emulator used SwiftShader software graphics: it produced
much worse frame-duration p95 (up to 178 ms), despite bounded chapter caches.
That baseline is preserved under `android-software-gpu/`; final Android reports
include the actual Apple M4 host GLES driver string. It is not valid to treat
software-emulated GPU results as phone performance.

The actual Android scroll reader also runs at early/middle/final chapter jumps.
A hidden eager path was removed: `itemsIndexed` passed lazy chapter bodies to
Compose's nearby-key map, even though its key ignored the item. Count/index keys
now avoid body access, and initial scroll positioning starts directly at the
target chapter. Each verified jump loads only two additional bodies; the core
cache stays at eight. This guards the scroll path separately from paged mode.

`final-report.json` records per-case measurements, original-book and built-binary
hashes, source fingerprints, cache cleanup and verification results. The current
workspace checks pass 231 tests, strict Clippy, formatting, localization and diff
checks. Four fingerprinted Chinese books pass 24 further desktop reflows. All
3,545 canonical body digests match the legacy parser. The first indexing probe
and app-level caches are deliberately described separately from body loading.


## Reproducing local measurements

Normal release builds exclude platform performance entry points. Desktop requires
`--features performance-checks`, Android requires `-PREADER_PERFORMANCE_CHECKS`,
and iOS requires `SWIFT_ACTIVE_COMPILATION_CONDITIONS=READER_PERFORMANCE_CHECK`.
CI checks all Cargo features, but desktop release packaging enables only its
normal application features. Android's diagnostic activity and iOS's measurement
code are opt-in and do not ship in ordinary release packages.

For a caller-owned EPUB, the offline core probe can be run with:

```sh
cargo run --locked --release -p reader_core --example chapter_release_probe -- /absolute/path/to/book.epub
```

It reports catalogue, per-chapter and bounded-cache timings without book text.
Its RSS probe is supported on macOS; a zero value on other systems means that
this probe did not measure memory. Platform measurements use
`desktop/src/performance.rs`, `ios/Tests/ReadingReleasePerformance.swift` and
`android/app/src/performance/`. The mobile harnesses expect a locally supplied
large EPUB; no private sample is bundled or downloaded by the tests. Initial
investigation-only eager/C-ABI probes were preserved in the ignored local
`target/precommit-cleanup-20261016/local-probes/` directory.

Desktop native-window measurement must run from an isolated, writable working
directory with `READER_PERFORMANCE_REPORT` set to an output path. Launch its
feature-enabled executable with `--measure-reader /absolute/path/to/book.epub`.
This diagnostic uses mock credentials; it must not be used to test real account
sync. Keep previous reports and binary hashes when rebuilding, since a later
source check does not retroactively validate an earlier measurement.

## Remaining scope

Complex nested/rowspan tables, cell-level selection, fixed layout, vertical text,
and other unsupported CSS remain outside this increment. Unsupported/corrupt
resources retain a bounded fallback; desktop's existing 16-megapixel decode
safety limit remains. Real GPU/device frame pacing still needs physical-device profiling. Windows/Linux and physical
Android/iPhone acceptance have not been performed in this macOS session.

No commit or push was made. The diagnostic desktop app and both mobile preview
surfaces remain available for user experience checks.
