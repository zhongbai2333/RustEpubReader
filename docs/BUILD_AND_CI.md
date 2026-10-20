# Build and CI

## Workflow map

| Entry | Checks and output |
| --- | --- |
| `ci.yml`: main/dev pushes, PRs, manual | Rust fmt / locked metadata / strict Clippy / workspace tests; native Windows core safety tests; Android JVM tests and Release lint; all three platform builds |
| `desktop-build.yml`: reusable | Windows x64/arm64 ZIP, Linux x64/arm64 ZIP, macOS Intel/Apple Silicon DMG |
| `android-build.yml`: reusable | Three per-ABI Release APKs; publishing secrets are optional, otherwise development signing |
| `ios-build.yml`: reusable or manual | Rust core/bridge tests, native Swift checks, IPA packaging regressions, ad-hoc signed arm64 simulator Debug app, unsigned arm64 device Release archive and IPA with SHA-256 receipt |
| `pr-review.yml`: `run-build` label | Reuses all three builds for the PR head SHA |
| `release.yml`: `v*` tag or manual tag | Requires all three builds; publishes desktop/Android assets and the unsigned iOS IPA, checksum, signing instructions and build information. iOS simulator/archive artifacts remain in Actions |

Both `ios/**` and `ios-bridge/**` trigger CI. Shared Swift scheme and
`Package.resolved` belong in Git; local Xcode user state and runtime data do not.

## Linux desktop dependencies

The release ZIP contains the reader executable and relies on system GTK, audio,
keyboard and graphics libraries. A build passing without starting a window does
not check runtime-loaded libraries. On Ubuntu24.04, install:

```sh
sudo apt-get install libgtk-3-0t64 libasound2t64 libxkbcommon-x11-0 \
  libegl1 libegl-mesa0 libgl1 libglx-mesa0 libgl1-mesa-dri \
  fonts-noto-cjk fonts-noto-color-emoji
```

`libxkbcommon-x11-0` is needed by the X11 native-window backend; installing only
`libxkbcommon-dev` does not provide it. CJK/emoji fonts supply system fallbacks.
For source builds, also install the development libraries used by the Linux CI
steps: GTK3/GLib/ATK, ALSA, XCB render/shape/xfixes, xkbcommon, EGL/OpenGL, a C/C++
compiler and `pkg-config`. Package names vary across distributions.

The local Ubuntu ARM64 check uses an isolated source snapshot and Xvfb/Openbox
with Mesa software rendering. That verifies native window/layout behavior on
that guest; it is separate from a physical GPU, other distributions and a hosted
job. Its evidence keeps the missing-XKB launch failure as well as the subsequent
run after installing the required runtime package.

## App versions

Use Python 3.9 or newer from any platform; no extra packages are needed:

```sh
python3 scripts/bump-version.py --check
python3 scripts/bump-version.py patch --dry-run
python3 scripts/bump-version.py patch       # 1.0.8 -> 1.0.9
python3 scripts/bump-version.py minor       # increment minor, reset patch
python3 scripts/bump-version.py major       # increment major, reset minor/patch
python3 scripts/bump-version.py 1.2.0       # choose a greater stable X.Y.Z
python3 scripts/bump-version.py --build-only
```

Each update changes the workspace app version, the desktop's entry in
`Cargo.lock`, Android `APP_VERSION_NAME` / `APP_VERSION_CODE`, and all iOS
`MARKETING_VERSION` / `CURRENT_PROJECT_VERSION` settings. Build numbers increment
automatically; internal Rust library versions and locked dependencies stay as
they are. `--build-only` keeps the displayed app version. `--dry-run` shows the
diff without writing. The script rejects inconsistent starting versions,
downgrades, and missing fields before writing; ordinary write failures roll back
completed writes. Review the resulting diff before committing.

If a local test installation used build-number overrides, pass greater numbers
explicitly, for example:

```sh
python3 scripts/bump-version.py --build-only --ios-build 1017.4 --android-code 10
```

The defaults come from committed project files, not connected devices or previous
build outputs. Xcode/Gradle command-line overrides still take precedence when
building. This script does not build, sign, install, commit, tag, or publish.

CI checks version consistency and tests the updater. Release builds first require
the tag to equal the source version, using
`python3 scripts/bump-version.py --check --tag v1.0.8`. Create/push that tag only
after committing the intended version changes. Historical examples, test fixtures
and third-party versions are not updated by the script.

Desktop and Android update notices compare numeric stable versions, so an older
published release is not offered to a newer test build. Android chooses an APK
matching the device's supported ABIs; if no match exists, it opens the release
page. Desktop ZIP/DMG updates open the official release page for installation by
the user, with an inline confirmation. Packaged archives are never used as raw
executable replacements or compared against the installed executable for hotfixes.
Legacy raw executable replacement remains available on Windows/Linux.

## iOS environment and artifacts

