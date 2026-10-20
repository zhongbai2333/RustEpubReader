package com.zhongbai233.epub.reader.ui.reader

/** Bounded diagnostics enabled only by the isolated native instrumentation. */
internal object ReaderAccessibilityTurnTrace {
    @Volatile private var enabled = false
    private val events = ArrayDeque<String>()
    @Synchronized fun start() { events.clear(); enabled = true }
    @Synchronized fun record(value: String) {
        if (!enabled) return
        events.addLast("${android.os.SystemClock.uptimeMillis()} $value")
        while (events.size > 256) events.removeFirst()
    }
    @Synchronized fun finish(): List<String> { enabled = false; return events.toList() }
}
