/**
 * PaginationUtils.kt — 分页与排版计算工具
 *
 * 负责将章节内容按屏幕尺寸进行分页，估算文本块高度，处理 CJK 标点禁则等排版逻辑。
 */
package com.zhongbai233.epub.reader.ui.reader

import android.graphics.BitmapFactory
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.model.*
import kotlin.math.ceil

// ─── CJK 标点禁则 ───

internal val NO_BREAK_BEFORE = charArrayOf(
    '，', '。', '！', '？', '；', '：', '、', '\u201D', '\'', '）', '》',
    '」', '』', '】', '〉', '〕', '〗', '〙', '〛', '，', '．',
    '！', '？', '）', '：', '；', '\u201D', '\'', '」', '〉', '》',
    '】', '〗', '〙', '〛', '.', ',', '!', '?', ';', ':', ')',
    ']', '}', '…', '‥', '％', '‰', '℃', 'ー', '〜', '～'
)

internal val NO_BREAK_AFTER = charArrayOf(
    '\u201C', '\'', '（', '《', '「', '『', '【', '〈', '〔', '〖',
    '〘', '〚', '（', '\u201C', '\'', '「', '〈', '《', '【', '〖',
    '〘', '〚', '(', '[', '{'
)

/** 分页缓存最大章节数 */
internal const val PAGINATION_CACHE_MAX_SIZE = 10

/**
 * 手动将标题按行宽拆成多行（插入 \n），避免 Compose 自动 wrap 导致 lineHeight 不生效。
 */
internal fun breakTitleIntoLines(title: String, contentWidthPx: Float, titleFontSizeSp: Float, spToPx: Float): String {
    // 使用 1.1f 倍宽作为字符估算，防止极端字体时一行的字被挤到下一行
    val charWidth = titleFontSizeSp * spToPx * 1.1f
    val charsPerLine = (contentWidthPx / charWidth).toInt().coerceAtLeast(4)
    if (title.length <= charsPerLine) return title
    val sb = StringBuilder()
    var i = 0
    while (i < title.length) {
        if (i > 0) sb.append('\n')
        val end = (i + charsPerLine).coerceAtMost(title.length)
        sb.append(title, i, end)
        i = end
    }
    return sb.toString()
}

internal fun paginateContent(
    chapter: Chapter,
    fontSize: Float,
    availableHeight: Dp,
    contentWidth: Dp,
    density: androidx.compose.ui.unit.Density,
    showChapterTitle: Boolean,
    titleVPaddingDp: Dp = 32.dp,
    lineSpacing: Float = 1.5f,
    paraSpacing: Float = 0.5f,
    textIndentChars: Int = 2,
    titleFontScale: Float = 1.5f
): List<List<ContentBlock>> {
    val contentWidthPx = with(density) { contentWidth.toPx() }
    // sp → px 需要乘 fontScale（处理系统字体缩放）
    val spToPx = density.fontScale * density.density
    val lineHeight = fontSize * lineSpacing * spToPx
    
    // 采用更精准的容错边距（不再一次性扣除40dp+整行高，那会导致严重底部留白）
    // 给系统布局误差保留 0.5 行高的弹性空间足矣，因为屏幕 padding 本身已避开页码
    val safetyMarginPx = lineHeight * 0.5f
    val maxHeightPx = with(density) { availableHeight.toPx() } - safetyMarginPx
    
    val pages = mutableListOf<List<ContentBlock>>()
    var currentPage = mutableListOf<ContentBlock>()
    var currentHeight = 0f
    var isFirstPage = true

    // 第一页有章节标题占的高度
    if (isFirstPage && showChapterTitle) {
        // 标题实际渲染 lineHeight = (fontSize * 2.2f).sp
        val titleLineHeightPx = fontSize * titleFontScale * 1.45f * spToPx
        // 使用 breakTitleIntoLines 保持分行逻辑一致
        val brokenTitle = breakTitleIntoLines(chapter.title, contentWidthPx, fontSize * titleFontScale, spToPx)
        val titleLines = brokenTitle.count { it == '\n' } + 1
        // 标题行高 + padding(top = vPadding*0.5 + bottom = vPadding) + 30% 缓冲
        val titlePaddingPx = with(density) { titleVPaddingDp.toPx() } * 1.5f
        currentHeight += (titleLines * titleLineHeightPx + titlePaddingPx) * 1.3f
    }

    for (block in chapter.blocks) {
        val blockHeight = estimateBlockHeight(block, fontSize, lineHeight, contentWidthPx, density, paraSpacing, textIndentChars, titleFontScale)

        if (currentHeight + blockHeight > maxHeightPx && currentPage.isNotEmpty()) {
            pages.add(currentPage.toList())
            currentPage = mutableListOf()
            currentHeight = 0f
            isFirstPage = false
        }

        currentPage.add(block)
        currentHeight += blockHeight
    }

    if (currentPage.isNotEmpty()) {
        pages.add(currentPage.toList())
    }

    return pages.ifEmpty { listOf(emptyList()) }
}