The iOS workflow uses `macos-26` (arm64) and explicitly selects Xcode 26.4.1
and an installed iOS 26.4 runtime. It fails if that runtime is absent rather
than silently selecting another version from the runner image.
The [runner label table](https://github.com/actions/runner-images#available-images)
and [installed tools](https://github.com/actions/runner-images/blob/main/images/macos/macos-26-arm64-Readme.md)
are the source for this choice. Do not rely on the runner's default Xcode: the
source references SDK 26 APIs behind runtime availability checks. The minimum
deployment target remains iOS 17.

Xcode 26.4 includes fixes for Simulator cache creation and `simctl` hangs
([Apple release notes](https://developer.apple.com/documentation/xcode-release-notes/xcode-26_4-release-notes),
170836966 and 172343027). Runtime cache preparation and a system-app launch
probe run before admitting the reader tests; the reader's own launch or test
failures remain failures. Different Xcode/host combinations use separate caches.
The disposable iPad's first boot has a 600-second budget for initial system
data migration. Reader launches retain their 180-second bound; this does not
retry or admit a timed-out launch. Failed simulator commands retain partial
standard output as well as standard error, followed by bounded diagnostics
and owned-device cleanup.
Native selection and system Reduce Motion XCTest cases run for three complete
iterations with parallel execution disabled. This is repetition, not retry on
failure: any failed iteration rejects the run. Touches require hittable targets;
selection uses a held slow drag, and Settings uses its nested switch control.
Failed cases retain explicit screenshots/hierarchies and native gesture traces.

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
- `RustEpubReader-iOS-arm64-<version>-unsigned.ipa` and `.ipa.sha256`: physical-device
  Release application under `Payload/<app>.app`, without app signing credentials
  or a provisioning profile. It must be signed before installation.
- `RustEpubReader-iOS-SIGNING.md`: [user signing instructions](../ios/UNSIGNED_IPA.md).
- `BUILD-INFO.txt`: source SHA, caller version label and tool versions.

Build logs upload separately even if a build fails. No Apple credentials are
required for these build checks or unsigned IPA packaging. The packager preserves
embedded frameworks and executable permissions; it rejects simulator/wrong-CPU
executables, app signing metadata and invalid ZIP/checksum data. The publisher
verifies the downloaded IPA before attaching it to a Release. Developer signing,
provisioning and store submission are not configured. The project has host Swift checks, Rust tests and an isolated `ReaderUITests`
UI-test target for native selection, system Reduce Motion and Files interactions. Compilation does not validate physical page-turn
gestures, background behavior or App Store acceptance.

All three readers use native rendering. Publication scripts use an embedded
QuickJS interpreter without a WebView, JIT, network, filesystem or account APIs.
Android generates interpreter bindings against the NDK; its build host needs
libclang (provided by the CI LLVM toolchain). Desktop builds need no WebView2 or
WebKitGTK runtime.

For native EPUB image diagnostics and CSS/JavaScript runtime boundaries,
see [EPUB media and interactivity](EPUB_MEDIA_AND_INTERACTIVITY.md). The reusable mobile workflows launch native simulator/emulator checks through
`scripts/check-mobile-runtime.py`, including the committed painting corpus and
real selection gestures. See [mobile runtime checks](MOBILE_RUNTIME_CI.md) for
artifact isolation, system-setting restoration and exact invocation. Local
passes and configured jobs do not establish a hosted pass for unpushed source.

## Local gates

Run from the repository root:

```sh
python3 scripts/bump-version.py --check
python3 scripts/test-bump-version.py
cargo fmt --all -- --check
cargo metadata --locked --format-version 1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
python3 scripts/check-i18n.py
python3 scripts/test-i18n.py
python3 -m unittest discover -s ios/Tests -p test_unsigned_ipa.py
git diff --check
```

On macOS with full Xcode:

```sh
export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
ios/scripts/check-layout.sh
ios/scripts/check-oauth.sh
ios/scripts/check-appearance.sh
ios/scripts/check-reading-mode.sh
ios/scripts/check-speech.sh
ios/scripts/check-contents.sh
ios/scripts/check-i18n.sh
xcodebuild -project ios/RustEpubReader.xcodeproj -scheme ReaderPreview \
  -configuration Debug -sdk iphonesimulator \
  -destination 'generic/platform=iOS Simulator' \
  -derivedDataPath target/ios-local/Simulator CODE_SIGN_IDENTITY=- ARCHS=arm64 build
xcodebuild -project ios/RustEpubReader.xcodeproj -scheme ReaderPreview \
  -configuration Release -sdk iphoneos -destination 'generic/platform=iOS' \
  -derivedDataPath target/ios-local/Device \
  -archivePath target/ios-local/ReaderPreview.xcarchive \
  CODE_SIGNING_ALLOWED=NO ARCHS=arm64 archive
python3 ios/scripts/package-unsigned-ipa.py pack \
  --archive target/ios-local/ReaderPreview.xcarchive \
  --output-dir target/ios-local/assets --version v1.0.8
python3 ios/scripts/package-unsigned-ipa.py verify \
  target/ios-local/assets/RustEpubReader-iOS-arm64-v1.0.8-unsigned.ipa \
  --checksum target/ios-local/assets/RustEpubReader-iOS-arm64-v1.0.8-unsigned.ipa.sha256
```

Android uses JDK 21 to align CI with the locally verified environment. The Java
bytecode target remains 17. Both Android workflows explicitly install
`platform-tools`, `platforms;android-35` and `build-tools;36.0.0`; the APK workflow
then installs NDK 26.1.10909125. The setup action v3 defaults to the obsolete
`tools` package, so never omit its `packages` override. See the
[action input defaults](https://github.com/android-actions/setup-android/blob/v3/action.yml).
With SDK configured, run from `android/`:

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

PR label builds and ordinary CI do not inherit Android release signing secrets.
The release workflow still passes signing secrets to the reusable Android build.
The Android workflow is reusable only; manually dispatching ordinary CI does not
pass release secrets. A manual dispatch of the controlled release workflow can
pass them. Builds without signing values fall back to development signing.
