# RustEpubReader

一款用 Rust 编写的跨平台 EPUB 阅读器，支持 **桌面端（Windows / Linux / macOS）** 和 **Android**。

## 特性

- **书库管理** — 自动记录已打开的书籍、阅读进度与章节位置
- **阅读模式** — 支持滚动模式与翻页模式，翻页动画可选（滑动 / 覆盖 / 仿真 / 无）
- **目录导航** — 侧边栏目录（TOC），可随时展开/折叠
- **外观自定义** — 亮色 / 暗色主题，自定义背景色、字体颜色、背景图片及透明度
- **字体设置** — 自动发现系统字体，可按需切换字体与字号
- **多语言** — 内置中文（简体）与 English 界面
- **局域网共享** — 通过点对点协议在设备间传输书籍（PIN 配对）
- **段评支持** — 正文段落链接直达段评面板，支持锚点筛选、结构化卡片展示、时间戳格式化（Android & Desktop）
- **Android 支持** — 通过 JNI 桥接核心逻辑，Jetpack Compose 构建 Android UI，支持左右翻页与上下滚动

## 项目结构

```
RustEpubReader/
├── core/           # 核心库：EPUB 解析、书库、i18n、局域网共享协议
├── desktop/        # 桌面端（egui/eframe）
├── android-bridge/ # Android JNI 桥接层（cdylib）
└── android/        # Android 工程（Kotlin + Jetpack Compose）
```

## 构建

### 桌面端

```bash
# Windows（无 CSC/ONNX 依赖，体积更小）
cargo build --release --no-default-features -p rust_epub_reader

# Linux / macOS
cargo build --release -p rust_epub_reader
```

Windows 下会自动嵌入应用图标（需要 `rc.exe`）。

### Android

项目已配置 `cargoBuild` Gradle task，编译时会自动调用 `cargo-ndk` 生成 `.so`：

```bash
cd android
./gradlew assembleDebug
```

如需手动编译 bridge：
```bash
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 build --release -p android-bridge
```

## 依赖

| 用途 | 库 |
|------|----|
| EPUB 解析 | `epub` |
| HTML 提取 | `scraper` |
| 桌面 UI | `eframe` / `egui` |
| 文件对话框 | `rfd` |
| 图像处理 | `image` |
| Android JNI | `jni` |
| 序列化 | `serde` / `serde_json` |

## License

见 [LICENSE](LICENSE)。
