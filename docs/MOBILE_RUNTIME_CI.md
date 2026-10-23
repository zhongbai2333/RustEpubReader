# Local mobile runtime checks

Native UI tests run locally, not in GitHub Actions. CI and Release retain Rust
core/bridge tests, Swift host logic checks, Android JVM tests/lint, compilation
and package verification. They do not start mobile simulators/emulators or run
XCTest/instrumentation UI cases. `test-mobile-runtime.py` and `test-ios-ci.py`
are host-side unit tests for the local tools; they do not launch a device.

`scripts/check-mobile-runtime.py` uses synthetic EPUBs and isolated app IDs,
without user libraries, credentials or a live GitHub account. Keep its reports,
screenshots and XCResults under an ignored local output directory.

Android builds a separate `local.reader.columns` debug app and instrumentation
APK. The runner checks APK package names and requires an emulator before installing.
It exercises imports, automatic/forced columns, reflow anchors, persisted settings,
JNI storage failures/recovery, navigation/return, remote-position acceptance,
reader reopening after activity recreation, explicit close, and continuous mode.
Use an owned emulator and a diagnostic APK matching its ABI. TalkBack checks
require an image with that service installed. Native checks are not bundled
into the published Release APK.

Each local iOS invocation creates its own iPhone or iPad Simulator, choosing a device
type compatible with the selected runtime. It installs the shared debug build,
executes the selected checks, then shuts down and deletes only that new simulator.
The iPad layout shard exercises four paging modes and five navigation modes,
exact anchors/returns and accepted remote positions without a new local edit.
The other shards cover UI features, selection and system Reduce Motion, as listed
below. These checks do not test actual account synchronization.
First boot and the initial Debug-app installation each have a separate ten-minute
preparation budget; normal reader launches retain their three-minute command
budget and native reports retain their own limits. No install or failed test is
retried by the collector. Failure artifacts also retain host memory, process
names, CPU and disk information to distinguish infrastructure stalls from UI
assertions.

An explicit `passed: true` report is required; missing, malformed, failed and stale
reports cannot count as success. Output directories must be new so earlier runs
remain distinct. `scripts/test-mobile-runtime.py` tests
report acceptance, missing-result timeout and compatible device selection.

Build and run Android checks on a local emulator:

```sh
(cd android && ./gradlew :app:assembleDebug :app:assembleDebugAndroidTest \
  -PREADER_DIAGNOSTIC_APPLICATION_ID=local.reader.columns)
python3 scripts/check-mobile-runtime.py android \
  --serial emulator-5556 \
  --app android/app/build/outputs/apk/debug/app-arm64-v8a-debug.apk \
  --tests android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk \
  --with-corpus --with-talkback --with-book-opening --with-selection \
  --output /absolute/new/android-evidence-directory
```

With full Xcode and the Rust iOS simulator target installed, build the local
XCTest products in a dedicated output directory:

```sh
xcodebuild -project ios/RustEpubReader.xcodeproj -scheme ReaderUITests \
  -configuration Debug -sdk iphonesimulator \
  -destination 'generic/platform=iOS Simulator' \
  -derivedDataPath target/ios-ui-local \
  CODE_SIGN_IDENTITY=- ARCHS=arm64 READER_APP_BUNDLE_ID=local.reader.columns \
  build-for-testing
```

Resolve the single manifest from that dedicated output directory:

```sh
READER_UI_MANIFEST="$(python3 -c 'import glob; p=glob.glob("target/ios-ui-local/Build/Products/ReaderUITests_*.xctestrun"); assert len(p)==1,p; print(p[0])')"
python3 scripts/check-mobile-runtime.py ios \
  --app target/ios-ui-local/Build/Products/Debug-iphonesimulator/ReaderPreview.app \
  --xctestrun "$READER_UI_MANIFEST" \
  --with-corpus --with-selection --timeout 240 \
  --output /absolute/new/ios-evidence-directory
```

Use a new evidence directory for every run. These checks preserve three
repetitions for each selection mode and system Reduce Motion; failed iterations
are not retried into passes. Run device portions serially on one Mac to avoid
competing for focus or settings. Simulator results do not establish physical-device
performance or acceptance.

The simulator build explicitly sets `READER_APP_BUNDLE_ID=local.reader.columns`;
release archives keep their normal bundle identifier. The iOS runner refuses a
non-diagnostic bundle before creating a simulator. The separate Files UI target
uses `local.reader.ui-files`, also allowed by the owned support-panel harness.
Local UI automation builds the simulator app and targetless XCTest runner together.
`ios-ci.py pack` records the source SHA, optional workflow identity, collector hash,
app/runner/manifest hashes and Xcode version. A tar archive preserves executable
bits and framework symlinks. The local collector checks that identity before
installing the app; the collector also checks that XCTest leaves the admitted
installation and input artifacts unchanged.

The collector supports five local portions, each owning a fresh simulator and
report directory:

| Shard | Device | Coverage |
| --- | --- | --- |
| `ipad-layout` | iPad | Single column, navigation and native typography/script corpus |
| `ipad-features` | iPad | All five TXT/service/contribution cases |
| `iphone-features` | iPhone | The same five feature cases at phone size |
| `ipad-selection` | iPad | All five selection modes, three repetitions each |
| `ipad-motion` | iPad | System Reduce Motion and restoration, three repetitions |

Run XCTest serially on a local host. Each invocation owns its app data and
reports and restores the system settings it changes. `xcresulttool` must report the exact
case set and all three successful repetitions; missing, skipped or failed cases
fail the shard. The selection shard still requires fifteen independent native
touch traces. Layout and corpus assertions are unchanged.