internal fun estimateBlockHeight(
    block: ContentBlock,
    fontSize: Float,
    lineHeight: Float,
    contentWidthPx: Float,
    density: androidx.compose.ui.unit.Density,
    paraSpacing: Float = 0.5f,
    textIndentChars: Int = 2,
    titleFontScale: Float = 1.5f
): Float {
    return when (block) {
        is ContentBlock.Heading -> {
            val scale = when (block.level) {
                1 -> titleFontScale * 1.3f
                2 -> titleFontScale * 1.1f
                3 -> titleFontScale * 0.9f
                else -> titleFontScale * 0.8f
            }
                .coerceAtLeast(1.0f)
            val spToPx = density.fontScale * density.density
            var cjkCount = 0
            var asciiCount = 0
            block.spans.forEach { span ->
                span.text.forEach { ch ->
                    if (ch.code > 255) cjkCount++ else asciiCount++
                }
            }
            // 准确区分纯英文和中文估算不同字宽，不一拍脑门乘以全行缩减 1.1f 倍
            val estimatedTextWidthPx = (cjkCount * 1.05f + asciiCount * 0.6f) * (fontSize * scale * spToPx)
            val lines = ceil(estimatedTextWidthPx / contentWidthPx).toInt().coerceAtLeast(1)
            // 顶部 1.2 + 底部 1.8 = 3.0 padding (增加标题与正文间距)
            lines * lineHeight * scale + fontSize * 3.0f * density.density
        }
        is ContentBlock.Paragraph -> {
            val spToPx = density.fontScale * density.density
            var cjkCount = 0
            var asciiCount = 0
            block.spans.forEach { span ->
                span.text.forEach { ch ->
                    if (ch.code > 255) cjkCount++ else asciiCount++
                }
            }
            // 中文字体平均严格占宽 1.05em (包括字距)，英文平均大约在 0.55em
            val estimatedTextWidthPx = (cjkCount * 1.05f + asciiCount * 0.55f) * (fontSize * spToPx)
            // 加上首行缩进的 em + 适当的尾行排版容错
            val totalWidthPx = estimatedTextWidthPx + (fontSize * spToPx * (textIndentChars + 0.5f))
            val lines = ceil(totalWidthPx / contentWidthPx).toInt().coerceAtLeast(1)
            lines * lineHeight + fontSize * (paraSpacing * 2f) * density.density
        }
        is ContentBlock.Image -> {
            estimateImageBlockHeight(
                data = block.data,
                contentWidthPx = contentWidthPx,
                density = density,
                fontSize = fontSize
            )
        }
        is ContentBlock.Separator -> lineHeight + fontSize * density.density
        is ContentBlock.BlankLine -> fontSize * 0.5f * density.density
    }
}

internal fun estimateImageBlockHeight(
    data: String,
    contentWidthPx: Float,
    density: androidx.compose.ui.unit.Density,
    fontSize: Float
): Float {
    val options = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    val bytes = android.util.Base64.decode(data, android.util.Base64.DEFAULT)
    BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options)
    val w = options.outWidth
    val h = options.outHeight
    if (w <= 0 || h <= 0) {
        return (fontSize * 8f * density.density).coerceAtLeast(96f)
    }

    val ratio = h.toFloat() / w.toFloat()
    val imageHeight = contentWidthPx * ratio
    val verticalPadding = fontSize * 0.7f * density.density
    return imageHeight + verticalPadding
}

internal fun shouldRenderChapterTitle(chapter: Chapter): Boolean {
    val first = chapter.blocks.firstOrNull() as? ContentBlock.Heading ?: return true
    val headingText = first.spans.joinToString("") { it.text }.trim().replace(" ", "")
    val chapterText = chapter.title.trim().replace(" ", "")
    if (headingText.isBlank() || chapterText.isBlank()) return true
    return headingText != chapterText
}
