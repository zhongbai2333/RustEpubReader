# Pre-commit review — 2026-10-15

This review covers the current working tree, including modified tracked files and
new source files. No staging, commit, push, release publication or persistent
preview auto-start task was performed. Existing unrelated working changes were
preserved.

Local build/test gates passed. **The current iOS Release implementation is not
App Store ready** because it includes private ambient-light APIs; see the open
finding below. Compilation is not cross-platform runtime acceptance.

## Documentation and contribution entry points

- Rewrote the Chinese [README](../README.md), added the matching
  [English README](../README.en.md), and added a bilingual
  [contribution guide](../CONTRIBUTING.md).
- Moved detailed account/sync documentation into [GitHub sync](GITHUB_SYNC.md).
  Corrected its stale statement about statistics: daily per-device reading and
  listening activity now participates in synchronization. Annotations and
  settings remain outside that schema.
- Added iOS/iPadOS platform and affected-area choices, device/system/install
  details, and simulator/Xcode guidance to the Issue form. Updated the automatic
  issue triage context and PR checklist to include iOS and shared bridge changes.
- Chinese drafts received model-assisted copy review before formal writes;
  final terminology, links and claims were checked against source. Review
  suggestions were not treated as feature evidence or human acceptance.

## Fixes made while checking

- Applied workspace Rust formatting and fixed strict Clippy failures. Removed
  unreachable legacy background-image/account-summary UI helpers, retained the
  active settings/account flows, clarified cache types, and moved pagination
  test definitions after implementation. Narrow argument-count exceptions keep
  the shared heartbeat ABI and explicit swatch parameters intact.
- Isolated Android ICU title boundaries behind a function parameter. Production
  keeps Android ICU's strict Chinese boundaries; JVM position-conservation tests
  use real JVM boundaries instead of invoking an unavailable Android stub. This
  does not constitute device coverage of Android ICU.
- Made new iOS host-check scripts executable, supplied localization dependencies,
  and bundled real catalogs for deterministic Chinese TOC fallback checks. Added
  all seven native check groups to iOS CI.
- Added CI triggers for the patched `vendor/` dependency. Removed Android signing
  secret inheritance from ordinary CI and PR label builds. Controlled release
  signing remains available through the controlled release workflow, including its
  manual dispatch. The reusable Android workflow has no direct dispatch entry.
- Ignored device-local reading activity, sharing history and temporary sharing
  files, including their actual atomic-write temporary names. No source-candidate
  file matched the bounded credential/private-key scan.

## Verification on this source

These results come from local runs on this Mac. Updated hosted workflows and
platform runtime acceptance are not included; see the limitations below.

| Check | Result |
| --- | --- |
| `git diff --check` | Passed |
| Locked Cargo metadata, full dependency graph | Passed offline |
| Workspace Rust formatting | Passed with Rust 1.98 and stable 1.99 |
| Strict workspace Clippy, all targets/features | Passed with Rust 1.98 and stable 1.99 |
| Workspace tests, all features | 180 passed; one corpus test intentionally excluded from the default run |
| Explicit Chinese corpus test | Passed separately: 24 reflows over four books, three styles and two widths; source anchors preserved |
| Translation validation and checker regression tests | Passed: 911 shared keys; seven invalid-catalog/source cases rejected |
| Desktop optimized build | macOS `release-fast` build passed |
| Android JVM tests | 37 passed, zero failures |
| Android Release lint | Passed |
| Android native/Release APK build | Passed with freshly compiled armeabi-v7a, arm64-v8a and x86_64 bridges |
| Android APK validation | All three signatures verified; 16 KB alignment checks passed |
| iOS host checks | Layout, OAuth, appearance, reading mode, speech, contents and localization passed |
| iOS arm64 simulator Debug build | Passed with ad-hoc signing |
| iOS arm64 device Release archive | Passed unsigned; not an installable IPA |
| Workflow syntax/expression checks | Passed with actionlint 1.7.12 |
| Local documentation links | Passed |
| Source-candidate credential/local-data scan | No known token/private-key patterns or local credential/data candidates found; not a comprehensive security audit |

