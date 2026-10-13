import Foundation

enum ReaderErrors {
    static func display(_ raw: String) -> String {
        if raw.hasPrefix("无法打开 EPUB 文件:") || raw.hasPrefix("无法读取资源:") { return "无法读取所选 EPUB，请检查文件后重试。" }
        if raw == "EPUB has no readable chapters" { return "EPUB 中没有可阅读的章节。" }
        if raw == "EPUB exceeds the local 100 MiB limit" { return "文件超过本机导入上限（100 MiB）。" }
        if raw.contains("rendered content exceeds") { return "书籍内容过大，当前无法排版。" }
        if raw == "Cannot copy EPUB into library storage" || raw == "Cannot save downloaded book" { return "无法保存书籍，请检查设备存储空间后重试。" }
        if raw == "Book is no longer in the library" { return "这本书已不在书库中，请重新导入。" }
        if raw.hasPrefix("Reading ") { return "阅读位置无效，请重新打开书籍。" }
        if raw.contains("login expired") { return "GitHub 登录已失效，请重新登录。" }
        if raw.hasPrefix("REAUTHORIZE:") { return "GitHub 权限不足，请重新授权。" }
        if raw.hasPrefix("RETRY_AFTER:") { return "GitHub 暂时无法同步，请稍后重试。" }
        if raw.contains("GitHub account changed") { return "GitHub 账号已变化，请重新登录。" }
        if raw.contains("must be a private repository") { return "同步仓库必须为私有仓库，并属于当前账号。" }
        if raw.contains("not a reader sync repository") || raw.contains("unrelated EpubReaderInfo") { return "同名仓库未用于本应用同步，请检查 GitHub 仓库。" }
        return "操作未完成，请稍后重试。"
    }
}
