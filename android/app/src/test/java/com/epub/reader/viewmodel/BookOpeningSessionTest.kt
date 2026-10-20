package com.zhongbai233.epub.reader.viewmodel

import kotlinx.coroutines.Job
import org.junit.Assert.*
import org.junit.Test

class BookOpeningSessionTest {
    @Test fun cancellingInvalidatesLateResultsAndAnOldFinishCannotClearANewerTicket() {
        val cancelled=mutableListOf<Long>();val session=BookOpeningSession {cancelled.add(it)}
        val original=session.generation;val work=Job()
        session.job=work;session.started(11);session.cancel()
        assertTrue(work.isCancelled);assertNull(session.job);assertNotEquals(original,session.generation)
        assertEquals(listOf(11L),cancelled)
        session.started(12);session.finished(11)
        assertEquals(12L,session.ticket)
        session.cancel();session.finished(12)
        assertEquals(listOf(11L,12L),cancelled);assertEquals(0L,session.ticket)
    }
    @Test fun cancellationBeforeDispatchStillInvalidatesTheGeneration() {
        val session=BookOpeningSession {error("No native operation should exist")}
        repeat(64) {val before=session.generation;session.cancel();assertEquals(before+1,session.generation)}
        assertEquals(0L,session.ticket)
    }
    @Test fun ownerAndJobAreInvalidatedBeforeTheNativeFailureCanComplete() {
        val work=Job()
        lateinit var session:BookOpeningSession
        session=BookOpeningSession {
            assertTrue(work.isCancelled)
            assertNull(session.job)
            assertEquals(1,session.generation)
        }
        session.job=work;session.started(7)
        session.cancel()
    }
}