The Rust test total is 180 plus the separately invoked corpus test. Host workspace
tests and desktop/Apple builds used Homebrew Rust 1.98.0; formatting/strict Clippy
were additionally verified with the isolated stable Rust 1.99.0 toolchain used for
Android native builds. Android checks used JDK 21, SDK 35 and Build Tools 36.0.0;
local native packaging used NDK 29.0.14206865. CI still pins NDK 26.1.10909125.
Local Apple builds used Xcode 27.2; CI selects Xcode 26.3. These local results do
not claim that the exact hosted toolchain versions have run on this new tree.

Logs, source/artifact fingerprints, the unsigned archive, copy-review records,
and the machine-readable result are in the ignored local directory
`target/precommit-20261015-final-review/`. `verification.json` and
`source-sha256.json` identify the checked snapshot; these files are not source
commit candidates.

## Open finding: private iOS ambient-light APIs

[IOHIDAmbientLightReader.swift](../ios/Reader/IOHIDAmbientLightReader.swift)
dynamically loads IOKit and resolves private IOHID event functions for physical
devices. This implementation is intentionally included in Debug and Release;
simulator builds return unavailable instead. A successful unsigned device archive
does not validate the private API at runtime or establish store eligibility.

[Apple's App Review Guidelines, section 2.5.1](https://developer.apple.com/app-store/review/guidelines/#software-requirements)
require public APIs. Therefore a public-API-only distribution variant or another
explicit product decision is required before treating this source as App Store
ready. This review documents the existing behavior rather than silently removing
the ambient-light feature.

## Remaining acceptance and commit boundary

- Windows/Linux builds and runtime behavior were not rerun on this Mac. Follow
  [Windows testing](WINDOWS_TESTING.md), then run the updated hosted workflows.
- Physical iOS devices, minimum iOS 17/iOS 26 runtimes, real account authorization,
  real three-device sync/share round trips, and Android Release runtime behavior
  were not accepted in this review. Host tests, local protocol tests and packaged
  artifacts are narrower evidence.
- iOS signing/provisioning, IPA export and TestFlight/App Store publication are
  not configured. Public macOS signing/notarization remains separate work.
- Commit the new source along with tracked modifications: in particular the
  shared modules/fixtures, native UI files, `scripts/`, `vendor/epaint`, new iOS
  scripts, README languages and contribution guide. A tracked-files-only commit
  omits required code. Keep local libraries, journals, books, SDK configuration,
  model caches, signing files and build artifacts outside the source commit.


## Independent expert follow-up — 2026-10-15

Three configured external models completed an independent copy/source-evidence
review of 15 selected final documents, workflows, scripts and source excerpts.
Their replies were checked against the repository; this is model-assisted review,
not a full security audit, visual review or human runtime acceptance.

- Aligned local and CI Rust checks with explicit `--locked --workspace`; added
  the missing translation checks to the build guide. `--all` was already a
  workspace alias, so this does not claim to fix previously missing tests.
- Matched CI cargo-ndk installation to the documented `--locked` command. This
  locks the selected tool release's dependencies, not the tool version itself.
- Limited PR workflow write permission to the final comment/label job and removed
  the unnecessary unrelated-label cancellation action. Build jobs remain read-only.
- Corrected the nonexistent direct Android manual-dispatch entry and clarified
  Android's Keystore-key-encrypted token storage in both root README languages.
- Added LLVM/libclang prerequisites, explicit Xcode selection, matching English
  platform check commands, and the complete seven-group iOS CI description.
- Declined unsupported suggestions that host tests are only static checks,
  supported catalogs are missing, English Listening terminology is incorrect, or
  virtual-workspace Clippy necessarily fails. Existing private IOHID and untested
  hosted/platform-runtime findings remain open.

The application source content still matches the earlier checked snapshot; this
follow-up changes only documentation and workflows. Strict workspace Clippy and
formatting were rerun; workflow lint, documentation paths/anchors, translation
checks and whitespace checks passed. No application rebuild or runtime acceptance
is inferred from these edits. The earlier build results remain tied to the
original source manifest, while the updated workflow still needs a hosted run.

Full replies, decisions, the exact input, both source manifests, and a supplemental
`verification.json` indexing the previously omitted metadata/format/i18n/diff logs
are saved locally in `target/precommit-expert-review-20261015/`. Nothing was staged,
committed, pushed, published, or registered as an auto-start preview.
