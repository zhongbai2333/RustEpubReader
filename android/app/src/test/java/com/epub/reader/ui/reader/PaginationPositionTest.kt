package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.model.*
import org.junit.Assert.*
import org.junit.Test

class PaginationPositionTest {
    // Unit tests exercise position conservation with JVM line boundaries.
    // Production continues to use Android ICU's strict Chinese boundaries.
    private fun titleLineBreaks(text: String): List<Int> {
        val iterator = java.text.BreakIterator.getLineInstance(java.util.Locale.CHINA)
        iterator.setText(text)
        val offsets = mutableListOf<Int>()
        var offset = iterator.first()
        while (offset != java.text.BreakIterator.DONE) {
            offsets.add(offset)
            offset = iterator.next()
        }
        return offsets
    }

    private val paragraphs =
        (0..5).map { index ->
            ContentBlock.Paragraph(
                listOf(
                    TextSpan("第 $index 段，中文标点与 English text。".repeat(45)),
                    TextSpan("带有样式的结尾。", InlineStyle.Bold),
                )
            )
        }

    private fun verifySourcePositions(pages: List<List<PaginatedBlock>>, chapter: Chapter) {
        val blocks = pages.flatten()
        chapter.blocks.forEachIndexed { index, original ->
            val fragments = blocks.filter { it.sourceBlockIndex == index }
            assertTrue(fragments.isNotEmpty())
            var consumed = 0
            val text = StringBuilder()
            for (fragment in fragments) {
                assertEquals(consumed, fragment.sourceCharOffset)
                val spans =
                    when (val block = fragment.block) {
                        is ContentBlock.Paragraph -> block.spans
                        is ContentBlock.Heading -> block.spans
                        else -> error("Unexpected block")
                    }
                val part = spans.joinToString("") { it.text }
                text.append(part)
                consumed += part.length
            }
            val expected =
                when (original) {
                    is ContentBlock.Paragraph -> original.spans.joinToString("") { it.text }
                    is ContentBlock.Heading -> original.spans.joinToString("") { it.text }
                    else -> error("Unexpected block")
                }
            assertEquals(expected, text.toString())
        }
    }

    @Test
    fun reflowAcrossSizesAndTextScalesKeepsEveryCharacterAndResumeOffset() {
        val chapter = Chapter("有导航标题的章节", paragraphs)
        for (height in listOf(420, 760)) for (width in listOf(280, 620)) for (scale in
            listOf(1f, 1.6f)) {
            verifySourcePositions(
                paginateContent(chapter, 18f, height.dp, width.dp, Density(2f, scale), true, titleLineBreaks = ::titleLineBreaks),
                chapter,
            )
        }
    }

    @Test
    fun suppressingGeneratedChapterTitleDoesNotChangeSourceBlockIndices() {
        val chapter =
            Chapter(
                "第 1 章",
                listOf(ContentBlock.Heading(1, listOf(TextSpan("EPUB 自带标题")))) + paragraphs,
            )
        for (showTitle in listOf(true, false)) {
            verifySourcePositions(
                paginateContent(chapter, 18f, 600.dp, 320.dp, Density(2f), showTitle, titleLineBreaks = ::titleLineBreaks),
                chapter,
            )
        }
    }
}
