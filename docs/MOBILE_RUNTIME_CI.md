# Native mobile smoke checks

The Android and iOS reusable build workflows run `scripts/check-mobile-runtime.py`
after building the native application. These checks use the existing synthetic
`native-reading-modes.epub`, not a user library, credentials or a live GitHub
account. Reports and screenshots are uploaded even when checks fail.

Android builds a separate `local.reader.columns` debug app and instrumentation
APK. The runner checks APK package names and requires an emulator before installing.
It exercises imports, automatic/forced columns, reflow anchors, persisted settings,
JNI storage failures/recovery, navigation/return, remote-position acceptance,
reader reopening after activity recreation, explicit close, and continuous mode.
The workflow boots an API 35 x86_64 tablet with KVM. Native checks are not bundled
into the published Release APK.

iOS creates its own iPad Simulator, choosing a device type whose supported system
range includes the installed runtime. It installs the debug simulator app, runs
column and navigation checks, collects results, then shuts down and deletes only
that newly created simulator. The tests exercise four paging modes and five
navigation modes, exact anchors/returns and accepted remote positions without a
new local edit. This does not test actual account synchronization.

An explicit `passed: true` report is required; missing, malformed, failed and stale
reports cannot count as success. Output directories must be new so earlier runs
remain distinct. Android Cargo caches are limited to build directories and do not
retain smoke reports or emulator state. `scripts/test-mobile-runtime.py` tests
report acceptance, missing-result timeout and compatible iPad selection.

Local reproduction (build the isolated app first):

```sh
python3 scripts/check-mobile-runtime.py android \
  --serial emulator-5556 \
  --app android/app/build/outputs/apk/debug/app-arm64-v8a-debug.apk \
  --tests android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk \
  --output /absolute/new/evidence-directory

python3 scripts/check-mobile-runtime.py ios \
  --app target/ios-ci/Simulator/Build/Products/Debug-iphonesimulator/ReaderPreview.app \
  --timeout 240 --output /absolute/new/evidence-directory
```

Both runners passed locally on macOS/arm64. Hosted Ubuntu x86_64 Android and the
workflow's selected Xcode runtime still require an actual hosted run. Simulator
checks do not establish physical-device performance or acceptance.

The simulator build explicitly sets `READER_APP_BUNDLE_ID=local.reader.columns`;
release archives keep their normal bundle identifier. The iOS runner refuses a
non-diagnostic bundle before creating a simulator. The separate Files UI target
uses `local.reader.ui-files`, also allowed by the owned support-panel harness.

Both workflows now enable `--with-corpus`. The committed typography and script
fixtures are rendered under all three style policies at widths320/640. Native
checks verify complete text pagination, image admission/decoding, table/note
semantics and actual drawing of representative/semantic pages. Android uses the
production `NativeChapterList`, native text measurer, image cache and Compose
block renderer; iOS uses the production chapter bridge, CoreText typesetter and
attachment painter. Declared layout coverage, positive page counts, drawn-page
bounds and screenshot names must agree. Evidence from a previous case cannot
satisfy the current screenshot list. Android crosses two display frames before
retaining a screenshot; screenshots are correctness evidence, not timing data.

Android additionally enables `--with-talkback` on the Google APIs emulator image.
The installed TalkBack service must remain enabled alongside public
`UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES`. The test focuses actual
body text and invokes next/previous actions with exact return under all four
paged modes. The runner snapshots and restores both secure accessibility settings
in a `finally` block, including activation/native-check failures. Emulator/service
admission happens before mutation. This verifies coexistence/action dispatch, not
human acceptance of speech output. Hosted execution remains a separate gate.

The typography corpus now uses base size 32 to retain the native ruby-line
overlap regression; its independent prelude still checks ordinary-size layout.
The collector rejects reports with the wrong or absent `base_font_size`.
This tests the reader's size control, not the device's system font-scale setting.

Android additionally provides the isolated `check=book-opening` instrumentation
entry for actual shelf touch → system Back → late-completion observation → same
book reopening. Supply an owned `cache/book-opening.epub`; the app namespace must
be `local.reader.columns`, and `cache/book-opening-runtime` must be new. It writes
its own summary and screenshots. `--with-book-opening` includes this functional
gate in the Android workflow; hosted execution is still unverified. It is not a
Release timing baseline. Native
`IOException` cancellation must not publish a stale opening-failed alert.

When TalkBack was not configured before the test, cleanup also closes the service
process the test activated, after restoring secure settings. This dismisses its
own pending onboarding activity without selecting a notification permission. An
initially configured service is retained, including full/short component names.
Opening input uses the existing read-only cover-bounds observer and confirms
reader entry before sending Back; it does not click a potentially obscured title
or accept a system dialog as an application screen.


Both workflows enable `--with-selection`. Android injects actual touchscreen
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

The selection gate also exercises the actual system animation policy. Android
snapshots the emulator's three global animation-scale settings, disables them,
repeats native selection and quick curl return, requires
`ValueAnimator.areAnimatorsEnabled() == false`, and restores each original or
absent setting in `finally`, including partial activation failure. iOS public
XCTest opens Settings → Accessibility → Motion → Reduce Motion in its disposable
simulator, enables the switch, checks all five native navigation/return modes,
checks the displayed reader layers for geometric animations, and restores the
original switch before exit. These are interaction/policy checks, not a motion
perception or physical accessibility acceptance claim. No physical-device system
settings or existing preview simulator are changed.


Android result collection retains the current screen even when a native report
fails validation or is missing. A screenshot failure is recorded and fails an
otherwise passing check; it cannot hide the original native failure. The owned
missing-fixture check confirms the raw failure report and PNG are both retained,
then restores the diagnostic fixture/report byte-for-byte. It does not inject a
fault into a user library. The runner regressions also cover partial settings
activation, missing reports and simultaneous native/screenshot failures.
