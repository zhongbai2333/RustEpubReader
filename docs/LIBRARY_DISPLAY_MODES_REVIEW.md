# Library display modes

Local changes and simulator review on 2026-10-15, uncommitted alongside the earlier library/receiving changes.

- Android More now contains separate Sort and Display sections. Recent/Title and Grid/List use selected checkmarks; the old conditional sort toggle beside All Books is removed.
- All three shelves support a single-column list with small cover, a bounded two-line title, reading position and trailing book actions. Management replaces book actions with selection. Existing search, ordering, multi-selection and recent-book cards use the same data in both displays.
- Grid remains the default for installations without a saved preference. Display and sort are stored locally: Android preferences, shared iOS AppStorage for library/search/iPad, and backward-compatible desktop settings fields.
- List rows register their actual small cover frames for the existing book transition. Existing lazy thumbnail workers/caches are reused. Desktop skips invisible rows; Android/iOS retain lazy shelf containers. There is no new full-book metadata/cover scan on display changes.
- Desktop display selection sits beside sort. Mobile choices remain in More. iOS uses native toolbar/Menu/Picker and native search, retaining system glass. Missing iOS list covers use a small book glyph rather than squeezing a duplicate title into the thumbnail; actual covers and grid placeholders keep their existing behavior.

## Validation

- Android diagnostic Debug packaging and Release packaging/vital lint passed; the existing JVM suite passed 38 tests. iOS final arm64 simulator build passed. Desktop build and Clippy with warnings denied passed.
- The existing desktop long-title/mixed-cover geometry regression now covers both displays, normal/management states, widths 420/820/1509 and pixel scales 1/1.5/2. It also checks list rows do not overlap. The test passed. An initial desktop row/menu reservation issue was found during real rendering, corrected and rechecked.
- Shared localization and its seven rejection regressions passed; whitespace checks passed. New strings are Display, Grid and List in both shared languages.
- Actual native checks used four owned synthetic EPUBs staged only after all three review catalogs were confirmed empty. They covered list/grid switching, Android sort selection, management selection surviving display changes on all three, title truncation, list book-action menus, real book open/return on all three, and list search results on desktop/iOS. The query `harbor` matched Harbor Notes and Summer HARBOR.
- Android/iOS retained List on restart; desktop retained it after normal quit and replacement/relaunch. Android sort selection was also checked in its stored preference. Library/search share the same iOS display setting.
- Existing authored chapter titles are preserved. After reading the synthetic chapter named Chapter One, desktop/Android show that actual chapter title; a never-opened book uses the localized chapter-number fallback. iOS uses its existing anchor chapter-number presentation. These preexisting metadata differences were not rewritten as UI translations.

The design skill's three-model plan, three-label copy review and actual-screenshot visual review were completed and read. Adopted changes include compact bounded rows, explicit selected choices, small-cover animation frames and the simpler iOS missing-cover glyph. Inferred review claims about nonexistent iOS-menu screenshots and a three-line desktop title were checked against the actual artifacts: the desktop image shows two title lines plus the separate chapter line. Broader navigation changes and translating authored chapter names were rejected.

Owned test files and activity were retained outside the original catalogs, and the empty review catalogs were restored. Diagnostic previews remain open without keep-alive/relaunch services. The isolated Android review package was updated; no personal library or account was used. This pass did not perform physical-device acceptance, maximum accessibility text/VoiceOver/TalkBack testing, a thousand-book benchmark, or runtime checks of the older-iOS/iPad branches. Earlier core/transfer evidence remains separate.

Build logs, decisions, screenshots, fixtures and source fingerprints: `target/library-list-20261015/`.

Final preview restart: Android/iOS returned to their restored empty shelves. The desktop process remains running but is waiting on macOS Keychain authorization before it can expose its window. A new SecurityAgent process appeared at launch; the protected authorization app cannot be controlled through the computer-use tool. The earlier same-build desktop list/search/manage/open-return runtime checks above completed before this last restart. The final desktop preview therefore needs the user's system authorization; it is not claimed to be interactively ready after the last restart.

After the user completed system authorization, the desktop preview reached the restored empty shelf. Both sort choices and both display choices were opened through the native UI, the saved List choice was retained, and the menus were dismissed. The desktop preview is now interactive and remains open; the final authorization wait above is resolved.
