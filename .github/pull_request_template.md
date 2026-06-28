<!--
提交前请确认：
- 选择一个分类，并在标题前保留对应前缀：
  - [BUG] 修复: ...
  - [Feature] 新功能: ...
  - [Doc] 文档: ...
  - [Update] 更新/优化: ...
-->

# Pull Request 模板

## 分类

- [ ] BUG 修复（bug）
- [ ] 新功能（enhancement）
- [ ] 文档（documentation）
- [ ] 更新/优化（update）

## 变更内容概述

## 相关 Issue（可选）

## 影响平台

- [ ] Desktop (Windows / macOS / Linux)
- [ ] Android
- [ ] Core / EPUB 解析 / 搜索 / 分享
- [ ] CSC 校对 / 模型加载
- [ ] CI / Release / 构建脚本
- [ ] 文档 / 示例

## 验证方式

- [ ] 已运行 `cargo fmt --all -- --check`
- [ ] 已运行 `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] 已运行 `cargo test --all --all-features`
- [ ] 如修改 Android，已运行 `./gradlew :app:assembleRelease` 或相关 Gradle 检查
- [ ] 如修改发布流程，已检查 workflow YAML 与产物路径

## 兼容性/风险评估

- [ ] 无破坏性变更
- [ ] 需要文档更新
- [ ] 配置/依赖有变更
- [ ] 涉及书库/设置/本地数据结构变更
