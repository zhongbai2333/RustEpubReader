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

    @Test
    fun forcingSingleColumnReflowsWithoutLosingTextOrCanonicalResumePositions() {
        val chapter = Chapter("Column reflow", paragraphs)
        val block = 3
        val offset = 417
        val pageCounts = mutableListOf<Int>()
        for (forced in listOf(false, true, false)) {
            val columns = readerColumnCount(1280f, forced)
            assertEquals(if (forced) 1 else 2, columns)
            val padding = 1280f * 0.065f
            val width = if (columns == 2) (1280f - padding * 3) / 2 else 1280f - padding * 2
            val pages = paginateContent(chapter, 18f, 650.dp, width.dp, Density(1f), false, titleLineBreaks = ::titleLineBreaks)
            verifySourcePositions(pages, chapter)
            val page = pages.indexOfLast { leaf -> leaf.any { it.sourceBlockIndex == block && it.sourceCharOffset <= offset } }
            assertTrue(page >= 0)
            val fragment = pages[page].last { it.sourceBlockIndex == block && it.sourceCharOffset <= offset }
            val length = (fragment.block as ContentBlock.Paragraph).spans.sumOf { it.text.length }
            assertTrue(offset in fragment.sourceCharOffset until fragment.sourceCharOffset + length)
            pageCounts += pages.size
        }
        assertTrue(pageCounts[1] < pageCounts[0])
        assertEquals(pageCounts[0], pageCounts[2])
        assertEquals(1, readerColumnCount(412f, false))
        assertEquals(1, readerColumnCount(600f, false))
        assertEquals(1, readerColumnCount(2560f, true))
    }
}
