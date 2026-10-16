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

/** 文本片段 — 对应 Rust 版本的 TextSpan */
@Serializable
data class TextSpan(
    val text: String,
    val style: InlineStyle = InlineStyle.Normal,
    val linkUrl: String? = null
)

/** 内容块 — 对应 Rust 版本的 ContentBlock */
@Serializable
sealed class ContentBlock {
    @Serializable
    @SerialName("heading")
    data class Heading(
        val level: Int,
        val spans: List<TextSpan>,
        @SerialName("anchor_id") val anchor_id: String? = null,
        @SerialName("native_style") val nativeStyle: NativeStyle = NativeStyle()
    ) : ContentBlock()

    @Serializable
    @SerialName("paragraph")
    data class Paragraph(
        val spans: List<TextSpan>,
        @SerialName("anchor_id") val anchor_id: String? = null,
        @SerialName("native_style") val nativeStyle: NativeStyle = NativeStyle()
    ) : ContentBlock()

    @Serializable
    @SerialName("image")
    data class Image(val data: String, val alt: String? = null, @SerialName("native_style") val nativeStyle: NativeStyle = NativeStyle()) : ContentBlock()

    @Serializable
    @SerialName("separator")
    data object Separator : ContentBlock()

    @Serializable
    @SerialName("blankLine")
    data object BlankLine : ContentBlock()
}

/** Illustrations retain the original shared text coordinates. */
@Serializable
data class ChapterImage(val block: Int, val after: Boolean = false, val data: String, val alt: String? = null, val nativeStyle: NativeStyle = NativeStyle()) {
    fun content() = ContentBlock.Image(data, alt, nativeStyle)
}

/** 单个章节的数据传输对象 */
@Serializable
data class ChapterDto(
    val title: String,
    @SerialName("sourceHref")
    val sourceHref: String? = null,
    val blocks: List<ContentBlock>,
    val images: List<ChapterImage> = emptyList()
)

@Serializable
data class TocEntryDto(
    val title: String,
    val chapterIndex: Int
)

@Serializable
data class ChapterReviewEntry(
    @SerialName("main") val main: Int,
    @SerialName("review") val review: Int
)

@Serializable
data class BookMetadataDto(
    val title: String,
    val chapterCount: Int,
    val chapterTitles: List<String> = emptyList(),
    val toc: List<TocEntryDto>,
    val hasCover: Boolean,
    @SerialName("chapterReviews") val chapterReviews: List<ChapterReviewEntry> = emptyList(),
    @SerialName("reviewChapterIndices") val reviewChapterIndices: List<Int> = emptyList()
)

/** 方便与老代码兼容保留的 Chapter 定义（如果需要映射） */
data class Chapter(
    val title: String,
    val blocks: List<ContentBlock>,
    val sourceHref: String? = null,
    val images: List<ChapterImage> = emptyList()
)

/** EPUB 书籍 (兼容用) */
data class EpubBook(
    val title: String,
    var chapters: List<Chapter>,
    val toc: List<Pair<String, Int>>,
    val coverData: ByteArray? = null,
    val chapterTitles: List<String> = emptyList()
) {
    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other !is EpubBook) return false
        // Publication bodies are lazy: structural equality would fetch every chapter.
        return title == other.title && chapters === other.chapters
    }

    override fun hashCode(): Int = title.hashCode() * 31 + System.identityHashCode(chapters)
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
    @JsonNames("lastBlock", "last_block")
    val lastBlock: Int = 0,
    @JsonNames("lastCharOffset", "last_char_offset")
    val lastCharOffset: Int = 0,
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
    @SerialName("last_block") val lastBlock: Int = 0,
    @SerialName("last_char_offset") val lastCharOffset: Int = 0,
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
    @SerialName("last_block") val lastBlock: Int = 0,
    @SerialName("last_char_offset") val lastCharOffset: Int = 0,
    @SerialName("last_chapter_title") val lastChapterTitle: String? = null,
    @SerialName("last_opened") val lastOpened: Long = 0,
    @SerialName("created_at") val createdAt: Long = 0,
    @SerialName("updated_at") val updatedAt: Long = 0,
    val bookmarks: List<BookmarkDto> = emptyList(),
    val highlights: List<HighlightDto> = emptyList(),
    val notes: List<NoteDto> = emptyList(),
    val corrections: List<CorrectionDto> = emptyList()
)

