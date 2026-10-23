# Reading appearance and interface linking

iOS and Android store a reading style as a pair of light and dark variants. Each variant has its own paper color, text color, optional background image, and image strength. Font selection, font size, spacing, margins and reading anchors remain in the existing shared settings.

Editing a preset creates or replaces **My style**, leaving built-in presets unchanged. The editor keeps a local draft and an explicit editing target. Changing the active appearance does not change that target. Save commits both variants together; Cancel and Back discard the draft. Desktop preserves separate light/dark paper, ink, background image and strength alongside its existing reading controls.

## Common linking rules

iOS, Android and desktop implement the same chain: **automatic source → app interface → reading appearance**. Fresh installations follow system appearance with both links enabled. Each link can be disconnected independently. Disconnecting freezes the current resolved appearance; reconnecting immediately uses the current upstream value. It never restores an older independent mode behind the user's back.

The automatic source is either System or Ambient. Only that source controls a linked group. An independent app interface is manually Light or Dark. Independent reading may be Light, Dark or Automatic, with its own source in Automatic. A manual reading shortcut changes app and reading together while they are linked, and disconnects the app from its automatic source. If reading is independent, the shortcut affects reading alone and exits its automatic mode. Navigation into or out of reading does not change these preferences.

All three native implementations consume `core/tests/fixtures/appearance-linkage.json`. The 19 shared sequences cover conflicting signals, missing ambient readings, manual changes, both disconnections, reconnection, shortcuts and persistence after every transition. Sensor values are transient and are never stored as user preferences.

## Automatic mode

- Android registers public `TYPE_LIGHT` only while the application is resumed and at least one active automatic group uses Ambient. Leaving the reader does not disable an ambient-controlled app interface. Below 15 lux continuously for three seconds selects dark; above 40 lux continuously for three seconds selects light. Values between thresholds keep the current appearance. Invalid readings are ignored. System changes do not reset an active ambient decision. Before valid data, or if the sensor is unavailable or registration fails, the selected group follows system appearance and shows the appropriate waiting/unavailable message.
- iOS reads system appearance from screen traits while applying the resolved app theme at the presentation root, including native chrome. The system signal is refreshed only in the foreground and cannot feed back from a manually overridden app theme.
- iOS includes a read-only private IOHID path, in both Debug and Release. It loads IOKit dynamically, matches usage page `0xff00` / usage `4`, schedules a default event client, and polls ambient event type `12`, field `12 << 16`, once per second. It samples only while the application is active and an active automatic group uses Ambient. No private entitlements, sensor-property writes or screen-brightness inference are used. Its native sensor scale is not presented as calibrated lux: below 5 continuously for five seconds selects dark; above 12 continuously for three seconds selects light. Values from 5 through 12 retain a settled appearance; on startup they settle to light after three seconds so a dim room does not indefinitely inherit system dark mode. Returning to the current appearance or missing a reading cancels a pending switch. Before a stable decision it follows system appearance. These more conservative iOS thresholds replace the original 15/40 pair following real-use feedback; they still need subjective calibration in normal dim and dark rooms. Missing symbols/services and five seconds without valid readings use the unavailable fallback; sensor recovery resumes the selected source. Backgrounding or disabling the last ambient group releases the client and clears transient decisions. The existing setting displays waiting, active or unavailable status as appropriate. The private interface is an intentional distribution choice and may change across iOS versions.
- Desktop preserves the Ambient selection but explicitly displays that ambient light is unavailable and that system appearance is used instead. An unused source does not display a misleading active fallback message.

## Storage and migration

The legacy style `mode` is retained for migration compatibility; active rendering and shortcuts resolve the linking policy instead. Mobile styles retain `reader.appearance.v1`; linking is independently stored as `appearance.linkage.v1` in existing iOS UserDefaults and Android `reader_settings`. Desktop stores the equivalent `appearance_linkage` object in its existing settings file, plus paired reading paints and the logical identity of the currently displayed variant. This identity is persisted independently of paper luminance so that a custom dark-colored light variant cannot overwrite the actual dark variant on restart.

A valid existing pair takes precedence over legacy colors. Missing/invalid pair settings are migrated from legacy colors and background images. Legacy typography and progress keys are not rewritten. A legacy global image is initially retained for both appearances; an explicitly disabled iOS image stays disabled. Existing app/reading differences are migrated as independent settings to preserve their appearance. Legacy Android automatic reading retains Ambient; legacy iOS automatic reading retains System. New installations default to the fully linked system chain.

iOS keeps its existing Paper, Gray, Soft green, White and Warm presets. Android keeps Paper, Gray and Soft green; legacy night and graphite settings select the corresponding dark variant. Custom legacy colors and non-default text colors become My style.

New images are decoded and reduced to at most 2048 pixels per dimension and saved inside the application container. Android copies new document selections rather than depending on a temporary document grant; old external URIs remain compatible. iOS stores filenames so app-container relocation does not invalidate references. Copying an image to the other variant shares the immutable file while keeping the color and strength independent.

An editor tracks only the files it creates. Cancel removes unused new files and never deletes previously saved images. Save retains files referenced by either variant, removes unused new files, and releases replaced managed images that are no longer referenced. Legacy external resources are preserved. Imports carry an editor request token; a result arriving after a target switch, removal or dismissal is discarded and its newly created file removed.

## iOS controls and settings

