package com.zhongbai233.epub.reader.ui.reader

import android.os.Handler
import android.os.HandlerThread
import android.os.SystemClock
import android.view.Choreographer
import android.view.FrameMetrics
import android.view.MotionEvent
import androidx.compose.ui.geometry.Rect
import androidx.lifecycle.lifecycleScope
import com.zhongbai233.epub.reader.BuildConfig
import com.zhongbai233.epub.reader.MainActivity
import com.zhongbai233.epub.reader.ui.library.LibraryRenderObserver
import com.zhongbai233.epub.reader.viewmodel.ReaderViewModel
import java.io.File
import kotlinx.coroutines.*
import org.json.JSONArray
import org.json.JSONObject
import kotlin.coroutines.resume

/** Observes the actual MainActivity, production view model and library grid. */
object ShelfPerformanceHooks {
    private fun memory():JSONObject {
        val info=android.os.Debug.MemoryInfo();android.os.Debug.getMemoryInfo(info)
        val status=File("/proc/self/status").readLines()
        fun value(key:String)=status.firstOrNull {it.startsWith("$key:")}?.split(Regex("\\s+"))?.getOrNull(1)?.toLongOrNull()?.times(1024)
        return JSONObject().put("rss_bytes",value("VmRSS") ?: JSONObject.NULL).put("peak_rss_bytes",value("VmHWM") ?: JSONObject.NULL).put("pss_bytes",info.totalPss.toLong()*1024)
    }
    private suspend fun frame()=suspendCancellableCoroutine<Unit> {c ->Choreographer.getInstance().postFrameCallback {if(c.isActive)c.resume(Unit)}}
    @JvmStatic fun attach(activity:MainActivity,vm:ReaderViewModel) {
        check(BuildConfig.READER_PERFORMANCE_CHECKS && !BuildConfig.DEBUG && activity.packageName=="local.reader.performance")
        val start=activity.intent.getLongExtra("shelf_started_ns",0);check(start>0)
        val expected=activity.intent.getIntExtra("shelf_expected",0);check(expected in 1..10000)
        val coverRects=mutableMapOf<String,Rect>();val requests=mutableSetOf<String>()
        vm.libraryRenderObserver=object:LibraryRenderObserver {
            override fun coverBounds(uri:String,bounds:Rect) {if(bounds.width>0 && bounds.height>0)coverRects[uri]=bounds else coverRects.remove(uri)}
            override fun coverRequested(uri:String) {requests+=uri}
        }
        val thread=HandlerThread("ShelfFrameMetrics").apply{start()}
        val frames=java.util.Collections.synchronizedList(mutableListOf<Double>())
        val listener=android.view.Window.OnFrameMetricsAvailableListener {_,metrics,_->frames.add(metrics.getMetric(FrameMetrics.TOTAL_DURATION)/1_000_000.0)}
        activity.window.addOnFrameMetricsAvailableListener(listener,Handler(thread.looper))
        val report=JSONObject().put("scenario","dense-shelf").put("harness_schema",1).put("profile","release")
            .put("run_id",activity.intent.getStringExtra("performance_run")).put("book_count",expected)
            .put("fixture_manifest_sha256",activity.intent.getStringExtra("shelf_manifest_sha256"))
            .put("baseline_memory",memory()).put("process_id",android.os.Process.myPid())
        activity.lifecycleScope.launch {
            try {
                withTimeout(30000) {while(vm.books.size!=expected || coverRects.isEmpty())delay(10)}
                frame();frame()
                report.put("metadata_first_layout_ms",(SystemClock.elapsedRealtimeNanos()-start)/1_000_000.0)
                    .put("first_metadata_memory",memory()).put("initial_cover_requests",requests.size)
                check(expected<=96 || requests.size<expected.coerceAtMost(1000)) {"Shelf eagerly requested every cover"}
                withTimeout(30000) {while(vm.coverCache.values.count{it!=null}<minOf(expected,4))delay(10)}
                frame();frame()
                report.put("first_covers_ready_ms",(SystemClock.elapsedRealtimeNanos()-start)/1_000_000.0)
                val view=activity.window.decorView
                val visible=coverRects.values.filter{it.bottom>view.height*.15f && it.top<view.height*.85f && it.right>0 && it.left<view.width}
                check(visible.isNotEmpty())
                val x=visible.first().center.x.coerceIn(1f,view.width-1f)
                val top=maxOf(view.height*.2f,visible.minOf{it.top}).coerceAtMost(view.height*.55f)
                val bottom=minOf(view.height*.8f,visible.maxOf{it.bottom}).coerceAtLeast(top+view.height*.2f)
                val curve=JSONArray();frames.clear()
                repeat(30) {swipe ->
                    val down=SystemClock.uptimeMillis()
                    fun touch(action:Int,y:Float) {MotionEvent.obtain(down,SystemClock.uptimeMillis(),action,x,y,0).let{view.dispatchTouchEvent(it);it.recycle()}}
                    touch(MotionEvent.ACTION_DOWN,bottom)
                    repeat(24) {step ->delay(16);touch(MotionEvent.ACTION_MOVE,bottom+(top-bottom)*(step+1)/24f)}
                    touch(MotionEvent.ACTION_UP,top);delay(300);frame();frame()
                    check(vm.currentBook==null) {"Scroll gesture accidentally opened a book"}
                    check(vm.coverCache.size<=96)
                    if((swipe+1)%5==0)curve.put(JSONObject().put("swipes",swipe+1).put("memory",memory()).put("cover_cache_count",vm.coverCache.size).put("requested",requests.size))
                }
                delay(750);val steady=JSONArray();repeat(5){steady.put(memory());delay(150)}
                val timings=synchronized(frames){frames.toList().sorted()};check(timings.size>=100)
                check(vm.books.size==expected && (expected<=96 || requests.size<expected) && requests.size>=minOf(expected,12))
                report.put("swipes",30).put("scroll_memory_samples",curve).put("steady_memory_samples",steady)
                    .put("cover_cache_count",vm.coverCache.size).put("cover_cache_encoded_bytes",vm.coverCache.values.sumOf{it?.size?.toLong()?:0L})
                    .put("total_cover_requests",requests.size).put("frame_samples",timings.size)
                    .put("frame_duration_p50_ms",timings[timings.size/2]).put("frame_duration_p95_ms",timings[timings.size*95/100]).put("frame_duration_max_ms",timings.last())
                    .put("viewport",JSONObject().put("width_pixels",view.width).put("height_pixels",view.height).put("density_dpi",activity.resources.displayMetrics.densityDpi).put("font_scale",activity.resources.configuration.fontScale))
                    .put("passed",true)
            } catch(error:Throwable) {report.put("passed",false).put("error",error.stackTraceToString())}
            finally {
                vm.libraryRenderObserver=null;activity.window.removeOnFrameMetricsAvailableListener(listener);thread.quitSafely()
                File(checkNotNull(activity.getExternalFilesDir(null)),"shelf-performance.json").writeText(report.toString(2))
            }
        }
    }
}