@Serializable data class NativeDimension(val px: Float=0f,val em: Float=0f,val percent: Float=0f) { fun resolve(font: Float,extent: Float): Float=(px+em*font+(if(extent.isFinite())percent*extent else 0f)).coerceAtLeast(0f) }
@Serializable data class NativeInline(val start:Int,val end:Int,val size:Float?=null,val color:String?=null,val family:String?=null,val bold:Boolean?=null,val italic:Boolean?=null,val underline:Boolean=false,val strike:Boolean=false,val hidden:Boolean=false,val baseline:Float=0f,@SerialName("small_caps")val smallCaps:Boolean=false,val note:String?=null)
@Serializable data class NativeList(val start:Int,val end:Int,val text:String,val depth:Int,val marker:String)
@Serializable data class NativeRuby(val start:Int,val end:Int,val text:String)
@Serializable data class NativeCell(val start:Int,val end:Int,val text:String,val header:Boolean=false,val span:Int=1,val align:String?=null)
@Serializable data class NativeTable(val cells:List<NativeCell>,val columns:Int)
@Serializable data class NativeNote(val text:String,val href:String)
@Serializable data class NativeStyle(
 val align:String?=null,val size:Float?=null,val line:Float?=null,val indent:Float?=null,val before:Float?=null,val after:Float?=null,val color:String?=null,val family:String?=null,val bold:Boolean=false,val italic:Boolean=false,val hidden:Boolean=false,val action:Int?=null,val text:String?=null,
 @SerialName("size_px")val sizePx:Float?=null,val inline:List<NativeInline> = emptyList(),val ruby:List<NativeRuby> = emptyList(),val table:NativeTable?=null,val list:List<NativeList> = emptyList(),@SerialName("list_depth")val listDepth:Int=0,val quote:Boolean=false,val pre:Boolean=false,
 @SerialName("inset_left")val insetLeft:NativeDimension?=null,@SerialName("inset_right")val insetRight:NativeDimension?=null,
 @SerialName("padding_left")val paddingLeft:NativeDimension?=null,@SerialName("padding_right")val paddingRight:NativeDimension?=null,@SerialName("padding_top")val paddingTop:NativeDimension?=null,@SerialName("padding_bottom")val paddingBottom:NativeDimension?=null,
 @SerialName("margin_top")val marginTop:NativeDimension?=null,@SerialName("margin_bottom")val marginBottom:NativeDimension?=null,@SerialName("indent_length")val indentLength:NativeDimension?=null,
 @SerialName("image_width")val imageWidth:NativeDimension?=null,@SerialName("image_height")val imageHeight:NativeDimension?=null,@SerialName("image_max_width")val imageMaxWidth:NativeDimension?=null,@SerialName("image_max_height")val imageMaxHeight:NativeDimension?=null
) {
 fun font(base:Float):Float=(sizePx?:base*(size?:1f)).coerceIn(6f,96f)
 fun lineScale(base:Float):Float=((line?:base)*inline.filter { !it.hidden }.mapNotNull {it.size}.fold(1f,::maxOf)).coerceAtLeast(if(ruby.isEmpty())1f else 1.7f)
 fun imageSize(naturalWidth:Float,naturalHeight:Float,font:Float,width:Float,height:Float):Pair<Float,Float>{
  val validHeight=imageHeight?.takeIf {height.isFinite()||it.px!=0f||it.em!=0f}
  val desiredWidth=imageWidth?.resolve(font,width)?:naturalWidth;val desiredHeight=validHeight?.resolve(font,height)?:naturalHeight
  val requested=when{imageWidth!=null&&validHeight!=null->minOf(desiredWidth/naturalWidth,desiredHeight/naturalHeight);imageWidth!=null->desiredWidth/naturalWidth;validHeight!=null->desiredHeight/naturalHeight;else->1f}
  val scale=minOf(requested,(imageMaxWidth?.resolve(font,width)?:width).coerceAtMost(width)/naturalWidth,(imageMaxHeight?.takeIf {height.isFinite()||it.px!=0f||it.em!=0f}?.resolve(font,height)?:height).coerceAtMost(height)/naturalHeight).coerceAtLeast(0f)
  return naturalWidth*scale to naturalHeight*scale
 }
 // Rust ranges count Unicode scalars; Compose and persisted Android fragments use UTF-16.
 fun inUtf16(source:String):NativeStyle {
  val offsets=mutableListOf(0);var cursor=0;while(cursor<source.length){cursor+=Character.charCount(source.codePointAt(cursor));offsets.add(cursor)}
  fun at(index:Int)=offsets[index.coerceIn(0,offsets.lastIndex)]
  return copy(inline=inline.map {it.copy(start=at(it.start),end=at(it.end))},ruby=ruby.map {it.copy(start=at(it.start),end=at(it.end))})
 }
}
@Serializable data class NativePresentation(val styles: List<NativeStyle> = emptyList(),val errors: List<String> = emptyList())
fun ContentBlock.nativeStyle(): NativeStyle = when(this){is ContentBlock.Paragraph->nativeStyle;is ContentBlock.Heading->nativeStyle;is ContentBlock.Image->nativeStyle;else->NativeStyle()}
