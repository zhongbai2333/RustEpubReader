package com.zhongbai233.epub.reader.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.json.JsonNames

/** 内联样式 — 对应 Rust 版本的 InlineStyle */
@Serializable
enum class InlineStyle {
    Normal, Bold, Italic, BoldItalic
}

/** 纠错状态 — 对应 Rust 版本的 CorrectionStatus */
@Serializable
enum class CorrectionStatus {
    Pending, Accepted, Rejected, Ignored
}

/** 纠错信息 — 对应 Rust 版本的 CorrectionInfo */
@Serializable
data class CorrectionInfo(
    val original: String,
    val corrected: String,
    val confidence: Float = 0f,
    val charOffset: Int = 0,
    val status: CorrectionStatus = CorrectionStatus.Pending
)

/** 文本片段 — 对应 Rust 版本的 TextSpan */
@Serializable
data class TextSpan(
    val text: String,
    val style: InlineStyle = InlineStyle.Normal,
    val linkUrl: String? = null,
    val correction: CorrectionInfo? = null
)

/** 内容块 — 对应 Rust 版本的 ContentBlock */
@Serializable
sealed class ContentBlock {
    @Serializable
    @SerialName("heading")
    data class Heading(val level: Int, val spans: List<TextSpan>, val anchorId: String? = null) : ContentBlock()

    @Serializable
    @SerialName("paragraph")
    data class Paragraph(val spans: List<TextSpan>, val anchorId: String? = null) : ContentBlock()

    @Serializable
    @SerialName("image")
    data class Image(val data: String, val alt: String? = null) : ContentBlock()

    @Serializable
    @SerialName("separator")
    data object Separator : ContentBlock()

    @Serializable
    @SerialName("blankLine")
    data object BlankLine : ContentBlock()
}

/** 单个章节的数据传输对象 */
@Serializable
data class ChapterDto(
    val title: String,
    @SerialName("sourceHref")
    val sourceHref: String? = null,
    val blocks: List<ContentBlock>
)

@Serializable
data class TocEntryDto(
    val title: String,
    val chapterIndex: Int
)

@Serializable
data class ChapterReviewEntry(
    val main: Int,
    val review: Int
)

@Serializable
data class BookMetadataDto(
    val title: String,
    val chapterCount: Int,
    val toc: List<TocEntryDto>,
    val hasCover: Boolean,
    val chapterReviews: List<ChapterReviewEntry> = emptyList(),
    val reviewChapterIndices: List<Int> = emptyList()
)

/** 方便与老代码兼容保留的 Chapter 定义（如果需要映射） */
data class Chapter(
    val title: String,
    val blocks: List<ContentBlock>,
    val sourceHref: String? = null
)

/** EPUB 书籍 (兼容用) */
data class EpubBook(
    val title: String,
    var chapters: List<Chapter>,
    val toc: List<Pair<String, Int>>,
    val coverData: ByteArray? = null
) {
    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other !is EpubBook) return false
        return title == other.title && chapters == other.chapters
    }

    override fun hashCode(): Int = title.hashCode() * 31 + chapters.hashCode()
}

/** 书库条目 — 用于持久化 */
@Serializable
@OptIn(ExperimentalSerializationApi::class)
data class BookEntry(
    @SerialName("id") val id: String = "",
    val title: String,
    @JsonNames("uri", "path")
    val uri: String,       // Android content URI 或文件路径
    @SerialName("config_path") val configPath: String? = null,
    @JsonNames("lastChapter", "last_chapter")
    val lastChapter: Int = 0,
    @SerialName("last_chapter_title") val lastChapterTitle: String? = null,
    @JsonNames("lastOpened", "last_opened")
    val lastOpened: Long = System.currentTimeMillis()
)

/** epub 元数据缓存 */
@Serializable
data class EpubMetadata(
    val title: String? = null,
    val author: String? = null,
    val publisher: String? = null,
    val language: String? = null,
    val identifier: String? = null,
    val description: String? = null,
    val subject: String? = null,
    val date: String? = null,
    val rights: String? = null,
    val contributor: String? = null,
    @SerialName("chapter_count") val chapterCount: Int? = null
)

