package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.model.*
import org.junit.Assert.*
import org.junit.Test

class ScalarFragmentIntegrityTest {
    @Test fun minimumProgressNeverDividesTheFirstSupplementaryScalar() {
        val source="😀😀😀😀😀😀😀😀中文结尾。"
        val chapter=Chapter("Scalar fragments",listOf(ContentBlock.Paragraph(listOf(TextSpan(source)))))
        for(scale in listOf(0.75f,1f,2.75f)) {
            val pages=paginateContent(chapter,18f,1.dp,10.dp,Density(scale),false)
            val pieces=pages.flatten().map {item->(item.block as ContentBlock.Paragraph).spans.joinToString(""){it.text}}
            assertEquals(source,pieces.joinToString(""))
            assertTrue("An independent fragment must be valid Unicode",pieces.all {Charsets.UTF_8.newEncoder().canEncode(it)})
            assertTrue("Tiny viewports must still advance",pieces.all {it.isNotEmpty()})
        }
    }
}
