# Pre-commit review — 2026-10-14

The local source is ready for the Windows acceptance stage. No commit, staging,
push or release publication was performed during this review.

## Changes completed in this review

- Added reusable/manual iOS CI and connected it to ordinary CI, PR label builds
  and release validation. Added iOS source/workflow trigger paths.
- Selected the documented Apple Silicon runner and Xcode 26.3, installed Rust
  iOS targets, preserved the chosen Rust toolchain in Xcode phases, and pinned
  Swift dependency resolution. Build pipelines preserve failures through log pipes.
- Added Android JVM tests to CI, aligned the build JDK to 21, and made native
  Android builds use the Cargo lockfile.
- Resolved strict Rust lint/format failures, documented C ABI ownership/safety,
  and removed the unused desktop contribution-only login entry. The active
  unified repository login remains covered by the OAuth intent tests.
- Added ignores for device-local failed-transfer queues, Xcode user state,
  signing material and Python caches. Kept Unix script line endings stable across
  Windows edits and made the Gradle wrapper executable.
- Updated platform/build/account documentation and added Windows acceptance steps.

## Current local verification

| Check | Result |
| --- | --- |
| `git diff --check` | Passed |
| Locked Cargo metadata | Passed |
| Rust formatting | Passed |
| All targets/features strict Clippy | Passed on macOS |
| Workspace Rust tests | 107 passed: Android bridge 5, core 39, iOS bridge 12, desktop 51 |
| Android JVM tests | 16 passed |
| Android Release lint | Passed |
| Desktop `release-fast` build | Passed on macOS |
| Android Release packaging | Passed for armeabi-v7a, arm64-v8a and x86_64 |
| iOS host Swift layout/OAuth checks | Passed; OAuth tests use simulated responses |
| iOS arm64 simulator Debug build | Passed with ad-hoc signing |
| iOS arm64 device Release archive | Passed unsigned |
| Workflow syntax/expression checks | Passed with actionlint 1.7.12 |
| Known credential/private-key pattern scan | No matches in source candidates; not a comprehensive security audit |

The local Apple builds use Xcode 26.0 and Homebrew Rust 1.98.0's matching-source
fallback. CI selects Xcode 26.3 and rustup's prebuilt iOS standard libraries.
Those exact hosted jobs still require their first run after commit/push. Earlier
successful main-branch CI runs refer to older committed source.

Local logs, source/artifact fingerprints and the unsigned archive are under
`target/` and are intentionally outside the source commit. The snapshot report is
`target/precommit-review/verification.json`.

## Remaining acceptance and distribution boundaries

- Windows/Linux runtime behavior and the Windows x64/arm64 builds were not run
  on this Mac. Use [Windows final testing](WINDOWS_TESTING.md) for the next stage.
- This review reran automated checks and builds, not the entire three-client
  manual UI or real-account cross-device synchronization suite.
- iOS CI provides a simulator application and unsigned archive, not an IPA.
  Signing/provisioning and TestFlight/App Store publishing are not configured.
- Public macOS builds still need publisher signing/notarization configuration.
  Local development signing is documented separately and must not be copied to CI.
- Existing desktop and mobile preview processes were preserved.

Include all new source files with the modified tracked files when committing;
especially `ios/`, `ios-bridge/`, sync modules/tests and `.github/workflows/ios-build.yml`.
Do not move credentials, local journals, books, model caches or signing files to
Windows as part of the source checkout. Windows requires its own GitHub login.
