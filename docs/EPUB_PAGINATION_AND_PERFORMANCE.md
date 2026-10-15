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
non-illustration table attachments, and transient decoder allocations. iOS still
retains the book's page layouts; this is not a bounded paging chapter window.

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

## Remaining scope

Complex nested/rowspan tables, cell-level selection, fixed layout, vertical text,
and other unsupported CSS remain outside this increment. Unsupported/corrupt
resources retain a bounded fallback; desktop's existing 16-megapixel decode
safety limit remains. Initial EPUB parsing, huge publication memory use and real
GPU/device frame pacing need separate profiling. Windows/Linux and physical
Android/iPhone acceptance have not been performed in this macOS session.

No commit or push was made. The diagnostic desktop app and both mobile preview
surfaces remain available for user experience checks.
