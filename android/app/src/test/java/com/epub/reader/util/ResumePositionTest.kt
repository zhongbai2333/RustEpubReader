package com.zhongbai233.epub.reader.util

import org.junit.Assert.assertEquals
import org.junit.Test

class ResumePositionTest {
    @Test fun rereadPositionBeatsOlderLargerPreference() { assertEquals(2, resolveResumeChapter(2, 10, 10)) }
    @Test fun chapterZeroIsAValidSyncedPosition() { assertEquals(0, resolveResumeChapter(0, 8, 8)) }
    @Test fun legacyPositionIsUsedOnlyWithoutLibraryEntry() { assertEquals(8, resolveResumeChapter(null, 0, 8)) }
    @Test fun negativePositionIsClamped() { assertEquals(0, resolveResumeChapter(-1, 2, 3)) }
}
