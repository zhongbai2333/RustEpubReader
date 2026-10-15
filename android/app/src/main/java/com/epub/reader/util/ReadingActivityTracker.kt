package com.zhongbai233.epub.reader.util

import android.os.SystemClock
import com.zhongbai233.epub.reader.RustBridge
import kotlinx.coroutines.*
import kotlinx.coroutines.channels.Channel
import org.json.JSONObject
import java.util.UUID
import java.util.TimeZone

/** Counts observed active intervals; sleep/background gaps are never inferred as reading. */
class ReadingActivityTracker(private val root: String) {
    var foreground = false
    var enabled = false
    var blocked = false
    private var book: String? = null
    private var session = UUID.randomUUID().toString()
    private val clock = ObservedActivityClock(SystemClock.elapsedRealtime())
    private val saved = LongArray(2)
    private val queue = Channel<String>(Channel.UNLIMITED)
    private val worker = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    init { worker.launch { for (request in queue) {
        val response = RustBridge.readingActivity(root, request)
        if (response == null || !JSONObject(response).optBoolean("ok")) android.util.Log.e("ReadingActivity", "Activity save failed")
    }; worker.cancel() } }
    fun touch() { clock.touch(SystemClock.elapsedRealtime()) }
    fun sample(id: String?, audible: Boolean) {
        val now = SystemClock.elapsedRealtime()
        if (id != book) { flush(); book=id; session=UUID.randomUUID().toString(); clock.reset(now); saved.fill(0) }
        val changed=clock.sample(now,id!=null && foreground && enabled && !blocked,id!=null && audible)
        if (changed || clock.totals.indices.any { clock.totals[it]/1000-saved[it]>=15 }) flush()
    }
    fun pause() {clock.stop(SystemClock.elapsedRealtime());flush()}

    fun flush() {
        val id=book ?: return
        val now=System.currentTimeMillis()
        for (i in clock.totals.indices) {
            val seconds=clock.totals[i]/1000
            if (seconds<=saved[i]) continue
            val request=JSONObject().put("op","activity_checkpoint").put("id",id).put("session",session+i)
                .put("elapsed",seconds).put("timestamp",now/1000).put("offset",TimeZone.getDefault().getOffset(now)/1000)
                .put("listening",i==1).toString()
            if (queue.trySend(request).isSuccess) saved[i]=seconds
        }
    }
    fun close() { pause(); queue.close() }
}
