package com.zhongbai233.epub.reader.ui.reader

import com.zhongbai233.epub.reader.model.*
import org.junit.Assert.*
import org.junit.Test

class BookPreparationTest {
    @Test fun stylingTheResumeChapterDoesNotReadTheWholePublication() {
        val reads=mutableListOf<Int>()
        val source=object:AbstractList<Chapter>() {
            override val size=500
            override fun get(index:Int):Chapter {reads+=index;return Chapter("Chapter $index",listOf(ContentBlock.Paragraph(listOf(TextSpan("text")))))}
        }
        val lazy=NativeChapterList(source,mapOf(230 to NativePresentation()))
        assertEquals(500,lazy.size)
        assertTrue(reads.isEmpty())
        assertEquals("Chapter 230",lazy[230].title)
        assertSame(lazy[230],lazy[230])
        assertEquals(listOf(230),reads)
    }
    @Test fun effectKeysDoNotTraverseLazyChaptersOrResetForNeighbourStyles() {
        val source=object:AbstractList<Chapter>() {
            override val size=500
            override fun get(index:Int):Chapter = error("Effect key read chapter $index")
        }
        val first=NativeChapterList(source,emptyMap())
        val second=NativeChapterList(source,mapOf(1 to NativePresentation()))
        assertEquals(publicationKey(first),publicationKey(second))
        assertEquals(ChapterListIdentity(first),ChapterListIdentity(first))
        assertNotEquals(ChapterListIdentity(first),ChapterListIdentity(second))
    }
    @Test fun onlyVisiblePendingIllustrationsDelayTheReader() {
        val images=VisibleReaderImages()
        val visible=Any(); val neighbour=Any()
        assertFalse(images.ready)
        images.viewport=androidx.compose.ui.geometry.Rect(0f,0f,320f,640f)
        images.position(neighbour,androidx.compose.ui.geometry.Rect(0f,900f,300f,1200f))
        assertTrue(images.ready)
        images.position(visible,androidx.compose.ui.geometry.Rect(0f,40f,300f,400f))
        assertFalse(images.ready)
        images.prepared(visible,true)
        assertTrue(images.ready)
        images.position(neighbour,androidx.compose.ui.geometry.Rect(0f,500f,300f,800f))
        assertFalse(images.ready)
        images.remove(neighbour)
        assertTrue(images.ready)
    }
    @Test fun cachedPagesCannotEscapeTheirFontGeometryOrPublicationStyles() {
        val cache=PageLayoutCache(2)
        val chapter=Chapter("First",listOf(ContentBlock.Paragraph(listOf(TextSpan("original")))))
        val pages=listOf(listOf(PaginatedBlock(chapter.blocks[0],0)))
        cache.put(0,chapter,"font-18/width-320",pages)
        assertSame(pages,cache.get(0,chapter,"font-18/width-320"))
        assertNull(cache.get(0,chapter,"font-24/width-320"))
        assertNull(cache.get(0,chapter.copy(title="Changed"),"font-18/width-320"))
        cache.put(1,chapter,"font-18/width-320",pages)
        cache.put(2,chapter,"font-18/width-320",pages)
        assertNull(cache.peek(0,"font-18/width-320"))
    }
}
