package com.zhongbai233.epub.reader.viewmodel

import kotlinx.coroutines.Job

/** Main-thread opening ownership; cancelling invalidates late results before UI publication. */
internal class BookOpeningSession(private val cancelNative: (Long) -> Unit) {
    var job: Job? = null
    var generation: Int = 0
        private set
    var ticket: Long = 0
        private set
    fun started(ticket: Long) { this.ticket = ticket }
    fun finished(ticket: Long) { if (this.ticket == ticket) this.ticket = 0 }
    fun cancel() {
        generation++
        job?.cancel()
        job = null
        // Native cancellation can finish a blocking JNI call with IOException.
        // Invalidate the owner before that completion is made runnable.
        if (ticket != 0L) cancelNative(ticket)
    }
}
