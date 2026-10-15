# Pre-commit review — 2026-10-15

Base: `main` at `514f3e093b209f5df0bc646273b11f765d09b71f`.
This review covers modified tracked files and all nonignored new source/assets.
Nothing was staged, committed, pushed or published. Existing working changes were
preserved. Applications and emulators were not started for this review.

**Conclusion:** the local checks support committing this work for Windows and
hosted-CI testing. They do not establish Windows/Linux runtime acceptance, user
self-signing/physical iOS installation, or App Store readiness. The existing iOS
private ambient-light API is a store-distribution finding; see below.

## Current change scope

| Group | Included changes |
| --- | --- |
| Shared font library | 20 downloadable OFL fonts (10 Chinese, 10 English), pinned URLs/checksums, full license notices, 20 offline specimen images, and two official-only personal-use entries |
| Native clients | Font import/download/selection on desktop and Android; iOS app-local import, system font picker and restored font registration; desktop/iOS optional English override with persistent Follow Chinese Font selection |
| Regressions | Desktop named font families reference only loaded fonts, including headless fallback; punctuation checks cover bundled fallback; shelf-animation tests use an explicit time sample |
| iOS appearance | Deeper-dark threshold and asymmetric dwell times, with initial middle-band behavior and regression checks; existing appearance linkage retained |
| Unsigned IPA | Device-archive packaging and verification, eight portable regressions, Actions artifacts, tag Release attachments/checksums, and self-signing instructions |
| Documentation/CI | Font behavior and licensing, build/distribution instructions, updated checks, and Windows acceptance checklist |

Font binaries, books, models, credentials, SDKs, local settings and build outputs
are not source candidates. The 20 PNG masks total **726,232 bytes**; real fonts
remain optional downloads. Android retains one reading-font selection for both
scripts; desktop and iOS have separate Chinese/English slots.

## Corrections made in this review

- Formatted the new font-picker actions in `desktop/src/ui/settings_new.rs`.
  The initial formatting gate failed on those lines; the final gate passed.
- Added `resources/**` to both push and pull-request path filters in CI.
  Font-catalog, license or specimen-only changes now trigger validation/builds.
- Replaced the accumulated report's stale counts and broad snapshot claims with
  the current results below. The prior report is retained with local evidence.
- Extended the Windows checklist for the new font flow and English override.

## Checks run against the current source

All logs below are under `target/precommit-current-20261015/` (ignored locally).

| Check | Current result / log |
| --- | --- |
| Rust formatting | Passed, `fmt-final.log` |
| Locked offline Cargo metadata | Passed, `metadata.json` / `metadata.log` |
| Strict Clippy, workspace/all targets/all features | Passed, `clippy.log` |
| Workspace tests, all features | **184 passed, 0 failed, 1 ignored**, `rust-tests.log`; the existing opt-in real-book corpus check remains ignored in this run |
| Font catalog and actual downloaded files | 20 pinned binaries, all sizes/hashes/licenses verified; 20 specimen masks and two official-only entries passed, `catalog.log` |
| Shared localization | **955 keys**, 1,584 references, zero errors; seven invalid-input regressions rejected, `i18n.log` / `i18n-tests.log` |
| Android JVM tests | **37 passed, zero failures/errors/skips** across 11 suites, `android-checks.log` / `test-counts.json` |
| Android Release lint | Passed in the Gradle check; native bridge build excluded from this check, `android-checks.log` |
| iOS host checks | All seven groups passed: layout, OAuth, appearance, reading mode, speech, contents and localization, `swift-*.log` |
| Unsigned IPA packaging regressions | **8 passed**, `ipa-tests.log` |
| Workflow syntax/expressions | Passed with actionlint, `actionlint.log` |
| Local documentation links and whitespace | Passed, `doc-links.json` / `diff-check.log` |
| Candidate scan | No known token/private-key pattern matches or local credential/data candidates; bounded scan only, `candidate-scan.json` |

Rust tests include the three new desktop font-library tests with all 20 actual
font files supplied through `READER_FONT_FIXTURES`. No live GitHub account or
three-device sync acceptance is inferred from unit/protocol checks.

