# Library and receiving books

## Loading and covers

- Shelf covers do not require a complete EPUB/chapter parse. The shared reader reads the cover manifest entry with a streaming 16 MiB input limit, decodes off the UI thread and caches JPEG thumbnails up to 480 × 720 without upscaling. Source path, size and nanosecond modification time invalidate positive and negative disk entries.
- Desktop uses one bounded cover worker (at most eight pending requests), at most two texture uploads per frame and a 64-entry texture budget. Offscreen rows are skipped, including during shelf reordering unless a card moves from the visible region.
- Android startup, directory scanning and refresh run on IO dispatchers. Refresh returns a coalesced Job, and the pull-to-refresh indicator awaits completion with a finally reset. Cover work is limited to two workers and 96 encoded memory entries.
- iOS catalog requests return metadata immediately. A separate cover actor performs thumbnail requests and display preparation, with an NSCache budget of 64 images / 64 MiB. SwiftUI does not read cover files synchronously in view bodies.
- The persistent thumbnail directory is an application cache; the thumbnail cache has no total disk-size eviction policy.

## Display and sorting

- Android More contains separate Sort and Display sections. Recent/Title and Grid/List use selected checkmarks.
- All three shelves support a single-column list with small cover, a bounded two-line title, reading position and trailing book actions. Management replaces book actions with selection. Existing search, ordering, multi-selection and recent-book cards use the same data in both displays.
- Grid remains the default for installations without a saved preference. Display and sort are stored locally: Android preferences, shared iOS AppStorage for library/search/iPad, and backward-compatible desktop settings fields.
- List rows register their actual small cover frames for the existing book transition. Existing lazy thumbnail workers/caches are reused. Desktop skips invisible rows; Android/iOS retain lazy shelf containers. Changing the display does not trigger a full-book metadata/cover scan.
- Desktop display selection sits beside sort. Mobile choices remain in More. iOS uses native toolbar/Menu/Picker and native search, retaining system glass. Missing iOS list covers use a small book glyph rather than squeezing a duplicate title into the thumbnail; actual covers and grid placeholders keep their existing behavior.

## Mobile navigation and search

- Two home destinations sit together at the lower left; the search action sits independently at the lower right. Search expands into a bottom input bar and filters local book titles.
- iOS 26+ uses the real system TabView/search-role tab, preserving Liquid Glass, selection/press behavior and native search morphing. The search controller is scoped to the search presentation, preventing an extra top search field after dismissal. iOS 17–25 keep the public SwiftUI/material fallback; regular-width iPad sidebar search is retained.
- iOS library/search presentations share the library model, query, sort and stable shelf order. Only the visible native tab contributes cover coordinates. Resigning input to open a book preserves the query and filtered return destination.
- Android uses Material surfaces, an animated expanding input, selected tab semantics and default font scaling. The search circle follows the measured navigation-group height. Import is a labeled toolbar action. Insets place the field above the IME, and inactive shelf search cannot consume reader Back events.
- Clear retains the search session; Cancel closes it. Hiding only the keyboard preserves the query. iOS uses its native cancel behavior, including return to the previously selected tab. Android search from Sync opens the existing library. Both filters ignore leading/trailing whitespace without rewriting the input text.

The mobile header places the library heading/count at the upper left and orders the right-hand actions as Import, Manage, More on both mobile platforms. Android uses separate outlined circular icon actions and a capsule management button; its header is an ordinary full-width grid item, so it scrolls with the shelf without a pinned app-bar backdrop. The empty-library import action is also a capsule. iOS retains real toolbar buttons, fixed toolbar separators for independent glass shapes, and the native TabView/search animation. Its plain heading explicitly hides the shared toolbar glass background to avoid the system collapsing it into an overflow action; the navigation-bar backdrop is hidden.

## Receiving books

All platforms keep source selection, receiving/verification, explicit import preview and result counts separate from publishing/history. The payload protocol and explicit import requirement remain unchanged.

- Android: grouped source rows, a separate scanner using the existing ZXing engine, normal orientation handling, actual framing-region overlay, permission/error fallback, optional torch and gallery access. The receiver has no top-right X. Back/swipe dismissal, Cancel and Done remain available. Success icon/title/counts are centered.
- Desktop: focused link input, secondary local-image action, compact virtualized preview, aligned actions. Native file picking runs off the UI thread. The close action uses an independent 40-point content-layout target.
- iOS: native grouped source list, conditional focused link editor with the text field’s system paste menu, keyboard-safe actions, preview titles/sizes and explicit import. The sheet defaults to medium height and can expand. Returning from link editing retains the draft; reopening from the library starts a fresh receiver. Dismissal cancellation belongs to the outer sheet, avoiding cancellation when presenting the child scanner.

The receive protocol, size limits and import validation are described in [book sharing](BOOK_SHARING.md).
