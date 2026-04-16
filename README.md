# RustEpubReader

一款用 Rust 编写的 EPUB 阅读器，目前仅支持 **Windows 64 位（x86_64）**。

## 特性

- **书库管理** — 自动记录已打开的书籍、阅读进度与章节位置
- **阅读模式** — 支持滚动模式与翻页模式，翻页动画可选（滑动 / 覆盖 / 仿真 / 无）
- **目录导航** — 侧边栏目录（TOC），可随时展开/折叠
- **外观自定义** — 亮色 / 暗色主题，自定义背景色、字体颜色、背景图片及透明度
- **字体设置** — 自动发现系统字体，可按需切换字体与字号
- **多语言** — 内置中文（简体）与 English 界面
- **局域网共享** — 通过点对点协议在设备间传输书籍（PIN 配对）

## 项目结构

```
RustEpubReader/
├── core/           # 核心库：EPUB 解析、书库、i18n、局域网共享协议
└── desktop/        # Windows 桌面端（egui/eframe）
```

## 构建

### 桌面端

```bash
cargo build --release -p rust_epub_reader
```

Windows 下会自动嵌入应用图标（需要 `rc.exe`）。

## 依赖

| 用途 | 库 |
|------|----|
| EPUB 解析 | `epub` |
| HTML 提取 | `scraper` |
| 桌面 UI | `eframe` / `egui` |
| 文件对话框 | `rfd` |
| 图像处理 | `image` |
| 序列化 | `serde` / `serde_json` |

## License

见 [LICENSE](LICENSE)。
