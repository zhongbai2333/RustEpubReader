package com.zhongbai233.epub.reader.model

import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class ReaderNavigationTest {
    @Test fun historyPreservesSameChapterOffsetsAndRoundTrips() {
        val origin = ReaderReturnPoint(ReaderLocation(2, 4, 37), "真正的章节名", "第 3 / 8 页")
        val next = ReaderLocation(2, 4, 380)
        assertTrue(recordReaderJump(emptyList(), origin, origin.location).isEmpty())
        val recorded = recordReaderJump(emptyList(), origin, next)
        assertEquals(listOf(origin), recorded)
        assertEquals(recorded, recordReaderJump(recorded, origin, next))
        val serializer = ListSerializer(ReaderReturnPoint.serializer())
        assertEquals(recorded, Json.decodeFromString(serializer, Json.encodeToString(serializer, recorded)))
        assertTrue(recorded.dropLast(1).isEmpty())
    }
    @Test fun boundedHistoryRetainsMostRecentOriginsInOrder() {
        var history = emptyList<ReaderReturnPoint>()
        for (index in 0 until 40) history = recordReaderJump(history,
            ReaderReturnPoint(ReaderLocation(index, 1, 8), "Chapter $index", "Page 2"), ReaderLocation(index + 1))
        assertEquals(16, history.size)
        assertEquals(24, history.first().location.chapter)
        assertEquals(39, history.last().location.chapter)
    }
}