/** 单本书配置（books/{uuid}.json） */
@Serializable
data class BookConfig(
    val id: String,
    val title: String,
    @SerialName("epub_path") val epubPath: String,
    @SerialName("last_chapter") val lastChapter: Int = 0,
    @SerialName("last_chapter_title") val lastChapterTitle: String? = null,
    @SerialName("last_opened") val lastOpened: Long = System.currentTimeMillis(),
    @SerialName("created_at") val createdAt: Long = System.currentTimeMillis(),
    @SerialName("updated_at") val updatedAt: Long = System.currentTimeMillis(),
    @SerialName("settings") val settings: Map<String, String?> = mapOf(
        "bookmark" to null,
        "note" to null,
        "theme_override" to null
    ),
    @SerialName("file_hash") val fileHash: String? = null,
    val metadata: EpubMetadata? = null
)

/** 远程共享书籍信息 */
@Serializable
data class SharedBookInfo(
    val title: String,
    val hash: String,
    val size: Long
)

/** 已配对设备 */
@Serializable
data class PairedDevice(
    @SerialName("device_id") val deviceId: String,
    @SerialName("device_name") val deviceName: String,
    @SerialName("pairing_uuid") val pairingUuid: String = "",
    @SerialName("remote_public_key_pem") val remotePublicKeyPem: String = "",
    @SerialName("paired_at") val pairedAt: Long
)

/** 通过UDP广播发现的设备 */
@Serializable
data class DiscoveredPeer(
    @SerialName("device_id") val deviceId: String,
    @SerialName("device_name") val deviceName: String,
    val addr: String,
    @SerialName("last_seen") val lastSeen: Long = 0
)

/** 搜索结果 */
@Serializable
data class SearchResult(
    val chapterIndex: Int,
    val chapterTitle: String,
    val blockIndex: Int,
    val context: String,
    val matchStart: Int,
    val matchLen: Int
)

/** 书签 */
@Serializable
data class BookmarkDto(
    val chapter: Int,
    val block: Int = 0,
    @SerialName("created_at") val createdAt: Long = 0
)

/** 高亮颜色 */
enum class HighlightColor { Yellow, Green, Blue, Pink }

/** 高亮 */
@Serializable
data class HighlightDto(
    val id: String,
    val chapter: Int,
    @SerialName("start_block") val startBlock: Int,
    @SerialName("start_offset") val startOffset: Int,
    @SerialName("end_block") val endBlock: Int,
    @SerialName("end_offset") val endOffset: Int,
    val color: String = "Yellow",
    @SerialName("created_at") val createdAt: Long = 0
)

/** 笔记 */
@Serializable
data class NoteDto(
    @SerialName("highlight_id") val highlightId: String,
    val content: String,
    @SerialName("created_at") val createdAt: Long = 0,
    @SerialName("updated_at") val updatedAt: Long = 0
)

/** 纠错记录 */
@Serializable
data class CorrectionDto(
    val chapter: Int,
    @SerialName("block_idx") val blockIdx: Int,
    @SerialName("char_offset") val charOffset: Int,
    val original: String,
    val corrected: String,
    val status: String
)

/** 完整书籍配置（Rust BookConfig 的 Android 映射） */
@Serializable
data class FullBookConfig(
    val id: String,
    val title: String,
    @SerialName("epub_path") val epubPath: String,
    @SerialName("last_chapter") val lastChapter: Int = 0,
    @SerialName("last_chapter_title") val lastChapterTitle: String? = null,
    @SerialName("last_opened") val lastOpened: Long = 0,
    @SerialName("created_at") val createdAt: Long = 0,
    @SerialName("updated_at") val updatedAt: Long = 0,
    val bookmarks: List<BookmarkDto> = emptyList(),
    val highlights: List<HighlightDto> = emptyList(),
    val notes: List<NoteDto> = emptyList(),
    val corrections: List<CorrectionDto> = emptyList()
)
