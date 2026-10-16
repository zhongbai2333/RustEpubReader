package com.zhongbai233.epub.reader.ui.reader

import android.os.Bundle
import android.view.FrameMetrics
import android.os.Handler
import android.os.HandlerThread
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.lifecycle.lifecycleScope
import com.zhongbai233.epub.reader.RustBridge
import com.zhongbai233.epub.reader.model.*
import kotlinx.coroutines.*
import kotlinx.serialization.json.Json
import org.json.JSONObject
import org.json.JSONArray
import java.io.File
import kotlin.coroutines.resume

/** Compiled only with -PREADER_PERFORMANCE_CHECKS. Uses production pagination/painting/curl. */
class ReaderPerformanceActivity: ComponentActivity() {
    private val decoder=Json {ignoreUnknownKeys=true}
    private val frames=java.util.Collections.synchronizedList(mutableListOf<Double>())
    @Volatile private var measuring=false
    private var baseBook:EpubBook?=null
    private var publication by mutableStateOf<EpubBook?>(null)
    private var current by mutableIntStateOf(0)
    private var mode by mutableStateOf("Realistic")
    private var revision by mutableIntStateOf(0)
    private var ready=false
    private val controller=PageTurnController()
    private val report=JSONObject()
    private val layouts=JSONArray()
    private lateinit var path:File
    private fun memory():JSONObject {
        val info=android.os.Debug.MemoryInfo();android.os.Debug.getMemoryInfo(info)
        val status=File("/proc/self/status").readLines()
        fun value(key:String)=status.firstOrNull {it.startsWith("$key:")}?.split(Regex("\\s+"))?.getOrNull(1)?.toLongOrNull()?.times(1024)?:0L
        return JSONObject().put("rss_bytes",value("VmRSS")).put("peak_rss_bytes",value("VmHWM")).put("pss_bytes",info.totalPss.toLong()*1024)
    }
    private suspend fun prepare(index:Int) {
        val source=baseBook!!
        val styled=withContext(Dispatchers.IO) {
            val view=decoder.decodeFromString<NativePresentation>(checkNotNull(RustBridge.nativePresentation(path.path,index,"mixed",true,"[]")))
            check(view.errors.isEmpty());NativeChapterList(source.chapters,mapOf(index to view))
        }
        ready=false;current=index;revision++;publication=source.copy(chapters=styled)
        withTimeout(30_000) {while(!ready)delay(10)}
        frame();frame()
    }
    private suspend fun frame() = suspendCancellableCoroutine<Unit> { continuation -> android.view.Choreographer.getInstance().postFrameCallback {if(continuation.isActive)continuation.resume(Unit)} }
    override fun onCreate(savedInstanceState:Bundle?) {
        super.onCreate(savedInstanceState)
        com.zhongbai233.epub.reader.i18n.I18n.initialize(this)
        path=File(checkNotNull(getExternalFilesDir(null)),"large-publication.epub")
        val thread=HandlerThread("ReaderFrameMetrics").apply {start()}
        window.addOnFrameMetricsAvailableListener({_,metrics,_->if(measuring)frames.add(metrics.getMetric(FrameMetrics.TOTAL_DURATION)/1_000_000.0)},Handler(thread.looper))
        setContent {MaterialTheme {
            val book=publication
            val renderRevision=revision
            if(book==null)Text("Preparing local reading benchmark") else key(current,mode,revision) {
                if(mode=="Scroll") ScrollModeContent(chapter=book.chapters[current],allChapters=book.chapters,initialChapter=current,fontSize=18f,textColor=Color.Black,linkColor=Color.Blue,bgColor=Color(0xfff5f0e8),fontFamily=FontFamily.Default,onLinkClick={},onTextTapped={},onContentPrepared={if(revision==renderRevision)ready=true}) else PageModeContent(chapter=book.chapters[current],currentChapter=current,totalChapters=book.chapters.size,allChapters=book.chapters,fontSize=18f,textColor=Color.Black,linkColor=Color.Blue,bgColor=Color(0xfff5f0e8),fontFamily=FontFamily.Default,pageAnimation=mode,controlsVisible=false,settingsVisible=false,onPrevChapter={if(current>0){ready=false;current--;revision++}},onNextChapter={if(current+1<book.chapters.size){ready=false;current++;revision++}},onToggleControls={},onLinkClick={},onTextTapped={},onContentPrepared={if(revision==renderRevision)ready=true},turnController=controller)
            }
        }}
        lifecycleScope.launch {
            try {
                report.put("baseline_memory",memory());val start=System.nanoTime()
                val loaded=withContext(Dispatchers.IO) {
                    val json=checkNotNull(RustBridge.openBook(path.path));val meta=decoder.decodeFromString<BookMetadataDto>(json)
                    val source=LazyChapterList(meta.chapterCount) {index ->val dto=decoder.decodeFromString<ChapterDto>(checkNotNull(RustBridge.getChapter(path.path,index)));Chapter(dto.title,dto.blocks,dto.sourceHref,dto.images)}
                    source[0];EpubBook(meta.title,source,meta.toc.map {it.title to it.chapterIndex},chapterTitles=meta.chapterTitles)
                }
                report.put("core_and_first_body_ms",(System.nanoTime()-start)/1_000_000.0)
                baseBook=loaded;publication=loaded
                prepare(0)
                report.put("first_content_draw_ms",(System.nanoTime()-start)/1_000_000.0).put("first_content_memory",memory())
                for(animation in listOf("Realistic","Slide")) {
                    mode=animation
                    for(index in listOf(3,loaded.chapters.size/2,loaded.chapters.size-1)) {
                        val begin=System.nanoTime();prepare(index)
                        val first=(System.nanoTime()-begin)/1_000_000.0
                        frames.clear();val turns=mutableListOf<Double>();measuring=true
                        repeat(8) {val tick=System.nanoTime();controller.turn!!.invoke(it<4);frame();frame();turns+=(System.nanoTime()-tick)/1_000_000.0}
                        measuring=false;delay(100)
                        val sample=synchronized(frames){frames.toList().sorted()}
                        check(sample.isNotEmpty())
                        layouts.put(JSONObject().put("mode",animation).put("chapter",index).put("first_layout_draw_ms",first).put("turn_total_ms",JSONArray(turns)).put("frame_count",sample.size).put("frame_duration_p50_ms",sample[sample.size/2]).put("frame_duration_p95_ms",sample[(sample.size*95/100).coerceAtMost(sample.lastIndex)]).put("frame_duration_max_ms",sample.last()).put("memory",memory()))
                    }
                }
                report.put("before_walk_memory",memory())
                withContext(Dispatchers.IO) {for(index in 3..202){loaded.chapters[index];RustBridge.nativePresentation(path.path,index,"mixed",true,"[]")}}
                report.put("after_200_chapters_memory",memory()).put("cache_stats",JSONObject(checkNotNull(RustBridge.chapterCacheStats(path.path)))).put("chapter_count",loaded.chapters.size).put("passed",true)
                val scrollCases=JSONArray();mode="Scroll"
                for(index in listOf(3,loaded.chapters.size/2,loaded.chapters.size-1)) {
                    val before=JSONObject(checkNotNull(RustBridge.chapterCacheStats(path.path))).getInt("body_loads")
                    val begin=System.nanoTime();prepare(index);delay(150)
                    val stats=JSONObject(checkNotNull(RustBridge.chapterCacheStats(path.path)))
                    val extra=stats.getInt("body_loads")-before
                    check(extra<=8) {"Scroll keys eagerly loaded $extra chapter bodies"}
                    scrollCases.put(JSONObject().put("chapter",index).put("ready_ms",(System.nanoTime()-begin)/1_000_000.0).put("extra_body_loads",extra).put("resident",stats.getInt("resident")))
                }
                report.put("scroll_cases",scrollCases)
            } catch(error:Throwable) {report.put("passed",false).put("error",error.stackTraceToString())}
            report.put("process_id",android.os.Process.myPid()).put("profile","release").put("layouts",layouts).put("environment","Android emulator; hardware-accelerated production PageModeContent")
            File(checkNotNull(getExternalFilesDir(null)),"release-performance.json").writeText(report.toString(2))
            thread.quitSafely()
        }
    }
}
