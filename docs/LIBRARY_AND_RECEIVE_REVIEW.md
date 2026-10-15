# Library responsiveness and receiving UI review

Local review: 2026-10-15. Base: `430ca6b`; the changes described below are uncommitted.

## Library loading

- Shelf covers no longer require a complete EPUB/chapter parse. The shared reader reads the cover manifest entry with a streaming 16 MiB input limit, decodes off the UI thread and caches JPEG thumbnails up to 480 × 720 without upscaling. Source path, size and nanosecond modification time invalidate positive and negative disk entries.
- Desktop uses one bounded cover worker (at most eight pending requests), at most two texture uploads per frame and a 64-entry texture budget. Offscreen rows are skipped, including during shelf reordering unless a card moves from the visible region.
- Android startup, directory scanning and refresh run on IO dispatchers. Refresh returns a coalesced Job, and the pull-to-refresh indicator awaits completion with a finally reset. Cover work is limited to two workers and 96 encoded memory entries.
- iOS catalog requests return metadata immediately. A separate cover actor performs thumbnail requests and display preparation, with an NSCache budget of 64 images / 64 MiB. SwiftUI no longer reads cover files synchronously in view bodies.
- The persistent thumbnail directory is an application cache; this change does not implement a total disk-size eviction policy. Existing unsupported cover formats are not newly guaranteed.

## Receiving flow

All platforms keep source selection, receiving/verification, explicit import preview and result counts separate from publishing/history. The payload protocol and explicit import requirement remain unchanged.

- Android: grouped source rows, a separate scanner using the existing ZXing engine, normal orientation handling, actual framing-region overlay, permission/error fallback, optional torch and gallery access. The receiver has no top-right X per user request. Back/swipe dismissal, Cancel and Done remain available. Success icon/title/counts are centered.
- Desktop: focused link input, secondary local-image action, compact virtualized preview, aligned actions. Native file picking runs off the UI thread. The X now uses an independent 40-point content-layout target; egui's built-in title-bar close target overlapped title-bar interaction after increasing the window margin.
- iOS: native grouped source list, conditional focused link editor with the text field’s system paste menu, keyboard-safe actions, preview titles/sizes and explicit import. The sheet defaults to medium height and can expand. Returning from link editing retains the draft; reopening from the library starts a fresh receiver. Dismissal cancellation belongs to the outer sheet, avoiding cancellation when presenting the child scanner.

Three-model plan reviews were completed for Android, desktop and iOS, followed by shared-copy review and actual-screenshot review. Adopted final refinements: equal Android source emphasis, shorter desktop form with a secondary image button, explicit semantic iOS title colors and medium/large detents. Incorrect review assumptions about missing iOS group separators and nonfunctional macOS window controls were rejected after checking pixels and source. Evidence and decisions are under `target/android-receive-redesign-20261015/`.

## Verification and limits

- Workspace Rust tests: 192 passed, one opt-in corpus test ignored. Final desktop layout/pointer regression was repeated after visual refinements; five states, three viewport sizes and two scale factors passed. Strict workspace Clippy passed.
- Shared catalogs: 976 keys, zero reported errors; seven checker rejection cases passed. Font-catalog checks passed (20 entries).
- Android: all three native ABI Release builds passed; 38 JVM tests passed. After the final UI-only changes, Release packaging/lint and isolated diagnostic Debug packaging passed again.
- iOS: final arm64 simulator Debug build passed. This is not a new device/archive/IPA signing verification.
- Actual CUA checks: desktop X dismisses and reenables library controls; Android empty-library pull refresh shows and then clears its indicator; Android keyboard leaves the receive action visible; Android preview/result fixtures render; final success content is centered with no X; iOS starts half-height, expands for the keyboard and returns half-height, with Back draft retention and fresh-session reset checked.
- Performance regressions: a 2,000-book egui shelf renders and searches while the cover worker is deliberately blocked; a 500-entry iOS catalog does not extract any covers; shared cover tests cover invalid files/cache invalidation, bounded decompression, GIF/BMP decoding and no upscaling. Timings are diagnostic observations, not device-wide startup promises.
- Android screenshots use a separately installed diagnostic application and public synthetic book names. Camera screenshots use the emulator's synthetic camera. Real camera decoding, physical devices and a new complete cross-platform network transfer were not accepted from these UI fixtures. Existing Rust transfer regressions passed.

Log roots: `target/shelf-performance-20261015/` and `target/android-receive-redesign-20261015/`. Simulator, emulator and desktop preview were left available without keep-alive/relaunch jobs. Temporary emulator keyboard settings used for testing were restored.
