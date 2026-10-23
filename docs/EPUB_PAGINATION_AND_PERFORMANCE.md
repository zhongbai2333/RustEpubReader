# EPUB pagination and image loading

Desktop, Android and iOS use native layout, source-coordinate anchors and bounded
chapter/image caches.

## Pagination and images

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
also have a separate bounded cache (see chapter loading below).

## Paragraph continuation

Desktop now fills the remainder of a leaf with complete lines of the next
paragraph even if that paragraph would fit on an empty leaf. Previously only
oversized blocks split, leaving large blank areas beneath ordinary paragraphs.
Image bounds and overlapping table/ruby bands remain indivisible unless a band
itself exceeds the viewport. Source offsets and selection continue to use the
same canonical block, rather than creating artificial paragraph breaks.

## Chapter loading

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
unread HTML resource. First-index cost therefore depends on the number and structure of spine documents.

Search runs sequentially on a worker with cancellation and bounded results;
speech and export request chapters individually. Sync position validation loads
only the target chapter. Shared-resource reads have decompressed-size guards.
The original file remains unchanged.

Desktop extracts only the selected font face, skips bitmap tables the
existing renderer cannot use, reads only small metadata tables during discovery,
and shares live face buffers through weak references. Outline/metric/table
identity, correct Chinese collection faces and whole-font checksums are tested.

Android scrolling uses count/index keys without accessing chapter bodies in
Compose's nearby-key map. Initial scroll positioning starts directly at the
target chapter. This keeps the scroll path lazy independently of paged mode.

## Reproducible checks

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
large EPUB; no private sample is bundled or downloaded by the tests.

Desktop native-window measurement must run from an isolated, writable working
directory with `READER_PERFORMANCE_REPORT` set to an output path. Launch its
feature-enabled executable with `--measure-reader /absolute/path/to/book.epub`.
This diagnostic uses mock credentials; it must not be used to test real account
sync. Keep previous reports and binary hashes when rebuilding, since a later
source check does not retroactively validate an earlier measurement.

Public Chinese books suitable for a local compatibility corpus include:

- [紅樓夢, Gutenberg 24264](https://www.gutenberg.org/ebooks/24264)
- [阿Ｑ正傳, Gutenberg 25332](https://www.gutenberg.org/ebooks/25332)
- [朝花夕拾, Gutenberg 25271](https://www.gutenberg.org/ebooks/25271)
- [天工開物, Gutenberg 25273](https://www.gutenberg.org/ebooks/25273)

Keep downloads and their source/license/fingerprint manifest outside the source
repository. Prose-only editions do not exercise tables or illustrations; use the
committed synthetic fixtures for those cases. See [native typography](EPUB_NATIVE_TYPOGRAPHY.md)
and [performance collectors](PERFORMANCE_BASELINES.md).

## Remaining scope

Complex nested/rowspan tables, cell-level selection, fixed layout, vertical text
and other unsupported CSS remain outside the native subset. Unsupported/corrupt
resources retain a bounded fallback; desktop keeps its 16-megapixel decode safety
limit. Profile frame pacing on each graphics backend and physical device; host
CPU timings and simulator callbacks are not physical GPU presentation latency.
