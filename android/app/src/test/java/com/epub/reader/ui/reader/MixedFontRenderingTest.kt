package com.zhongbai233.epub.reader.ui.reader
import androidx.compose.ui.text.font.FontFamily
import com.zhongbai233.epub.reader.model.TextSpan
import androidx.compose.ui.graphics.Color
import org.junit.Assert.*
import org.junit.Test
class MixedFontRenderingTest {
    @Test fun latinOverridesUseUtf16RangesWithoutTouchingChineseOrEmoji(){
        val text="中文 A😀 B café"
        val value=mixedScriptText(text,FontFamily.Serif,FontFamily.Monospace)
        assertEquals(text,value.text)
        assertEquals(listOf("A","B","café"),value.spanStyles.map{text.substring(it.start,it.end)})
        assertTrue(value.spanStyles.all{it.item.fontFamily==FontFamily.Monospace && it.item.fontSize == androidx.compose.ui.unit.TextUnit.Unspecified})
    }
    @Test fun followingChineseAddsNoLatinOverrides(){
        val text="中文 English 123"
        val value=mixedScriptText(text,FontFamily.Serif,null)
        assertEquals(text,value.text);assertTrue(value.spanStyles.isEmpty())
    }
    @Test fun inlineMeasurementAndRenderingShareTheSameLatinRanges(){
        val text="中文 😀 Hello 123"
        val value=buildSpanAnnotatedString(listOf(TextSpan(text)),18f,Color.Black,Color.Blue,FontFamily.Serif,null,latinFamily=FontFamily.Monospace)
        val overrides=value.spanStyles.filter{it.item.fontFamily==FontFamily.Monospace}
        assertEquals(listOf("Hello","123"),overrides.map{text.substring(it.start,it.end)})
    }
}