The review used macOS, Rust 1.98, JDK 21 and the installed local Android SDK;
Swift checks used Xcode 27.2. Hosted CI selects Xcode 26.3 and a different Android
NDK. Passing locally does not claim a run on those hosted versions.

## Earlier build/runtime evidence and source binding

The immediately preceding font-library verification includes real macOS Release,
three Android ABI Release APKs, iOS device Release archive and simulator builds,
as well as native font-import/rendering checks and font-setting persistence.
Those records remain in `target/font-preview-20-20261015/`; they were not rerun
as application builds or UI sessions for this review.

Comparison with its 76-file final source manifest found changes only in CI, this
report, and the desktop settings formatting fixed above. All other recorded
native source/assets still match. Current compilation/Clippy/tests cover the
formatted desktop source. This is an explicit evidence comparison, not a claim
that an older artifact was rebuilt from the final review manifest.

The IPA follow-up records in `target/ios-unsigned-ipa-ci-20261015/` verify the
actual workflow packaging/supporting-artifact commands using the existing real
arm64 Release archive, all 57 application/framework/resource files, checksum
verification and Release asset patterns. Its 13-file source manifest differs
only in the CI resource-trigger addition. The resulting IPA is about 13.10 MB.
No GitHub upload, hosted Actions run, signing or physical installation occurred.

Other retained evidence:

- `target/ci-font-fallback-20261015/`: reproduced missing-font panic, fix, actual
  egui renders across eight font-availability combinations and deterministic
  shelf-animation regression.
- `target/ios-ambient-sensitivity-20261015/`: policy checks, shared linkage and
  native simulator checks with injected readings. Physical sensor calibration
  and subjective room-light acceptance remain pending.
- `target/precommit-20261015-final-review/` and
  `target/precommit-expert-review-20261015/`: older broad gates and independent
  copy/source-evidence review. Their old test counts are not current results.

## Open finding: private iOS ambient-light APIs

[IOHIDAmbientLightReader.swift](../ios/Reader/IOHIDAmbientLightReader.swift)
loads IOKit and private IOHID functions on physical devices in Debug and Release;
simulator builds return unavailable. Policy input uses native readings rather
than calibrated lux. This review preserves the existing product behavior.

[Apple App Review Guideline 2.5.1](https://developer.apple.com/app-store/review/guidelines/#software-requirements)
requires public APIs (rechecked on 2026-10-15). A public-API-only distribution
variant or a product decision is needed before treating this implementation as
App Store ready. An unsigned build or paid developer membership does not settle
this finding.

## Submission and acceptance boundary

Suggested commit groups, without staging any files:

1. Shared font catalog/assets, all three native client integrations, localization,
   font-specific tests and [font documentation](FONTS.md). Include all new files;
   a tracked-modifications-only commit will omit required modules/resources.
2. Desktop headless font fallback, punctuation regression and deterministic shelf
   animation tests.
3. iOS ambient policy and its checks/documentation.
4. Unsigned IPA packager/tests, CI/Release integration and signing/build docs.

Shared documentation and CI hunks overlap these groups; keep the final font
resource-trigger fix and synchronized review/checklist with the completed set.
Avoid committing a partially split state that references absent new modules.

Before release acceptance:

- Run the updated hosted CI, including Linux tests and Windows x64/arm64 packages.
  Follow [Windows testing](WINDOWS_TESTING.md) for DPI, fonts, reading, credentials,
  sync/share and normal exit behavior. No Linux/Windows host success is claimed.
- Test user self-signing and physical installation, including embedded frameworks,
  matching bundle ID/keychain access groups and the Fonts app-usage entitlement.
  See [unsigned IPA instructions](../ios/UNSIGNED_IPA.md). Automatic signing,
  TestFlight/App Store publishing and macOS notarization remain separate work.
- Exercise physical iOS sensor behavior, supported/minimum OS versions, real
  authorization and cross-device transfers. Prior successful local font checks
  are narrower evidence; a prior Android menu ANR recovered after preview restart
  has no established cause or independently verified fix.

The two previously owned keepalive preview jobs were removed before this review;
their registrations and processes were checked absent. Do not recreate them for
manual previews. Source fingerprints and machine-readable results for this review
are saved as `source-final.json` and `verification.json` in the current evidence
directory. No cleanup of user data or build caches was performed.
