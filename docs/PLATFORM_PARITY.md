# 三端功能对照

核对日期：2026-10-24；功能以 V2.0.3（90cb519）为准。三端指桌面、Android 和 iOS / iPadOS。

表中“已接入”表示对应客户端已有完整调用路径，不等于所有真机操作均已验收。

## 共同功能

- EPUB 导入，原生滚动和分页，原生／混合／经典三档排版、独立脚本开关、强制单栏，以及原生支持范围内的插图、表格和脚注。
- 书库搜索、排序、网格／列表、多选管理、原书导出、归档、恢复及归档内删除。
- GitHub 私有书库同步先处理阅读进度，再处理书签、高亮和笔记，随后在后台传输书籍文件；支持大文件分块及失败文件重试。局域网配对及对外书籍分享另有各自入口。
- 阅读记录恢复、阅读中的远端进度跳转及返回入口；阅读统计、批注 Markdown 导出、书库备份校验与合并恢复。
- 本地字体导入、20 款字体下载库及预览、外观联动、在线朗读、可选 MacBERT 章节纠错。

三端均采用原生排版和受限脚本适配，不具备浏览器级 CSS／JavaScript 兼容性。阅读入口处理 EPUB 和 TXT 转换，PDF 阅读未接入。

## 平台差异

| 能力 | 桌面 | Android | iOS / iPadOS |
| --- | --- | --- | --- |
| TXT 自动识别、无编号标题、自定义正则 | 已接入 | 已接入 | 已接入 |
| TXT 逐项取消标题、从原文补选标题、正文摘录 | 已接入 | 已接入 | 已接入 |
| 从文件选择器一次导入多本 EPUB | 已接入；后台逐本导入、单本失败不阻断 | 已接入 | 已接入；单本失败不阻断 |
| 中英文字体分开设置／英文跟随中文 | 已接入 | 已接入；绘制与分页测量共用字形范围 | 已接入 |
| 在线朗读 | Edge 在线语音 | Edge 在线语音 | Edge 在线语音，另可选系统语音 |
| 本机系统语音朗读 | 未接入 | 未接入 | 已接入；取决于可用系统语音 |
| 选词查词 | 原生服务结果／主动打开汉典 | 原生服务结果／主动打开汉典 | 系统词典或自定义服务 |
| 选词翻译 | 原生结果、语言选择、重试 | 原生结果、语言选择、重试 | 原生结果、语言选择、重试 |
| 自定义翻译／词典 API | OpenAI 兼容／JSON；系统凭据存储 | OpenAI 兼容／JSON；Keystore 加密 | OpenAI 兼容／JSON；Keychain |
| 局域网设备自动发现 | Bonjour／UDP | 系统 DNS-SD／UDP | 前台 Bonjour；保留手动地址入口 |
| 已配对设备重连 | 已保存身份重连 | 已保存身份重连 | 已保存身份重连；前台自动尝试有间隔 |
| MacBERT 纠错样本贡献 | 预览、逐项移除、主动提交 | 预览、逐项移除、主动提交 | 预览、逐项移除、主动提交 |
| 导出包含纠错／批注的重建 EPUB | 原书／纠错／批注／两者 | 原书／纠错／批注／两者 | 原书／纠错／批注／两者 |
| 阅读环境光联动 | 不读取传感器，选中后提示回退系统外观 | 有 TYPE_LIGHT 传感器接入 | 代码有真机私有 IOHID 路径；模拟器不读取 |
| 新版本处理 | 检查更新；正式 ZIP／DMG 打开官方下载页，保留旧式裸可执行文件更新路径 | 提示并打开下载链接 | 提示并打开 Release 页面，不在应用内安装更新 |

多选导入仅处理 EPUB；TXT 需单独选取并核对分章。桌面和 iOS 在多选结果中跳过 TXT，并提示单独导入。

## TXT 的当前边界

