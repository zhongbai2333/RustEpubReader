package com.zhongbai233.epub.reader.ui.reader

import org.junit.Assert.assertEquals
import org.junit.Test

class HighlightRenderingTest {
    @Test fun boundaryPreviewsUseTheirActualChapterDuringEveryAnimation() {
        assertEquals(19, renderedHighlightChapter(20, false, true, false))
        assertEquals(21, renderedHighlightChapter(20, false, false, true))
        assertEquals(20, renderedHighlightChapter(20, false, false, false))
    }

    @Test fun committedChapterHandoffKeepsTheNewChaptersAnnotations() {
        assertEquals(20, renderedHighlightChapter(20, true, true, false))
        assertEquals(20, renderedHighlightChapter(20, true, false, true))
    }
}
