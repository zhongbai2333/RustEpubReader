# RustEpubReader

[简体中文](README.md) · [English](README.en.md)

An EPUB reader with a shared Rust core and clients for Windows, macOS, Linux, Android, and iOS/iPadOS. The desktop client uses egui/eframe, Android uses Jetpack Compose, and iOS uses SwiftUI/UIKit. Each client lays out book content locally, without a WebView.

## Platforms and installation

| Platform | Availability and requirements |
| --- | --- |
| Windows | x64 / ARM64 archives from [Releases](https://github.com/zhongbai2333/RustEpubReader/releases), or build from source |
| macOS | Intel / Apple Silicon DMG; Developer ID signing and notarization are not configured in the release workflow |
| Linux | x64 / ARM64 archives; desktop builds require GTK 3, ALSA, and other system libraries listed in the [build guide](docs/BUILD_AND_CI.md) |
| Android | Android 8.0 / API 26 or newer; choose the APK matching your device architecture |
| iOS / iPadOS | Development preview, minimum iOS 17; arm64 device IPA from Releases requires your own signing; building with Xcode is also supported |

Release workflows package desktop applications, Android APKs and an **unsigned iOS IPA**. The IPA requires your own signing before installation; see the [signing instructions](ios/UNSIGNED_IPA.md). Normal CI and PR artifacts are available in Actions. GitHub Releases attach the IPA, SHA-256 receipt and signing instructions; the iOS simulator application and unsigned device archive remain in Actions. TestFlight and App Store publishing are not configured. Runtime compatibility needs testing on each platform; a successful build does not establish physical-device acceptance.

Ambient-light reading on iOS currently uses private IOHID APIs in both Debug and Release (disabled in the simulator). Successful builds do not establish App Store distribution readiness; see the [iOS notes](ios/README.md).

## Reading and library

- Import EPUBs, manage your library and reading positions, navigate the table of contents, search books, and use bookmarks, highlights, and notes.
- Read continuously or page by page. Desktop and Android offer multiple page-turn effects; iOS uses system page-curl and sliding transitions. See the [iOS notes](ios/README.md) for platform details.
- Adjust Chinese and Latin fonts, text size, line/paragraph spacing, margins, paper colors, and background images. App and reading appearance can follow one another or be configured independently.
- Reading statistics, text-to-speech, and optional Chinese text correction (CSC) are available. Network voices and model downloads need connectivity; implementations vary by platform.
- Desktop and Android support TXT-to-EPUB conversion. The iOS file importer currently accepts EPUB files only.
- The interface supports Simplified Chinese, English, and automatic system-language selection.

EPUB styles, media, and scripts have documented limits. Shared text coordinates restore positions across devices; device-specific page numbers are not treated as shared positions. See [native typography](docs/EPUB_NATIVE_TYPOGRAPHY.md), [media and interactivity](docs/EPUB_MEDIA_AND_INTERACTIVITY.md), and [pagination/performance](docs/EPUB_PAGINATION_AND_PERFORMANCE.md).

## Sync and sharing

**Multi-platform sync** is for your own devices: local-network pairing and GitHub private repositories can synchronize EPUB files, reading progress, and daily per-device reading/listening time. GitHub sync requires authorization and separate enablement. A newer accepted remote position automatically moves the open reader; the bottom-left return button goes back to the previous position.

**Book sharing** sends selected books to other people using local-network transfers or encrypted GitHub share links/QR codes. It uses a separate flow and storage from private-library synchronization. Received books are validated before the recipient confirms import.

- [GitHub sync and permissions](docs/GITHUB_SYNC.md): synchronized data, credential storage, retries, and cancellation.
- [Book sharing](docs/BOOK_SHARING.md): receiving books, encrypted packages, link access, and limits.

GitHub private sync requires the `repo` scope, which grants broader access than a single sync repository. Desktop and iOS keep tokens in system credential storage; Android stores tokens in private app storage encrypted with a Keystore key. Tokens are excluded from sync files. A private repository is not end-to-end encrypted storage. Read the permission details before enabling sync, and share only content you have permission to distribute.

## Build from source

### Desktop

Install Rust stable and your platform's C/C++ build tools. Linux also needs system development libraries; Windows builds use MSVC and the Windows SDK.

```sh
cargo build --locked --release -p rust_epub_reader
```

Run the resulting executable. Use `--profile release-fast` instead for a faster local build. macOS development signing is documented in the [desktop signing notes](desktop/scripts/README.md); public distribution still needs separate signing and notarization.

### Android

Use JDK 21, Android SDK 35, Build Tools 36.0.0, NDK 26.1.10909125, cargo-ndk, and LLVM/libclang for native script binding generation. Set `sdk.dir` in `android/local.properties` or configure the SDK path through environment variables.

```sh
cargo install cargo-ndk --locked
rustup target add armv7-linux-androideabi aarch64-linux-android x86_64-linux-android
cd android
./gradlew :app:assembleRelease
```

Gradle builds and packages the Rust bridge for three ABIs. Without release credentials it uses development signing; configure your own signing key for distribution.

### iOS / iPadOS

Use an Apple Silicon Mac, full Xcode 26 or newer, and the Rust iOS targets.

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
open ios/RustEpubReader.xcodeproj
```

Choose the `ReaderPreview` scheme and a simulator, then click **Run**. Physical-device installation, command-line builds, dependencies, and feature limits are covered in the [iOS notes](ios/README.md). Windows/Linux can edit shared code; local Apple builds require macOS/Xcode.

## Development and feedback

- [Contributing](CONTRIBUTING.md#english): setup, checks, issues, and pull requests.
- [Report an issue](https://github.com/zhongbai2333/RustEpubReader/issues/new/choose): include app/OS versions, device details, reproduction steps, and relevant logs. For iOS, distinguish a physical device from a simulator.
- [Build and CI](docs/BUILD_AND_CI.md): workflows, artifacts, and local checks.
- [Pre-commit review](docs/PRECOMMIT_REVIEW.md): results and untested areas for this review.

Keep tokens, signing keys, and private books out of issues, PRs, and test materials. Prefer redistributable reproductions.

## Repository layout

```text
core/            EPUB parsing, library, sync, sharing, statistics, localization
desktop/        egui / eframe desktop client
android-bridge/  Rust JNI bridge
android/         Kotlin / Jetpack Compose client
ios-bridge/     Rust JSON / C ABI bridge
ios/            SwiftUI / UIKit client and native checks
scripts/         Cross-platform development checks
vendor/          Locally patched third-party source
docs/           Feature boundaries, builds, and verification
```

## License

[Apache License 2.0](LICENSE). Third-party source and dependencies retain their respective licenses.
