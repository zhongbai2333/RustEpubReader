# Pre-commit review — 2.0.0 preparation, 2026-10-17

Base: `main` at `07d2efca9443e78412426b6db3fe8a6b2afa6926`.
This review preserves the pre-existing version-tooling work and checks the full
workspace before the 2.0.0 release. Nothing was staged, committed, tagged, pushed,
or published. No application data, books, credentials, or build caches were removed.

## Version and fixes

- Public desktop, Android and iOS/iPadOS version: **2.0.0**.
- Android `versionCode`: **10**; iOS Debug/Release build: **1017.4**, above the
  recently installed development build 1017.3. Internal Rust library versions
  remain independent at 0.1.0. Cargo.lock matches the desktop package version.
- The cross-platform bump script updates all version fields, supports previews
  and build-only updates, validates starting consistency, and rolls back ordinary
  write failures. CI tests it; Release checks the tag before starting builds.
- Fixed a real update-notice bug: desktop and Android previously treated any
  different tag as newer. Numeric stable-version comparisons now reject older,
  identical, malformed and prerelease versions. iOS already compared versions.
- Desktop recognizes all six current ZIP/DMG package labels. Packaged updates
  open the official release page for user installation, with visible feedback;
  they never enter raw-executable replacement or executable-hash hotfix checks.
  Legacy raw replacement is retained for Windows/Linux. This review does not
  claim automatic ZIP extraction or DMG installation.
- Android chooses release APKs by the device's supported ABI order, rather than
  taking the first Android asset. With no compatible asset it opens the release
  page. Added regression cases for version ordering and ABI selection.
- Removed the hardcoded iOS release-version fallback and fixed the compiler's
  nested weak-capture warning in the appearance transition.
- Updated both root READMEs and the iOS notes to describe automatic accepted
  remote-position jumps and the return control accurately.

## Checks on this working tree

Evidence: `target/release-health-20261017/` (local, ignored).

| Check | Result / evidence |
| --- | --- |
| Version/tag consistency | Passed for `v2.0.0`, `versions-final.log` |
| Version updater | 8 passed; dry run, all increments, explicit versions, build-only updates, invalid inputs, CRLF, write rollback and concurrent edit preservation, `version-tests-final.log` |
| Rust formatting / locked metadata | Passed, `fmt-final.log` / `metadata.json` |
| Full workspace, all features | **243 passed, 0 failed, 4 ignored**, `rust-tests-final.log` |
| Strict workspace Clippy, all targets/features | Passed, `clippy-final.log` |
| Android JVM tests / Release lint | **44 passed, 0 failed/errors/skips**, lint passed, `android-checks-final.log` / `android-test-counts.json` |
| iOS host checks | Layout, OAuth, appearance, reading modes, speech, contents and localization passed, `swift-*.log` |
| Shared localization | **1012 keys, 1721 references, 0 errors**; seven invalid-input regressions rejected, `i18n-final.log` / `i18n-tests-final.log` |
| Font catalog | 20 direct-download entries/previews and two official-only entries passed, `fonts-final.log` |
| Unsigned IPA packager | 8 regressions passed, `ipa-tests.log` |
| Workflow validation | All workflows passed actionlint, `actionlint-final.log` |
| Local documentation links | 54 checked, no missing targets, `doc-links-final.json` |
| Bounded credential/source scan | No known token/private-key patterns or local credential/data candidates found; not a security certification, `candidate-scan-final.json` |
| macOS Release build | Passed, `desktop-release-final.log` |
| Android three-ABI Release APKs | Passed for armeabi-v7a / arm64-v8a / x86_64, `android-release-final.log` / `artifact-checks.json` |
| iOS simulator and unsigned device archive/IPA | Passed, `ios-simulator-final.log`, `ios-archive-final.log`, `ios-package-final.log`, `ios-package-verify.log` |

The final catalog wording was also checked by a targeted Rust i18n run
(`rust-i18n-final.log`) after the full workspace run.

The four opt-in Rust checks are cold-open profiling, a caller-supplied real-book
coordinate audit, a Release pagination benchmark, and a downloaded Chinese
corpus audit. They were not run in this review. Normal tests still cover native
pagination, long paragraphs/tables, illustrations, lazy chapters, sync conflicts,
archive lifecycle, chunked transfers, retry isolation, cancellation and restart
navigation. No new whole-book performance benchmark is claimed.

Initial local Android attempts used a missing SDK path and a PATH without
cargo-ndk. Both were environment setup failures; their logs are retained as
`*-attempt.log`. Final checks use the existing project-local SDK/toolchain.
Gradle reported that libandroid_bridge.so could not be stripped again; ELF
inspection confirmed the library already has no debug/symbol-table sections.
The iOS source warning was fixed; the remaining AppIntents extraction message
says the project has no AppIntents dependency.

## Hosted CI and acceptance boundary

The latest existing hosted [CI run](https://github.com/zhongbai2333/RustEpubReader/actions/runs/38994413513)
passed at the base commit above. It does **not** cover these uncommitted version,
updater, workflow, translation or documentation changes. Run hosted CI after
committing; Windows/Linux builds and runtime checks remain separate from this
macOS review. Local checks used Rust 1.98, JDK 21, Android NDK 29 and Xcode 27.2;
hosted CI selects NDK 26.1 and Xcode 26.3.

Prior native navigation runtime evidence is retained in
`target/reader-navigation-20261017/`, with an explicit source comparison in
`navigation-evidence-binding.json`. It is not a new 2.0.0 runtime run. The recent
physical iPhone/iPad installation evidence is for **1.0.8 (1017.3)**; this review
does not install 2.0.0 or claim physical acceptance. No live GitHub account sync,
real-device conflict session, new performance measurement, or user acceptance
was performed in this review.

Android APKs in this local review use the existing development-signing fallback;
the hosted release workflow can use its configured release signing secrets. The
generated iOS IPA is unsigned and requires the user's signing identity,
matching provisioning profile and re-signing of embedded frameworks. The existing
private ambient-light implementation is retained; this review does not establish
App Store readiness. Publisher signing, TestFlight, App Store upload and macOS
notarization are not part of this run.

Include the new `scripts/bump-version.py`, `scripts/test-bump-version.py`, Android
`ReleaseVersion.kt` and `ReleaseVersionTest.kt` when committing; updating only
tracked files would omit them. Exclude local build/evidence files and runtime
data. After committing the reviewed tree, wait for its CI result before creating
and publishing the `v2.0.0` release tag. Final source and artifact fingerprints are
recorded in `source-final.json` and `verification.json` under the evidence path.
