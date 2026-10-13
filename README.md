# RustEpubReader

一款用 Rust 编写的跨平台 EPUB 阅读器，支持 **桌面端（Windows / Linux / macOS）** 、**Android** 和 **iOS / iPadOS**。

## 特性

- **书库管理** — 自动记录已打开的书籍、阅读进度与章节位置
- **阅读模式** — 支持滚动模式与翻页模式，翻页动画可选（滑动 / 覆盖 / 仿真 / 无）
- **目录导航** — 侧边栏目录（TOC），可随时展开/折叠
- **外观自定义** — 亮色 / 暗色主题，自定义背景色、字体颜色、背景图片及透明度
- **字体设置** — 自动发现系统字体，可按需切换字体与字号
- **多语言** — 内置中文（简体）与 English 界面
- **局域网共享** — 通过点对点协议在设备间传输书籍（PIN 配对）
- **GitHub 同步** — 同步 EPUB 和阅读进度，自动同步间隔为 5 分钟
- **Android 支持** — 通过 JNI 桥接核心逻辑，Jetpack Compose 构建 Android UI
- **iOS / iPadOS 支持** — SwiftUI / UIKit 原生界面，Rust 核心桥接；具体支持范围见 [iOS 说明](ios/README.md)

## 项目结构

```
RustEpubReader/
├── core/           # 核心库：EPUB 解析、书库、i18n、局域网共享协议
├── desktop/        # 桌面端（egui/eframe）
├── android-bridge/ # Android JNI 桥接层（cdylib）
├── android/        # Android 工程（Kotlin + Jetpack Compose）
├── ios-bridge/     # iOS JSON / C ABI 桥接层（staticlib）
└── ios/            # iOS / iPadOS 工程（SwiftUI + UIKit）
```

## 构建

### 桌面端

```bash
cargo build --release -p rust_epub_reader
```

Windows 下会自动嵌入应用图标（需要 `rc.exe`）。

### macOS 安装说明

当前发布流程生成 Apple Silicon 和 Intel 两种 DMG。打开 DMG 后，将应用拖入“应用程序”目录。
现有流程尚未配置 Developer ID 签名及公证；系统可能要求在“系统设置 → 隐私与安全性”中授权打开。
本地开发的稳定签名方式见 [开发签名说明](desktop/scripts/README.md)，它不替代公开发行签名。

### Android

使用 JDK 21、Android SDK 和 NDK 26.1.10909125，配置 `android/local.properties` 的 `sdk.dir`，然后：

```bash
cargo install cargo-ndk --locked
rustup target add armv7-linux-androideabi aarch64-linux-android x86_64-linux-android
cd android
./gradlew assembleRelease
```

Gradle 会编译并打包三个 ABI 的 Rust 桥接库。没有配置发布密钥时，Release APK 使用开发签名；
正式分发需要仓库的 Android 签名配置。

### iOS / iPadOS

需要 Apple Silicon Mac、完整 Xcode 26 和 Rust iOS target。构建与功能范围见 [iOS 说明](ios/README.md)。
Windows 可以修改代码、检查共享 Rust 核心，并通过 GitHub CI 验证 Apple 平台构建；本地 iOS 构建需要 macOS / Xcode。

### CI 与提交前检查

普通 CI 覆盖 Rust 格式、严格 Clippy、workspace 测试、Android 单元测试 / Release lint，
以及桌面、Android 和 iOS 构建。iOS 流程产出 arm64 模拟器应用及**未签名真机 Release 归档**，
不产出可安装 IPA，也不执行 TestFlight / App Store 上传。

- [CI 配置与产物说明](docs/BUILD_AND_CI.md)
- [Windows 最后测试清单](docs/WINDOWS_TESTING.md)


## 依赖

| 用途 | 库 |
|------|----|
| EPUB 解析 | `rbook` |
| HTML 提取 | `scraper` |
| 桌面 UI | `eframe` / `egui` |
| 文件对话框 | `rfd` |
| 图像处理 | `image` |
| Android JNI | `jni` |
| 序列化 | `serde` / `serde_json` |

## License

