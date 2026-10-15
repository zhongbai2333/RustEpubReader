package com.zhongbai233.epub.reader.util

/** Monotonic observation avoids counting sleep, foreground gaps, and idle intervals. */
internal class ObservedActivityClock(now: Long) {
    private var last = now
    private var touched = now
    private var mode = -1
    val totals = LongArray(2)
    fun touch(now: Long) { touched=now }
    fun reset(now: Long) {last=now;touched=now;mode=-1;totals.fill(0)}
    fun sample(now: Long, reading: Boolean, listening: Boolean): Boolean {
        val delta=now-last
        if(mode>=0 && delta in 0..2000) totals[mode]+=delta
        last=now
        val next=if(listening) 1 else if(reading && now-touched<300000) 0 else -1
        val changed=next!=mode;mode=next;return changed
    }
    fun stop(now: Long) {sample(now,false,false)}
}
