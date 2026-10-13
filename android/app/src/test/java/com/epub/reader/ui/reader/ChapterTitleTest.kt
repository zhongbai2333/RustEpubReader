package com.zhongbai233.epub.reader.ui.reader

import com.zhongbai233.epub.reader.model.*
import org.junit.Assert.*
import org.junit.Test

class ChapterTitleTest {
    private fun heading(text: String) = ContentBlock.Heading(1, listOf(TextSpan(text)))

    @Test
    fun parserFallbackDoesNotPrependAnotherTitleToEpubHeading() {
        assertFalse(shouldRenderChapterTitle(Chapter("第 1 章", listOf(heading("第一章 · 留下阅读的时间")))))
    }

    @Test
    fun matchingTitleIsShownOnlyOnce() {
        assertFalse(shouldRenderChapterTitle(Chapter("慢一点看见", listOf(heading("慢一点 看见")))))
    }

    @Test
    fun distinctSectionHeadingKeepsChapterTitle() {
        assertTrue(shouldRenderChapterTitle(Chapter("慢一点看见", listOf(heading("走进森林")))))
    }

    @Test
    fun chapterWithoutItsOwnHeadingKeepsNavigationTitle() {
        assertTrue(
            shouldRenderChapterTitle(
                Chapter("第 1 章", listOf(ContentBlock.Paragraph(listOf(TextSpan("正文")))))
            )
        )
    }
}