本项目采用 [Apache License 2.0](LICENSE)。

## GitHub sync (desktop, Android and iOS)

Use **Multi-platform sync** in the library on all three clients, then choose
local network sharing or GitHub sign-in / account management. The desktop reader
toolbar uses the same entry. **Manage GitHub account** is also available in reader
settings and contributions.
The unified account interface requests `repo` access to create and synchronize the
private `EpubReaderInfo` repository. Sign-in alone does not enable synchronization
or submit correction samples. **Sign in and enable sync** starts sync only after
authorization succeeds; cancelling or failing authorization does not enable it.
Existing narrow-scope tokens require renewed device authorization, not deletion.
GitHub's `repo` scope also covers other private repositories accessible to the
account; this app's sync transport writes only its marked sync repository. Legacy
Android contribution-only authorization can use `public_repo` and needs renewed
permission for private sync.

Desktop uses macOS Keychain, Windows credential storage or Linux Secret Service.
iOS uses native Keychain. Android encrypts the token with an AES-GCM key in Android
Keystore, migrates legacy plaintext tokens after successful secure storage, and
excludes credentials and device identity from backups/transfers.
Tokens are never included in sync files or settings JSON.

The app creates or reuses the private `EpubReaderInfo` repository. A repository
with that name must have the reader schema marker; an unrelated repository or a
public repository is rejected without uploading books. If initial creation was
interrupted before the marker was written, rename that incomplete repository
before retrying. The client never deletes or changes repository visibility.

- `epub-reader.json`: application/schema marker.
- `books/<sha256>.epub`: immutable EPUB bytes (20 MiB maximum per file).
- `devices/<device-id>.json`: device-specific book inventory and reading positions.

Only EPUB files and reading progress are synchronized. Bookmarks, highlights,
notes, settings and reading statistics remain local. Positions currently use the
shared parser's chapter/block/character coordinates; EPUB CFI is not implemented.
Progress suggestions select the latest timestamp, so intentional rereading can
move backwards. Each device's record remains separate; device clocks should be
correct. The current open book never jumps automatically: use the settings
button to apply another device's suggested position. Books not currently open
receive newer positions automatically. Uploads use SHA-based compare-and-set;
conflicts are reread and retried without force updates.

All three clients sync and apply reading progress before starting EPUB transfers.
The file phase uses validated device manifests as an index to avoid checking every
already uploaded book individually. Automatic sync runs while the desktop app is
open or a mobile app is in the foreground, normally every five minutes. Mobile clients cancel further requests when
backgrounded, disabled, signed out, or when the account changes. Network
failures retain local records and retry; missing permissions require renewed
authorization. GitHub rate-limit delays are respected. Background downloads are
bounded to 20 attempts / 100 MiB per pass; uploads also have a 20-attempt limit.
Additional batches resume after 15 seconds. A failed book does not stop other
books: its account/device-specific retry record is saved locally, with exponential
backoff from 30 seconds up to one hour. **Retry failed books** retries only the
queued files, after refreshing progress, and still respects GitHub rate limits.
Completed uploads are retained in the manifest; download failures are cleared
only after EPUB validation and a successful library save.
Downloads are verified by size and SHA-256 before import. Unreadable/oversized
local books are skipped and counted. Disabling sync or signing out cancels future
requests without deleting local books or the repository; an already sent request
may complete. Local deletions do not delete cloud files; a previously advertised
book removed on this device is not automatically restored on the same device.

`reader_core` exposes the portable schema and the optional `github-sync` API
transport. Desktop, Android and iOS use this transport and repository format.
Mobile clients stage
downloads and applies them to the live library only after account/session checks.
Android resume uses the saved library position, allowing intentional rereading to
move backwards despite older preferences.
Private repositories are
not end-to-end encrypted storage, and Git history retains previous file versions.

Validation: local simulated GitHub API tests cover permission checks, private
repository creation, public/unrelated repository refusal, upload/download hashes,
progress conflicts, cancellation, progress-first ordering, per-book failure
isolation, persistent retries and rate-limit stopping. Real-account OAuth, remote creation and
multi-device acceptance require an authorized live run.
