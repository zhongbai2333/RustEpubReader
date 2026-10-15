package com.zhongbai233.epub.reader.util
import org.junit.Assert.assertEquals
import org.junit.Test

class ObservedActivityClockTest {
    @Test fun fiveMinutesIdlePausesAndTouchResumesWithoutAddingTheGap() {
        val clock=ObservedActivityClock(0)
        clock.sample(0,true,false)
        for(second in 1L..360L) clock.sample(second*1000,true,false)
        assertEquals(300000L,clock.totals[0])
        clock.touch(400000);clock.sample(400000,true,false);clock.sample(401000,true,false)
        assertEquals(301000L,clock.totals[0])
    }
    @Test fun playbackIsExclusiveAndBackgroundAndSleepAreExcludedFromReading() {
        val clock=ObservedActivityClock(0)
        clock.sample(0,true,false);clock.sample(1000,true,false)
        clock.sample(2000,true,true);clock.sample(3000,true,true)
        clock.stop(4000);clock.sample(5000,false,false)
        assertEquals(2000L,clock.totals[0]);assertEquals(2000L,clock.totals[1])
        clock.touch(6000);clock.sample(6000,true,false)
        clock.sample(9000,true,false) // Unobserved system suspension is excluded.
        assertEquals(2000L,clock.totals[0])
        clock.sample(10000,false,true);clock.sample(11000,false,true)
        assertEquals(3000L,clock.totals[0]);assertEquals(3000L,clock.totals[1])
    }
}