On iOS 26 and later, native reader buttons use `UIButton.Configuration.glass()` and the floating title and speech controls use SwiftUI `glassEffect`. Earlier supported versions retain system material fallbacks. The native quick-menu popover supplies its own material, without another blur background inside its hosted content. Reading paper and background images remain the selected style. Long settings panels use one SwiftUI presentation material with the resolved app color scheme, preserving text contrast when the theme changes while the panel is open.

Continuous and paginated text reserve a stable top region for the floating header. This budget changes with viewport geometry, not with control visibility, so showing or hiding controls does not move the text or repaginate it. Existing bottom/footer clearance remains in place.

The iOS linking section presents **automatic source → app interface → reading appearance** in one horizontal row. Selecting a node only changes the controls shown below it; the two separate connection buttons change the links. Each connection hit area is at least 44 points wide and separated from the node hit areas. Normal text fits the three nodes together; accessibility sizes can scroll horizontally and use menu pickers instead of cramped segmented controls. Reading preview, presets and the edit action share the following **Reading themes** section. These presentation changes do not alter the shared policy or persistence format.

The iOS AppIcon catalog uses the existing iOS/Android brand, exported as an opaque 1024-pixel PNG with nearest-neighbor scaling. The previous imageset alone did not configure a home-screen icon; both build configurations now select `AppIcon`.

## Verification

- `DEVELOPER_DIR=/path/to/Xcode.app/Contents/Developer sh ios/scripts/check-appearance.sh`: migration, idempotence, independent variant values, immutable presets, JSON round trips, mode resolution, ambient dwell/hysteresis, invalid/stale readings, foreground lifecycle and sensor recovery.
- Android `:app:testDebugUnitTest`: the appearance tests also cover ambient-light dwell time, hysteresis and invalid readings.
- `cargo test -p reader_core --test appearance_linkage`: shared sequences and desktop legacy migration.
- Android instrumentation `-e check appearance-linkage`, using the isolated `local.reader.appearance` diagnostic ID, drives real chain buttons, independent modes, an emulator ambient conflict, reconnection, persistence and exit after a linked shortcut. Set emulator system appearance dark and ambient light to 100 lux before running. Accessibility caches are refreshed between dynamic controls.
- Debug iOS `--appearance-reader --appearance-settings --verify-linkage` validates the production host and NativeReader paint, linked shortcuts, independent reading, unavailable-source fallback, reconnection, persistence, anchors and exit. The check captures actual UIKit windows to the diagnostic container's `Library/Caches/linkage-checks`.
- Debug desktop `--ui-preview --appearance-linkage-preview` opens the actual appearance window and captures its GPU viewport into the isolated working directory. Add `--appearance-linkage-independent` to inspect independent reading. `--ui-preview` uses mock credentials; run from an isolated diagnostic directory.
- Android instrumentation, built with `-PREADER_DIAGNOSTIC_APPLICATION_ID=local.reader.appearance`, supports `-e check appearance`. It drives the production activity and editor against a synthetic EPUB and a document-picker result. It refuses to run against the normal application ID. The optional `appearance-ambient` check coordinates with emulator light-sensor injection through a stage file and verifies actual dark/light transitions after the dwell time.
- Debug iOS launch arguments `--appearance-reader --appearance-settings` open `Documents/appearance.epub` in the production reader and display its settings. Use an isolated diagnostic bundle for runtime checks.
- Debug iOS `--appearance-reader --verify-ambient` injects a sensor into the production coordinator and checks native reading paint in both appearances, independent reading, background release, missing-data fallback, recovery, manual override, persistence and anchor/font preservation. `--appearance-reader --verify-ambient-device` uses the real IOHID reader and records three minutes of foreground sensor and native paint observations in `Documents/ambient-reader-check.json`, capturing each resolved appearance. These arguments are absent from Release and do not change Release sensor access.

## First reading defaults and introduction

A new mobile profile selects ambient light as its automatic source, keeps both appearance links enabled, and displays time plus a battery icon. Missing sensor data still falls back to system appearance. All three platforms start in paging mode with the realistic curl animation. Stored settings, including an explicit disabled status display or a different page animation, take precedence; legacy source decoding remains unchanged. Display reset also restores paging with a realistic curl.

The first prepared reader presents a short introduction after its book-opening transition. Mobile uses one bottom sheet; desktop uses one modal. The introduction explains page turns, revealing controls, the bottom-right menu, appearance settings, and mobile status information. The versioned local `reader.introduction.v1` flag (desktop `reader_introduction_seen`) is recorded on actual presentation. Dismissal does not change reading position; subsequent books, restarts and foreground changes do not trigger another presentation. Existing readers migrate to seen from prior reading evidence, while preferences such as language alone do not suppress the first-reading introduction.

## Color selection

All three clients offer the same 12 opaque paper swatches and 12 ink swatches.
Quick selections update the current light/dark draft and its sample, preserving
the other variant and unrelated reading themes. Existing exact color entry is
retained.

Desktop replaces the stock egui color popup with an expandable color row. The
panel contains a large saturation/value field, a separate vertical hue strip,
a preview and a hex field; the common colors remain visible below it. Pointer
positions clamp to the field boundaries. HSV state persists across RGB updates,
including grayscale/black selections. Invalid precise input disables Save.
Automatic ink remains available and is replaced only after a manual color
change.

Android's color swatch button opens an HSV dialog with a draggable field,
rainbow slider, precise entry and common colors. Cancel discards that dialog's
local choice. A debug-only sample activity exercises the production editor
without writing the user's appearance preferences. iOS retains its native system
ColorPicker and adds always-visible quick-color grids beneath both color rows.