UI launches explicitly stop the collector's Settings readiness probe, wait for
it and the previous reader process to exit, then require the Home screen to
become foreground before launching and activating the reader. The probe's PID
alone does not prove its initial scene transition has finished. TXT source
search is resolved inside the destination navigation bar. Input acknowledgements
are checked immediately after a setup tap, including when XCTest delayed the
tap itself. Held-drag endpoints are computed from the paper frame before
touch-down and anchored to the app, avoiding deep paper lookups mid-gesture.
The feature process budget includes cold runner setup and XCResult finalization
(up to ten minutes); individual UI assertions keep their original deadlines.
A failure still fails the suite, and its native log tail is printed in the job
log even when result-bundle finalization times out.

For optional local build transport, `ios-ci.py pack`, `unpack` and `aggregate`
verify source/build identities and complete results. These commands are not
called by hosted workflows. Missing/failed portions, changed installations,
wrong runtime/device families, incomplete drawing and missing input traces fail
validation.

The original `--with-selection` command remains available for a complete serial
local run. `--ios-shard layout|features|selection|motion` and
`--ios-device-family ipad|iphone` select local portions. `--build-record`
requires an admitted transported build. `BackupFilesUITests` remains separate.
Local results apply only to the source and products actually tested. See the
[platform capability audit](PLATFORM_PARITY.md).

Pass `--with-corpus` for local painting checks. The committed typography and script
fixtures are rendered under all three style policies at widths320/640. Native
checks verify complete text pagination, image admission/decoding, table/note
semantics and actual drawing of representative/semantic pages. Android uses the
production `NativeChapterList`, native text measurer, image cache and Compose
block renderer; iOS uses the production chapter bridge, CoreText typesetter and
attachment painter. Declared layout coverage, positive page counts, drawn-page
bounds and screenshot names must agree. Evidence from a previous case cannot
satisfy the current screenshot list. Android crosses two display frames before
retaining a screenshot; screenshots are correctness evidence, not timing data.

Pass `--with-talkback` on a local Google APIs emulator image.
The installed TalkBack service must remain enabled alongside public
`UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES`. The test focuses actual
body text and invokes next/previous actions with exact return under all four
paged modes. The runner snapshots and restores both secure accessibility settings
in a `finally` block, including activation/native-check failures. Emulator/service
admission happens before mutation. This verifies coexistence/action dispatch, not
human acceptance of speech output. Actual speech output still needs device acceptance.

The typography corpus now uses base size 32 to retain the native ruby-line
overlap regression; its independent prelude still checks ordinary-size layout.
The collector rejects reports with the wrong or absent `base_font_size`.
This tests the reader's size control, not the device's system font-scale setting.

Android additionally provides the isolated `check=book-opening` instrumentation
entry for actual shelf touch → system Back → late-completion observation → same
book reopening. Supply an owned `cache/book-opening.epub`; the app namespace must
be `local.reader.columns`, and `cache/book-opening-runtime` must be new. It writes
its own summary and screenshots. `--with-book-opening` includes this functional
check in the local Android run. It is a functional check, not a Release timing
baseline. Native
`IOException` cancellation must not publish a stale opening-failed alert.

When TalkBack was not configured before the test, cleanup also closes the service
process the test activated, after restoring secure settings. This dismisses its
own pending onboarding activity without selecting a notification permission. An
initially configured service is retained, including full/short component names.
Opening input uses the existing read-only cover-bounds observer and confirms
reader entry before sending Back; it does not click a potentially obscured title
or accept a system dialog as an application screen.


Pass `--with-selection` for local native selection checks. Android injects actual touchscreen
long-press/drag events into all five native modes, invokes the visible highlight
and color actions, then independently rereads the JNI-persisted Unicode scalar
range. Realistic mode also performs a quick forward/reverse curl and checks the
rendered page label and chapter return exactly. A held press must not be stolen
by the curl gesture or committed as a page tap.

iOS builds the isolated `ReaderUITests` scheme with `build-for-testing`; pass its
single pinned `ReaderUITests_*.xctestrun` via `--xctestrun`. Public XCTest touches
the actual CoreText surface under five modes and verifies the production
annotation model's persisted excerpt. The manifest, app and runner are admitted
and fingerprinted. XCTest can reinstall the app: results must be collected from
the data container resolved after the test, not a cached pre-test path.

The local selection run also exercises the actual system animation policy. Android
snapshots the emulator's three global animation-scale settings, disables them,
repeats native selection and quick curl return, requires
`ValueAnimator.areAnimatorsEnabled() == false`, and restores each original or
absent setting in `finally`, including partial activation failure. iOS public
XCTest opens Settings → Accessibility → Motion → Reduce Motion in its disposable
simulator, enables the switch, checks all five native navigation/return modes,
checks the displayed reader layers for geometric animations, and restores the
original switch before exit. The diagnostic motion host first launches into an
idle ready screen; XCTest starts the window-changing suite with a native button
after launch completes. Each repetition requires its own run ID and all existing
geometry assertions. These are interaction/policy checks, not a motion
perception or physical accessibility acceptance claim. No physical-device system
settings or existing preview simulator are changed.


Android result collection retains the current screen even when a native report
fails validation or is missing. A screenshot failure is recorded and fails an
otherwise passing check; it cannot hide the original native failure. The owned
missing-fixture check confirms the raw failure report and PNG are both retained,
then restores the diagnostic fixture/report byte-for-byte. It does not inject a
fault into a user library. The runner regressions also cover partial settings
activation, missing reports and simultaneous native/screenshot failures.
