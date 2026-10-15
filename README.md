# RustEpubReader

[简体中文](README.md) · [English](README.en.md)

一个以 Rust 为共享核心的 EPUB 阅读器，提供 Windows、macOS、Linux、Android 和 iOS / iPadOS 客户端。桌面使用 egui / eframe，Android 使用 Jetpack Compose，iOS 使用 SwiftUI / UIKit；正文由各客户端本地排版，不依赖 WebView。

## 平台与安装

| 平台 | 获取方式与要求 |
| --- | --- |
| Windows | [Releases](https://github.com/zhongbai2333/RustEpubReader/releases) 中的 x64 / ARM64 压缩包，或从源码构建 |
| macOS | Intel / Apple Silicon DMG；发布流程尚未配置 Developer ID 签名及公证 |
| Linux | x64 / ARM64 压缩包；桌面构建依赖 GTK 3、ALSA 等系统库，见[构建说明](docs/BUILD_AND_CI.md) |
| Android | Android 8.0 / API 26 及以上；按设备架构选择 APK |
| iOS / iPadOS | 开发预览，最低 iOS 17；arm64 真机可使用 Release 中的未签名 IPA，自行签名后安装；也可通过 Xcode 构建 |

发布流程会生成桌面安装包、Android APK 和 **iOS 未签名 IPA**。iOS IPA 需自行签名后安装，详见[签名说明](ios/UNSIGNED_IPA.md)。普通 CI 和 PR 构建的全部产物可在 Actions 下载；GitHub Release 附带 IPA、SHA-256 校验文件和签名说明，iOS 模拟器应用和未签名真机归档保留在 Actions。尚未配置 TestFlight 或 App Store 发布流程。各系统的实际兼容性仍需分别验证，构建通过不等于完成真机验收。

iOS 的环境光读取使用私有 IOHID 接口，Debug 和 Release 构建均包含该实现（模拟器不启用）。构建成功不代表已具备 App Store 分发条件，详见 [iOS 说明](ios/README.md)。

## 阅读与书库

- 导入 EPUB，管理书库、目录与阅读位置；支持书内搜索、书签、划线和笔记。
- 支持连续滚动和分页阅读。桌面与 Android 提供多种翻页效果；iOS 使用系统卷曲翻页与滑动过渡，具体范围见 [iOS 说明](ios/README.md)。
- 可调整中英文字体、字号、行距、段距、页边距、纸张颜色及背景图片。应用界面和阅读外观可联动，也可分别设置。
- 提供阅读统计、朗读和可选的中文纠错（CSC）功能。在线语音与模型下载需要网络连接，具体实现随平台而异。
- 桌面与 Android 支持 TXT 转 EPUB；iOS 当前通过文件导入器导入 EPUB。
- 界面支持简体中文、English，并可跟随系统语言。

EPUB 的样式、媒体与脚本支持有明确边界。项目保留共享文本坐标来恢复位置，不把不同设备的页码当作同一个位置。详见[原生排版](docs/EPUB_NATIVE_TYPOGRAPHY.md)、[媒体与交互](docs/EPUB_MEDIA_AND_INTERACTIVITY.md)和[分页与性能](docs/EPUB_PAGINATION_AND_PERFORMANCE.md)。

## 同步与分享

**多平台同步**用于自己的设备：支持局域网配对与 GitHub 私有仓库，可同步 EPUB、阅读进度及按设备记录的每日阅读／朗读时长。GitHub 同步需要授权并单独启用；当前打开的书不会因远端进度而自动跳页。

**书本分享**用于向他人发送选定书籍：提供局域网传输和 GitHub 加密分享链接 / 二维码。它与私有书库同步使用不同的流程和存储。收到的书籍先验证，再由接收者确认导入。

- [GitHub 同步与权限说明](docs/GITHUB_SYNC.md)：同步范围、账号存储、重试和取消行为。
- [书本分享说明](docs/BOOK_SHARING.md)：接收方式、加密包、链接权限和使用限制。

GitHub 私有同步需要 `repo` 权限，该权限范围大于单个同步仓库。桌面和 iOS 将令牌保存在系统凭据存储中；Android 使用 Keystore 密钥加密令牌后保存在应用私有存储中。令牌不进入同步文件。私有仓库也不等于端到端加密存储。请在启用前阅读权限说明；分享时仅发送自己有权分享的内容。

## 从源码构建

### 桌面

安装 Rust stable 及对应平台的 C/C++ 构建工具。Linux 还需安装系统开发库，Windows 构建使用 MSVC 与 Windows SDK。

```sh
cargo build --locked --release -p rust_epub_reader
```

运行生成的程序即可。需要缩短本地构建时间时，可改用 `--profile release-fast`。macOS 本地开发签名见[桌面签名说明](desktop/scripts/README.md)，公开分发仍需另行配置签名与公证。

### Android

需要 JDK 21、Android SDK 35、Build Tools 36.0.0、NDK 26.1.10909125、`cargo-ndk`，以及供原生脚本绑定生成使用的 LLVM / libclang。在 `android/local.properties` 中设置 `sdk.dir`，或通过环境变量配置 SDK 路径。

```sh
cargo install cargo-ndk --locked
rustup target add armv7-linux-androideabi aarch64-linux-android x86_64-linux-android
cd android
./gradlew :app:assembleRelease
```

Gradle 会构建并打包三个 ABI 的 Rust 桥接库。未提供发布密钥时使用开发签名；正式分发需要配置自己的签名密钥。

### iOS / iPadOS

需要 Apple Silicon Mac、完整 Xcode 26 或更新版本，以及 Rust iOS 目标。

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
open ios/RustEpubReader.xcodeproj
```

选择 `ReaderPreview` scheme 和模拟器运行。真机安装、命令行构建、依赖及功能限制见 [iOS 说明](ios/README.md)。Windows / Linux 可以修改共享代码；本地 Apple 平台构建需要 macOS / Xcode。

## 开发与反馈

- [贡献指南](CONTRIBUTING.md#简体中文)：开发环境、检查命令、Issue 与 PR 提交流程。
- [提交 Issue](https://github.com/zhongbai2333/RustEpubReader/issues/new/choose)：请附客户端版本、系统与设备、复现步骤及必要日志。iOS 问题请注明真机或模拟器。
- [构建与 CI](docs/BUILD_AND_CI.md)：各工作流、构建产物与本地检查。
- [提交前检查记录](docs/PRECOMMIT_REVIEW.md)：本次检查的结果和未验证项。

不要在 Issue、PR 或测试材料中附上令牌、签名密钥或完整私有书籍；优先使用可公开复现的样例。

## 项目结构

```text
core/            EPUB 解析、书库、同步、分享、统计与本地化
desktop/        egui / eframe 桌面客户端
android-bridge/  Rust JNI 桥接
android/         Kotlin / Jetpack Compose 客户端
ios-bridge/     Rust JSON / C ABI 桥接
ios/            SwiftUI / UIKit 客户端和原生检查
scripts/         跨平台开发检查
vendor/          本地修订的第三方源码
docs/           功能边界、构建与验证说明
```

## 许可证

项目采用 [Apache License 2.0](LICENSE)。第三方源码和依赖保留各自许可证。
