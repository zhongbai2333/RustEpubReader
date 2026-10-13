# Build and CI

## Workflow map

| Entry | Checks and output |
| --- | --- |
| `ci.yml`: main/dev pushes, PRs, manual | Rust fmt / locked metadata / strict Clippy / workspace tests; Android JVM tests and Release lint; all three platform builds |
| `desktop-build.yml`: reusable | Windows x64/arm64 ZIP, Linux x64/arm64 ZIP, macOS Intel/Apple Silicon DMG |
| `android-build.yml`: reusable | Three per-ABI Release APKs; publishing secrets are optional, otherwise development signing |
| `ios-build.yml`: reusable or manual | Rust core/bridge tests, native Swift layout/OAuth checks, ad-hoc signed arm64 simulator Debug app, unsigned arm64 device Release archive |
| `pr-review.yml`: `run-build` label | Reuses all three builds for the PR head SHA |
| `release.yml`: `v*` tag or manual tag | Requires all three builds; publishes desktop/Android assets. iOS validation artifacts remain in Actions |

Both `ios/**` and `ios-bridge/**` trigger CI. Shared Swift scheme and
`Package.resolved` belong in Git; local Xcode user state and runtime data do not.

## iOS environment and artifacts

The iOS workflow uses `macos-15` (arm64) and explicitly selects Xcode 26.3.
The [runner label table](https://github.com/actions/runner-images#available-images)
and [installed tools](https://github.com/actions/runner-images/blob/main/images/macos/macos-15-arm64-Readme.md)
are the source for this choice. Do not rely on the runner's default Xcode: the
source references SDK 26 APIs behind runtime availability checks. The minimum
deployment target remains iOS 17.

Rustup installs `aarch64-apple-ios` and `aarch64-apple-ios-sim`. The build phase
preserves the selected Rust toolchain and honors Cargo's target directory.
Homebrew's matching-source `build-std` path remains a local fallback.
ONNX Runtime comes from pinned Swift package 1.24.2; model weights remain optional
first-use downloads rather than app resources.

An iOS Actions artifact contains:

- `RustEpubReader-iOS-Simulator.zip`: Apple Silicon simulator `.app`, Debug,
  ad-hoc signed for simulator Keychain access.
- `RustEpubReader-iOS-UnsignedArchive.zip`: iPhone/iPad `.xcarchive`, Release,
  arm64, unsigned. It is not an installable IPA.
- `BUILD-INFO.txt`: source SHA, caller version label and tool versions.

Build logs upload separately even if a build fails. No Apple credentials are
required for these build checks. Signing, provisioning, IPA export and store
submission are not configured. The project has host Swift checks and Rust tests,
but no XCTest/UI test target. Compilation does not validate physical page-turn
gestures, background behavior or App Store acceptance.

## Local gates

Run from the repository root:

```sh
cargo fmt --all -- --check
cargo metadata --locked --format-version 1
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked --all --all-features
git diff --check
```

On macOS with full Xcode:

```sh
export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
ios/scripts/check-layout.sh
ios/scripts/check-oauth.sh
xcodebuild -project ios/RustEpubReader.xcodeproj -scheme ReaderPreview \
  -configuration Debug -sdk iphonesimulator \
  -destination 'generic/platform=iOS Simulator' \
  -derivedDataPath target/ios-local/Simulator CODE_SIGN_IDENTITY=- ARCHS=arm64 build
xcodebuild -project ios/RustEpubReader.xcodeproj -scheme ReaderPreview \
  -configuration Release -sdk iphoneos -destination 'generic/platform=iOS' \
  -derivedDataPath target/ios-local/Device \
  -archivePath target/ios-local/ReaderPreview.xcarchive \
  CODE_SIGNING_ALLOWED=NO ARCHS=arm64 archive
```

Android uses JDK 21 to align CI with the locally verified environment. The Java
bytecode target remains 17. With SDK configured, run from `android/`:

```sh
./gradlew :app:testDebugUnitTest :app:lintVitalRelease -x cargoBuild
./gradlew :app:assembleRelease
```

Skipping `cargoBuild` is limited to JVM tests/lint; it is never used for APK
packaging. Native builds use `--locked` and all three Android Rust targets.

## Commit boundary

The current platform work adds complete source directories. Include `ios/`,
`ios-bridge/`, the new core sync modules/tests, client UI files, workflows and
documentation in the source commit; staging tracked modifications alone misses
the new functionality. Preserve the source asset `ReaderIcon.png` and pinned
Swift package file.

Do not transfer `target/`, Gradle outputs, Android SDK paths, signing keys,
provisioning profiles, books, models, library/settings/journals or failed-transfer
queues into the source commit. GitHub tokens stay in device credential storage.
Local macOS development signing material is outside the repository and is not a
public release identity.

Local check results apply to their exact source snapshot. A new workflow has not
passed GitHub Actions until the source is committed/pushed and the resulting run
succeeds. Previous main-branch runs do not validate uncommitted work. Windows and
Linux runtime acceptance remains separate from builds and macOS-host tests.
