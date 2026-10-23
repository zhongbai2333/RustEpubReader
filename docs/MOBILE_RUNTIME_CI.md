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

Each iOS runtime job creates its own iPhone or iPad Simulator, choosing a device
type compatible with the selected runtime. It installs the shared debug build,
executes its assigned checks, then shuts down and deletes only that new simulator.
The iPad layout shard exercises four paging modes and five navigation modes,
exact anchors/returns and accepted remote positions without a new local edit.
The other shards cover UI features, selection and system Reduce Motion, as listed
below. These checks do not test actual account synchronization.

An explicit `passed: true` report is required; missing, malformed, failed and stale
reports cannot count as success. Output directories must be new so earlier runs
remain distinct. Android Cargo caches are limited to build directories and do not
retain smoke reports or emulator state. `scripts/test-mobile-runtime.py` tests
report acceptance, missing-result timeout and compatible device selection.

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

Local macOS/arm64 passes have been recorded. Hosted results must be checked
against their exact source commit, runner and selected runtime; local results do
not establish a hosted pass for later source. Simulator checks do not establish
physical-device performance or acceptance.

The simulator build explicitly sets `READER_APP_BUNDLE_ID=local.reader.columns`;
release archives keep their normal bundle identifier. The iOS runner refuses a
non-diagnostic bundle before creating a simulator. The separate Files UI target
uses `local.reader.ui-files`, also allowed by the owned support-panel harness.
The Apple workflow compiles the simulator app and targetless XCTest runner once.
`ios-ci.py pack` records the source SHA, workflow run/attempt, collector hash,
app/runner/manifest hashes and Xcode version. A tar archive preserves executable
bits and framework symlinks. Each runtime host checks that identity before
installing the app; the collector also checks that XCTest leaves the admitted
installation and input artifacts unchanged.

The runtime matrix uses five separate hosts, each with its own simulator and
report directory:

| Shard | Device | Coverage |
| --- | --- | --- |
| `ipad-layout` | iPad | Single column, navigation and native typography/script corpus |
| `ipad-features` | iPad | All five TXT/service/contribution cases |
| `iphone-features` | iPhone | The same five feature cases at phone size |
| `ipad-selection` | iPad | All five selection modes, three repetitions each |
| `ipad-motion` | iPad | System Reduce Motion and restoration, three repetitions |

XCTest runs serially inside each host. GUI focus, Settings state, app data and
report files are never shared between hosts. `xcresulttool` must report the exact
case set and all three successful repetitions; missing, skipped or failed cases
fail the shard. The selection shard still requires fifteen independent native
touch traces. Layout and corpus assertions are unchanged.

`ios-ci.py aggregate` rejects missing/failed shards, another source/build,
changed installations, wrong device families/runtime versions, missing native
input traces and incomplete drawing coverage. A successful earlier build from
the same workflow run can be reused when rerunning failed jobs; artifacts are
replaced by name, and all receipts must still match that build. Final artifact
collection also requires native logic checks and Release packaging to succeed.

The original `--with-selection` command remains available for a complete serial
local run. `--ios-shard layout|features|selection|motion` and
`--ios-device-family ipad|iphone` select isolated CI portions. `--build-record`
requires an admitted transported build. `BackupFilesUITests` remains separate.
Local passes and configuration changes do not establish a hosted result. See the
[platform capability audit](PLATFORM_PARITY.md).

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
gate in the Android workflow. It is a functional check, not a Release timing
baseline. Native
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
