package com.zhongbai233.epub.reader.model

import org.junit.Assert.*
import org.junit.Test

class LazyChapterListTest {
    @Test fun publicationEqualityNeverFetchesChapterBodies() {
        var loads=0
        val source=LazyChapterList(3545) { loads++; Chapter("Chapter $it", emptyList()) }
        val book=EpubBook("Book",source,emptyList())
        assertEquals(book,book.copy())
        assertEquals(book.hashCode(),book.copy().hashCode())
        assertNotEquals(book,book.copy(chapters=LazyChapterList(3545) { error("Unexpected load") }))
        assertEquals(0,loads)
        assertEquals(0,source.residentCount)
    }
    @Test fun recentlyUsedBodiesSurviveWhileOldBodiesAreEvicted() {
        val loaded=mutableListOf<Int>()
        val source=LazyChapterList(3545) { loaded+=it; Chapter("Chapter $it",emptyList()) }
        for(index in 0..7) source[index]
        source[0]
        source[8]
        assertEquals(8,source.residentCount)
        source[0]
        assertEquals(9,loaded.size)
        source[1]
        assertEquals(10,loaded.size)
        assertEquals(1,loaded.last())
        source[3544]
        assertEquals(8,source.residentCount)
    }
}
