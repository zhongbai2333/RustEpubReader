package com.zhongbai233.epub.reader.ui.reader

import com.zhongbai233.epub.reader.model.HighlightDto
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test
import java.io.File

class ReaderTextCoordinatesTest {
    private val contract = Json.parseToJsonElement(File(checkNotNull(System.getProperty("reader.coordinateContract"))).readText()).jsonObject
    @Test fun sharedVectorsRoundTripAndNeverPersistHalfASurrogate() {
        assertEquals("unicode_scalar", contract.getValue("persisted_unit").jsonPrimitive.content)
        for (item in contract.getValue("cases").jsonArray) {
            val value = item.jsonObject; val text = value.getValue("source").jsonPrimitive.content
            value.getValue("scalar_to_utf16").jsonArray.forEachIndexed { scalar, utf16 ->
                assertEquals(utf16.jsonPrimitive.int, utf16TextOffset(text, scalar))
            }
            value.getValue("utf16_to_scalar").jsonArray.forEachIndexed { utf16, scalar ->
                assertEquals(scalar.jsonPrimitive.int, unicodeScalarOffset(text, utf16))
            }
            assertEquals(0, unicodeScalarOffset(text, -1)); assertEquals(0, utf16TextOffset(text, -1))
            assertEquals(text.length, utf16TextOffset(text, Int.MAX_VALUE))
        }
    }
    @Test fun aHighlightIsConservedAcrossEveryNativeFragmentAndNeverLeaksChapters() {
        for (item in contract.getValue("cases").jsonArray) {
            val value=item.jsonObject;val text=value.getValue("source").jsonPrimitive.content
            val boundaries=value.getValue("scalar_to_utf16").jsonArray.map {it.jsonPrimitive.int}
            for (range in value.getValue("ranges").jsonArray) {
                val selected=range.jsonObject
                val highlight=HighlightDto("fixture",7,3,selected.getValue("start").jsonPrimitive.int,3,selected.getValue("end").jsonPrimitive.int)
                val painted=StringBuilder()
                // Adjacent scalar-sized fragments include astral and combining boundaries.
                for ((start,end) in boundaries.zipWithNext()) {
                    val local=highlightFragmentRange(highlight,7,3,text,start,end-start)
                    if(local!=null) painted.append(text.substring(start+local.start,start+local.end))
                    assertNull(highlightFragmentRange(highlight,8,3,text,start,end-start))
                }
                assertEquals(selected.getValue("text").jsonPrimitive.content,painted.toString())
            }
        }
    }
    @Test fun multiBlockRangesClipTheirFirstAndLastFragments() {
        val highlight=HighlightDto("fixture",0,1,2,3,2)
        assertEquals(HighlightTextRange(0,2),highlightFragmentRange(highlight,0,1,"甲😀乙丙",3,2))
        assertEquals(HighlightTextRange(0,2),highlightFragmentRange(highlight,0,2,"中間",0,2))
        assertEquals(HighlightTextRange(0,3),highlightFragmentRange(highlight,0,3,"甲😀乙",0,4))
        assertNull(highlightFragmentRange(highlight,0,4,"甲",0,1))
    }
}
