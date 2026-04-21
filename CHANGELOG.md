# Changelog

## [1.1.0] - 2026-04-21

### Added
- **Android 段评面板** — 点击正文 `(N)` 链接，底部弹层展示对应段评
  - 支持锚点筛选（如 `#para-6`），默认只显示当前段落评论
  - 可切换"显示全部"
  - 结构化卡片展示：作者（蓝色加粗）、评论内容、格式化时间、赞数
- **段评卡片化渲染** — 解析 `"1. 【内容】 作者：xxx | 时间：xxx | 赞：52"` 格式
- **时间戳格式化** — Unix 时间戳自动转换为 `yyyy-MM-dd HH:mm`
- **链接碰撞箱扩展** — 点击热区向四周扩展 30px，提升触屏体验
- **Win64 段评面板** — 右侧滑出覆盖层，支持 ESC 关闭、锚点滚动、Dark 模式适配

### Changed
- **默认翻页模式** — Android 默认从上下滚动（`isScrollMode = true`）改为左右翻页（`false`）
- **Android 构建配置** — `cargoBuild` targets 增加 `armeabi-v7a` 与 `x86_64`

### Fixed
- Desktop 段评面板焦点丢失问题
- Desktop 3 处阅读器焦点/交互 bug

### Notes
- Linux 代码未改动，保持与原项目同步
- Android 上下滚动模式存在已知 bug，暂时未修复

---

## [1.0.1] - 2025-04

### Fixed
- Copilot 安全修复（`core/src/sharing/*` 等 6 个文件）

## [1.0.0] - 2025-04

### Added
- 初始发布：跨平台 EPUB 阅读器（Windows / Linux / macOS / Android）
- 书库管理、目录导航、外观自定义、字体设置
- 多语言支持（中文简体 / English）
- 局域网共享（PIN 配对）
- 翻页动画（滑动 / 覆盖 / 仿真 / 无）
