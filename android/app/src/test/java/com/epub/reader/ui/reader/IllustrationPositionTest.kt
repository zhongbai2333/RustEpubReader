package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.model.*
import org.junit.Assert.*
import org.junit.Test

class IllustrationPositionTest {
    @Test fun supplementalImagesPreserveTextIndicesAndReceivePageHeightLimits() {
        val text = "插图不能改变阅读进度、高亮或纠错的文字坐标。".repeat(80)
        val chapter = Chapter("插图", listOf(ContentBlock.Paragraph(listOf(TextSpan(text)))),
            images = listOf(ChapterImage(0, false, "invalid", "before"), ChapterImage(0, true, "invalid", "after")))
        for (width in listOf(280, 620)) {
            val rendered = paginateContent(chapter, 18f, 600.dp, width.dp, Density(2f), true).flatten()
            val images = rendered.filter { it.block is ContentBlock.Image }
            assertEquals(2, images.size)
            assertTrue(images.all { it.sourceBlockIndex == 0 && it.imageMaxHeight.value in 1f..600f })
            assertEquals(text.length, images.last().sourceCharOffset)
            val paragraphs = rendered.filter { it.block is ContentBlock.Paragraph }
            assertEquals(text, paragraphs.joinToString("") { (it.block as ContentBlock.Paragraph).spans.joinToString("") { span -> span.text } })
            var offset = 0
            paragraphs.forEach { fragment ->
                assertEquals(0, fragment.sourceBlockIndex)
                assertEquals(offset, fragment.sourceCharOffset)
                offset += (fragment.block as ContentBlock.Paragraph).spans.sumOf { it.text.length }
            }
        }
    }
}
