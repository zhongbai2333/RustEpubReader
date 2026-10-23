# 阅读服务

三端都可在选词后查词或翻译，结果显示在原生面板中，失败后可以重试。未配置服务时，iOS 查词使用系统词典；其他入口可手动打开在线词典或翻译网页。应用不提供内置云服务。

## 配置 OpenAI 兼容接口

这是新配置的默认接口类型。翻译和词典可分别设置：

- 服务地址：填写基础地址或完整的 `/chat/completions` 地址。
- 模型名称：填写服务商提供的模型名。
- API 密钥：服务需要认证时填写。

基础地址有路径时，在末尾追加 `/chat/completions`；没有路径时使用 `/v1/chat/completions`。完整聊天地址不会重复追加。旧地址中的 `?model=模型名` 仍有效，优先于模型字段。

应用以非流式方式请求，关键字段为 `model`、`messages` 和 `stream:false`，读取 `choices[0].message.content`。当前实现的系统消息指定查词或翻译任务及目标语言，用户消息放入所选文字。部分服务的扩展接口可能需要单独适配。

## JSON 接口

选择 JSON 接口后，应用发送 HTTPS POST：

- `text`：所选文字。
- `source`：固定为 `auto`。
- `target`：目标语言，默认为 `zh`。
- `kind`：查词为 `dictionary`，翻译为 `translate`。

响应可直接是 JSON 字符串，也可把结果放在 `text`、`result`、`translatedText`、`translation` 或 `choices[0].message.content` 中。其他结构可在地址片段中填写 JSON Pointer，例如 `https://example.com/query#/data/text`；`#/data/text` 不会发送给服务端。

## 数据与限制

每次查询发送所选文字，不上传整本书。API 密钥通过 Bearer 认证发送，保存在本机系统凭据存储中，不写入普通设置、书库备份或 GitHub 同步。桌面和 Android 在安全保存成功后迁移旧设置中的密钥。请把密钥填在专用字段中，避免写入服务地址。

单次最多 16000 个 Unicode 字符，8 秒超时，响应上限为 1 MiB。除本机回环地址外，请求地址须为 HTTPS；请求不跟随重定向。关闭面板后，界面不会因迟到的结果重新打开。已发出的请求可能仍在服务端执行。

实现与协议测试见[共享阅读服务](../core/src/reading_services.rs)。