- 三端使用同一套混合格式识别规则；当前 TXT 上限为 64 MiB。原始 TXT 不会被转换操作修改，生成 EPUB 后按普通书籍导入。
- 三端均可包含／排除指定原文行。取消标题勾选只合并章节边界，标题原文字仍保留在正文中。
- 三端目录预览最多显示前 500 项，转换仍处理完整文本；原文搜索最多显示前 80 条匹配，摘录最多 240 字符。可从不超过 512 字符的原文行补选标题；校正仅用于当次转换。
- 无编号标题识别默认开启，明确的短章节不因篇幅短而合并。自定义规则无效或没有命中时会报错，需修改后再转换。
- 没有选定标题时，按文本长度分段，界面会标明。无编号短标题仍可能漏识别，正文短句也可能误判，需要人工核对。
- iOS 转换使用私有临时副本，普通关闭后释放。进程被强制结束时，下一次导入会清理本应用遗留的临时目录；不会清理原书或书库。

## 使用条件与实现边界

自定义服务以 OpenAI 兼容接口为默认选项，可输入基础地址或完整聊天地址，并单独设置模型；也保留 JSON 接口。仅主动查询时发送所选文字，没有预设免费服务。密钥使用本机系统凭据存储，不纳入书库备份和 GitHub 同步。具体协议和限制见[阅读服务](READING_SERVICES.md)。

三端接入同一 Bonjour 服务名 `_epubreader._tcp`，桌面和 Android 保留 UDP 发现旧客户端；iOS 随前台生命周期启停，保留手动地址。发现信息只用于提供地址，首次配对仍需 PIN，后续使用已保存身份。当前发现地址采用 IPv4。模拟器测试不能证明真机网络权限、路由器隔离和跨设备重连已通过。

纠错贡献仅收集已接受且仍匹配原文的纠错；短上下文每侧 10 字符，单条最多 50 字符、最多 500 条，可逐条移除。主动提交后样本在 GitHub 公开。内容 SHA-256 用于形成独立分支与文件名；相同样本复用既有贡献请求，不写用户 fork 的默认分支。

重建导出保留原 ZIP 内插图、字体、样式和其他资源，仅修改准确定位的正文，支持已接受纠错、高亮、笔记和书签锚点。位置失效、重叠纠错或无法对应原文时中止并清理临时文件，不覆盖原书和已有目标文件。排版或文本位置无法可靠对应的 EPUB 会提示导出失败。

## 源码与界面索引

- TXT：[共享分章与选择接口](../core/src/txt/mod.rs)、[桌面导入界面](../desktop/src/ui/txt_import.rs)、[Android 导入界面](../android/app/src/main/java/com/epub/reader/ui/library/TxtImportDialog.kt)、[iOS 导入与校正](../ios/Reader/TxtImport.swift)。
- 字体与朗读：[字体说明](FONTS.md)、[Android 设置](../android/app/src/main/java/com/epub/reader/ui/reader/ReaderSettingsSheet.kt)、[桌面朗读](../desktop/src/ui/tts.rs)、[Android 朗读](../android/app/src/main/java/com/epub/reader/tts/TtsManager.kt)、[iOS 语音选择](../ios/Reader/ReadingSpeechPanel.swift)。
- 查词／翻译：[桌面选区操作](../desktop/src/ui/reader_block.rs)、[Android 查询弹窗](../android/app/src/main/java/com/epub/reader/ui/reader/SelectionDialogs.kt)、[iOS 阅读工具](../ios/Reader/ReadingTools.swift)。
- 局域网：[桌面连接界面](../desktop/src/ui/sharing.rs)、[Android 发现与重连](../android/app/src/main/java/com/epub/reader/viewmodel/ReaderViewModel.kt)、[iOS 发现与配对入口](../ios/Reader/NearbySync.swift)。
- 导出与贡献：[桌面导出选项](../desktop/src/ui/export_dialog.rs)、[共享重建导出](../core/src/export.rs)、[Android 桥接](../android-bridge/src/lib.rs)、[iOS 桥接](../ios-bridge/src/lib.rs)、[桌面样本贡献](../desktop/src/ui/csc_contribute.rs)、[Android 样本贡献](../android/app/src/main/java/com/epub/reader/util/GitHubContributor.kt)。
- 导入选择器：[桌面](../desktop/src/app.rs)、[Android](../android/app/src/main/java/com/epub/reader/MainActivity.kt)、[iOS](../ios/Reader/ReaderPreviewApp.swift)。
- CI / 本地检查：[iOS 工作流](../.github/workflows/ios-build.yml)、[本地运行检查收集器](../scripts/check-mobile-runtime.py)、[TXT 界面测试](../ios/UITests/TxtImportUITests.swift)。
