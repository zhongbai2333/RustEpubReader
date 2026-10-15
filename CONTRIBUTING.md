# 贡献指南 / Contributing

[简体中文](#简体中文) · [English](#english)

## 简体中文

欢迎提交 Bug 报告、文档修订和功能改动。项目包含共享 Rust 核心，以及桌面、Android、iOS / iPadOS 三类客户端；共享数据结构或协议变化需要同时检查受影响的桥接与各客户端。

### 提交 Issue

先搜索已有 Issue，再使用 [Issue 表单](https://github.com/zhongbai2333/RustEpubReader/issues/new/choose)。请提供：

- 问题类型、程序版本、受影响的平台与系统版本。
- 尽可能短的复现步骤、期望结果和实际结果。
- 设备型号或 CPU 架构，以及 Release 下载、源码构建等安装方式。
- iOS / iPadOS 问题注明真机或模拟器；构建问题补充 Xcode 版本。Android 问题补充 Android 版本及设备架构。
- 有必要时提供脱敏日志、截图或可公开复现的 EPUB。不要附上访问令牌、签名文件或完整私有书籍。

### 开发环境

Rust workspace 包含 `reader_core`、`rust_epub_reader`、`android-bridge` 和 `reader_ios`。采用 Rust stable，依赖版本由根目录 `Cargo.lock` 锁定。`vendor/epaint` 是已修订的本地依赖，提交相关改动时须保留许可证。

桌面需要对应系统的 C/C++ 工具链；Linux 系统依赖见 [构建与 CI](docs/BUILD_AND_CI.md)。Android 使用 JDK 21、SDK 35、Build Tools 36.0.0、NDK 26.1.10909125 和 `cargo-ndk`。iOS 本地构建需要 Apple Silicon Mac、完整 Xcode 26 或更新版本，以及 Rust iOS 目标；最低部署版本是 iOS 17。

详细构建步骤见 [README](README.md)、[iOS 说明](ios/README.md)和[构建与 CI](docs/BUILD_AND_CI.md)。不要提交设备本地书库、SDK 路径、账号凭据、签名密钥或 Xcode 用户状态。

### 本地检查

从仓库根目录运行：

```sh
cargo fmt --all -- --check
cargo metadata --locked --format-version 1 --no-deps
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
python3 scripts/check-i18n.py
python3 scripts/test-i18n.py
git diff --check
```

Android 改动还需运行：

```sh
cd android
./gradlew :app:testDebugUnitTest :app:lintVitalRelease -x cargoBuild
./gradlew :app:assembleRelease
```

`-x cargoBuild` 只用于不需要重新打包原生库的检查，不代表 Rust JNI 已构建。修改桥接或共享核心后，打包验证必须重新编译原生库。

在 macOS 上修改 iOS 时，还需运行相关原生检查。以下命令中的 `DEVELOPER_DIR` 请按本机 Xcode 安装路径调整；检查脚本会编译并执行宿主测试：

```sh
export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
ios/scripts/check-layout.sh
ios/scripts/check-oauth.sh
ios/scripts/check-appearance.sh
ios/scripts/check-reading-mode.sh
ios/scripts/check-speech.sh
ios/scripts/check-contents.sh
ios/scripts/check-i18n.sh
```

然后构建 `ReaderPreview` 模拟器应用。涉及真机、签名、Keychain、相机、局域网或生命周期的改动，请说明实际测试的设备和场景；宿主检查或模拟器构建不能替代这些验收。不要为预览应用创建“退出后自动重启”的常驻后台任务。

### 提交 PR

1. 保持改动范围清晰，补充能验证行为的测试；共享协议变化说明迁移和兼容性。
2. 更新受影响的文档。修改根 README 时同步维护中英文版本；翻译文案以 `core/src/i18n` 为准。
3. 使用 PR 模板，说明问题、变更后行为、影响平台、验证结果及未运行的检查。选择已有标题分类：`[BUG]`、`[Feature]`、`[Doc]` 或 `[Update]`。
4. 不把“构建通过”写成“真机验收通过”。需要跨设备验证时，写明发送端、接收端及结果。

维护者可用 `run-build` 标签触发桌面、Android 和 iOS 构建。PR / 普通 CI 使用开发签名；发布密钥只用于受控的发布流程。iOS CI 产出模拟器应用及未签名归档，不发布可安装 IPA。贡献按项目的 [Apache-2.0 许可证](LICENSE)提交。

## English

The project has a shared Rust core and desktop, Android, and iOS/iPadOS clients. Check every affected bridge and client when changing shared data or protocols.

Search existing issues before opening the [issue form](https://github.com/zhongbai2333/RustEpubReader/issues/new/choose). Include the application version, platform, OS version, device/CPU architecture, installation method, minimal reproduction steps, and expected versus actual behavior. For iOS, distinguish a physical device from a simulator and include Xcode versions for build failures. Share sanitized logs and redistributable samples, without credentials, signing files, or private books.

Use Rust stable and the root lockfile. Desktop needs the platform C/C++ toolchain and Linux system libraries. Android uses JDK 21, SDK 35, Build Tools 36.0.0, NDK 26.1.10909125, and cargo-ndk. Local iOS builds require an Apple Silicon Mac, full Xcode 26 or newer, and Rust iOS targets; the minimum deployment target is iOS 17. See [README](README.en.md), [iOS notes](ios/README.md), and [build/CI documentation](docs/BUILD_AND_CI.md).

From the repository root, run:

```sh
cargo fmt --all -- --check
cargo metadata --locked --format-version 1 --no-deps
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
python3 scripts/check-i18n.py
python3 scripts/test-i18n.py
git diff --check
```

For Android, run from `android/`:

```sh
./gradlew :app:testDebugUnitTest :app:lintVitalRelease -x cargoBuild
./gradlew :app:assembleRelease
```

Skipping `cargoBuild` applies only to JVM tests/lint. APK validation after Rust changes must use freshly compiled JNI libraries.

For iOS on macOS, select your installed Xcode path and run the host checks, which compile and execute tests:

```sh
export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
ios/scripts/check-layout.sh
ios/scripts/check-oauth.sh
ios/scripts/check-appearance.sh
ios/scripts/check-reading-mode.sh
ios/scripts/check-speech.sh
ios/scripts/check-contents.sh
ios/scripts/check-i18n.sh
```

Then build the ReaderPreview simulator app. Report physical-device, signing, Keychain, camera, LAN, and lifecycle checks separately from host tests and simulator builds. Do not install persistent auto-restart jobs for preview applications.

Keep PRs focused. Use the template to describe the problem, resulting behavior, affected platforms, commands run, results, and untested cases. Maintain both root README languages and shared translation catalogs in `core/src/i18n`. Preserve licenses in vendored code; omit local runtime data, SDK paths, credentials, signing material, and Xcode user state. The existing title categories are `[BUG]`, `[Feature]`, `[Doc]`, and `[Update]`.

Maintainers can apply `run-build` for all three platform builds. PR/ordinary CI artifacts use development signing; release signing credentials belong only in controlled release builds. iOS CI produces a simulator app and unsigned archive, not an installable IPA. Contributions are made under [Apache-2.0](LICENSE).
