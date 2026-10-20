package com.zhongbai233.epub.reader.ui.reader

import com.zhongbai233.epub.reader.model.ContentBlock
import com.zhongbai233.epub.reader.model.HighlightDto

/** Persisted/core offsets count Unicode scalars; Compose fragments count UTF-16 units. */
internal fun readerSourceText(block: ContentBlock?): String = when (block) {
    is ContentBlock.Paragraph -> block.spans.joinToString("") { it.text }
    is ContentBlock.Heading -> block.spans.joinToString("") { it.text }
    else -> ""
}

internal fun unicodeScalarOffset(text: String, utf16: Int): Int {
    var end = utf16.coerceIn(0, text.length)
    // Selection and external callers must never persist half of a surrogate pair.
    if (end in 1 until text.length && Character.isHighSurrogate(text[end - 1]) && Character.isLowSurrogate(text[end])) end--
    return text.codePointCount(0, end)
}

internal fun utf16TextOffset(text: String, scalar: Int): Int =
    text.offsetByCodePoints(0, scalar.coerceIn(0, text.codePointCount(0, text.length)))

internal data class HighlightTextRange(val start: Int, val end: Int)

/** Intersect one source highlight with a painted fragment; end is exclusive. */
internal fun highlightFragmentRange(
    highlight: HighlightDto, chapter: Int, block: Int, source: String,
    fragmentStart: Int, fragmentLength: Int,
): HighlightTextRange? {
    if (highlight.chapter != chapter || block !in highlight.startBlock..highlight.endBlock || fragmentStart < 0 || fragmentLength <= 0) return null
    val start = if (block == highlight.startBlock) utf16TextOffset(source, highlight.startOffset) else 0
    val end = if (block == highlight.endBlock) utf16TextOffset(source, highlight.endOffset) else source.length
    val fragmentEnd = (fragmentStart.toLong() + fragmentLength).coerceAtMost(source.length.toLong()).toInt()
    val left = maxOf(start, fragmentStart)
    val right = minOf(end, fragmentEnd)
    return if (left < right) HighlightTextRange(left - fragmentStart, right - fragmentStart) else null
}
