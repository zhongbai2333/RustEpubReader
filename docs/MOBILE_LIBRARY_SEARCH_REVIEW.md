# Mobile library navigation and search

Local review: 2026-10-15. Changes remain uncommitted alongside the preceding library/receiving work.

## Result

- Two home destinations sit together at the lower left; the search action sits independently at the lower right. Search expands into a bottom input bar and filters local book titles.
- iOS 26+ uses the real system TabView/search-role tab, preserving Liquid Glass, selection/press behavior and native search morphing. The search controller is scoped to the search presentation, preventing an extra top search field after dismissal. iOS 17–25 keep the public SwiftUI/material fallback; regular-width iPad sidebar search is retained.
- iOS library/search presentations share the library model, query, sort and stable shelf order. Only the visible native tab contributes cover coordinates. Resigning input to open a book preserves the query and filtered return destination.
- Android uses Material surfaces, an animated expanding input, selected tab semantics and default font scaling. The search circle follows the measured navigation-group height. Import moved from the old top search row to a labeled toolbar action. Insets place the field above the IME, and inactive shelf search cannot consume reader Back events.
- Clear retains the search session; Cancel closes it. Hiding only the keyboard preserves the query. iOS uses its native cancel behavior, including return to the previously selected tab. Android search from Sync opens the existing library. Both filters ignore leading/trailing whitespace without rewriting the input text.

The iOS system-search API was checked against the installed SDK and [Apple's activation documentation](https://developer.apple.com/documentation/swiftui/view/tabviewsearchactivation(_:)). No private APIs or new dependencies were added. Existing shared copy was reused.

## Checks

- Final Android Release packaging/lint and isolated diagnostic Debug packaging passed. The JVM suite passed 38 tests during this change set; later adjustments were UI geometry/filter normalization, verified through the native UI and final packaging.
- Final iOS arm64 simulator Debug build passed. Shared localization validation reports 976 keys and zero errors; seven checker regression cases and whitespace checks passed.
- Actual native-UI checks covered idle layout, expansion, input, clear, cancellation, keyboard avoidance, and search from Sync. The iOS final native motion was recorded as `target/mobile-library-search-20261015/ios-native-search-final.mov`.
- Three synthetic EPUBs were staged only after confirming both review catalogs were empty. `harbor` and a whitespace-padded query matched `Harbor Notes` and `Summer HARBOR`. iOS no-match feedback was checked. Android input-method suggestion commits exposed the trailing-space case, which was corrected and rechecked with both matches visible.
- Each platform opened a synthetic EPUB into the real native reader and returned to its search results with the query preserved. On Android, the auto-hidden reader controls had to be shown and clicked promptly; this was an automation timing issue, not a source change to reading controls.
- Owned synthetic book/config files and test activity were moved out of the catalogs and retained as evidence. Original empty catalogs and the temporary AVD handwriting setting were restored. Previews and simulators remain open without relaunch/keep-alive jobs.

The design skill's three-model plan and actual-screenshot reviews were completed and read. The initial custom iOS approach was revised following direct user feedback about losing native glass/animation. Adopted screenshot refinements include equal Android rail/circle height, explicit cancel touch size/padding, hiding meaningless empty-library sort, and distinct no-match feedback. Review claims of human acceptance or inaccessible sort were not treated as facts: sort already used a native TextButton.

Evidence, review decisions, build logs and source fingerprints are under `target/mobile-library-search-20261015/`. Screenshots initially supplied to reviewers used empty catalogs; subsequent populated-book and actual-reader checks are separate local evidence. Physical devices, VoiceOver/TalkBack, maximum accessibility sizes, the older-iOS fallback and iPad were not runtime-accepted in this pass. Earlier Rust/cover/transfer checks refer to their earlier source snapshot; this UI follow-up did not rerun them.

## Header follow-up

The subsequent header refinement places the library heading/count at the upper left and orders the right-hand actions as Import, Manage, More on both mobile platforms. Android uses separate outlined circular icon actions and a capsule management button; its header is an ordinary full-width grid item, so it scrolls with the shelf without a pinned app-bar backdrop. The empty-library import action is also a capsule. iOS retains real toolbar buttons, fixed toolbar separators for independent glass shapes, and the native TabView/search animation. Its plain heading explicitly hides the shared toolbar glass background to avoid the system collapsing it into an overflow action; the navigation-bar backdrop is hidden.

This is a focused geometry/order follow-up to the preceding reviewed navigation, using existing shared strings. It did not repeat the full-page council process. Actual simulator checks confirmed both More menus and both Import file pickers; iOS picker cancellation returned to the shelf. The Android diagnostic app was restarted after the picker check. No books or account data were changed. Management remains disabled for empty catalogs; populated management and maximum accessibility text sizes were not rechecked in this follow-up. The final simulator screens were retained under `target/mobile-library-header-20261015/`, alongside current-source fingerprints and build/check logs. Both previews remain open.
