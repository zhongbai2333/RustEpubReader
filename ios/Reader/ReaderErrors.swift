import Foundation

enum ReaderErrors {
    static func display(_ raw: String) -> String {
        for (code, key) in [("TXT_INVALID_REGEX", "txt.error_regex"), ("TXT_PATTERN_NO_MATCH", "txt.error_no_match"), ("TXT_ENCODING_FAILED", "txt.error_encoding"), ("TXT_TOO_LARGE", "txt.error_size"), ("TXT_EMPTY", "txt.error_empty"), ("TXT_INVALID_SELECTION", "txt.error_selection")] {
            if raw.contains(code) { return L(key) }
        }
        if raw.hasPrefix("Reading service") || raw.hasPrefix("Service requires") || raw.hasPrefix("Invalid service") || raw.hasPrefix("No text result") || raw.hasPrefix("Credentials must") {return L("services.error")}
        if raw.hasPrefix("无法打开 EPUB 文件:") || raw.hasPrefix("无法读取资源:") { return L("reader.error_epub_read_failed") }
        if raw == "EPUB has no readable chapters" { return L("reader.error_epub_no_readable_chapters") }
        if raw == "EPUB exceeds the local 100 MiB limit" || raw == "EPUB exceeds the local 1 GiB limit" { return L("reader.error_file_exceeds_limit") }
        if raw.contains("rendered content exceeds") { return L("reader.error_content_too_large") }
        if raw == "Cannot copy EPUB into library storage" || raw == "Cannot save downloaded book" { return L("reader.error_save_book_failed") }
        if raw == "Book is no longer in the library" { return L("reader.error_book_not_in_library") }
        if raw.hasPrefix("Cannot save ") || raw.hasPrefix("Cannot create storage") { return L("reader.error_save_book_failed") }
        if raw.hasPrefix("Invalid stored data:") || raw.hasPrefix("Cannot read ") { return L("library.error_read_failed") }
        if raw.hasPrefix("Reading ") { return L("reader.error_invalid_position") }
        if raw.contains("login expired") { return L("sync.error_github_session_expired") }
        if raw.hasPrefix("REAUTHORIZE:") { return L("sync.error_github_insufficient_permissions_retry") }
        if raw.hasPrefix("RETRY_AFTER:") { return L("sync.error_github_temporarily_unavailable") }
        if raw.contains("GitHub account changed") { return L("sync.error_github_account_changed") }
        if raw.contains("must be a private repository") { return L("sync.error_repo_must_be_private") }
        if raw.contains("not a reader sync repository") || raw.contains("unrelated EpubReaderInfo") { return L("sync.error_repo_not_used_for_sync") }
        return L("common.error_operation_incomplete")
    }
}
