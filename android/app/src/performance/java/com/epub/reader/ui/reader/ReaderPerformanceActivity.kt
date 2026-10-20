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
    private var currentBlock by mutableIntStateOf(0)
    private var currentCharOffset by mutableIntStateOf(0)
    private var mode by mutableStateOf("Realistic")
    private var revision by mutableIntStateOf(0)
    private var ready=false
    private val controller=PageTurnController()
    private val startAtLastPage=booleanArrayOf(false)
    private val report=JSONObject()
    private val layouts=JSONArray()
    private lateinit var path:File
    private fun memory():JSONObject {
        val info=android.os.Debug.MemoryInfo();android.os.Debug.getMemoryInfo(info)
        val status=File("/proc/self/status").readLines()
        fun value(key:String)=status.firstOrNull {it.startsWith("$key:")}?.split(Regex("\\s+"))?.getOrNull(1)?.toLongOrNull()?.times(1024)
        return JSONObject().put("rss_bytes",value("VmRSS") ?: JSONObject.NULL).put("peak_rss_bytes",value("VmHWM") ?: JSONObject.NULL).put("pss_bytes",info.totalPss.toLong()*1024)
    }
    private suspend fun styledWindow(index:Int, neighbours:Boolean):NativeChapterList = withContext(Dispatchers.IO) {
        val source=baseBook!!
        val indices=if(neighbours) (maxOf(0,index-2)..minOf(source.chapters.lastIndex,index+2)).toList() else listOf(index)
        val styles=indices.associateWith { at ->
            source.chapters[at]
            decoder.decodeFromString<NativePresentation>(checkNotNull(RustBridge.nativePresentation(path.path,at,"mixed",true,"[]"))).also {check(it.errors.isEmpty())}
        }
        NativeChapterList(source.chapters,styles)
    }
    private suspend fun prepare(index:Int) {
        val source=baseBook!!
        val styled=styledWindow(index, intent.getBooleanExtra("performance_endurance",false))
        ready=false;startAtLastPage[0]=false;currentBlock=0;currentCharOffset=0;current=index;revision++;publication=source.copy(chapters=styled)
        withTimeout(30_000) {while(!ready)delay(10)}
        frame();frame()
    }
    private suspend fun frame() = suspendCancellableCoroutine<Unit> { continuation -> android.view.Choreographer.getInstance().postFrameCallback {if(continuation.isActive)continuation.resume(Unit)} }
    override fun onCreate(savedInstanceState:Bundle?) {
        super.onCreate(savedInstanceState)
        check(!com.zhongbai233.epub.reader.BuildConfig.DEBUG) {"Release build required"}
        com.zhongbai233.epub.reader.i18n.I18n.initialize(this)
        path=File(checkNotNull(getExternalFilesDir(null)),"large-publication.epub")
        val thread=HandlerThread("ReaderFrameMetrics").apply {start()}
        window.addOnFrameMetricsAvailableListener({_,metrics,_->if(measuring)frames.add(metrics.getMetric(FrameMetrics.TOTAL_DURATION)/1_000_000.0)},Handler(thread.looper))
        setContent {MaterialTheme {
            val book=publication
            val renderRevision=revision
            val renderChapter=current
            if(book==null)Text("Preparing local reading benchmark") else key(mode,revision) {
                if(mode=="Scroll") ScrollModeContent(chapter=book.chapters[current],allChapters=book.chapters,initialChapter=current,fontSize=18f,textColor=Color.Black,linkColor=Color.Blue,bgColor=Color(0xfff5f0e8),fontFamily=FontFamily.Default,onLinkClick={},onTextTapped={},onContentPrepared={if(revision==renderRevision && current==renderChapter)ready=true}) else PageModeContent(chapter=book.chapters[current],currentChapter=current,totalChapters=book.chapters.size,allChapters=book.chapters,fontSize=18f,textColor=Color.Black,linkColor=Color.Blue,bgColor=Color(0xfff5f0e8),fontFamily=FontFamily.Default,pageAnimation=mode,controlsVisible=false,settingsVisible=false,initialBlock=currentBlock,initialCharOffset=currentCharOffset,onPositionChange={block,offset->if(current==renderChapter){currentBlock=block;currentCharOffset=offset}},startAtLastPageRef=startAtLastPage,onPrevChapter={if(current>0){ready=false;startAtLastPage[0]=true;currentBlock=0;currentCharOffset=0;current--}},onNextChapter={if(current+1<book.chapters.size){ready=false;startAtLastPage[0]=false;currentBlock=0;currentCharOffset=0;current++}},onToggleControls={},onLinkClick={},onTextTapped={},onContentPrepared={if(revision==renderRevision && current==renderChapter)ready=true},turnController=controller)
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
                val endurance=intent.getBooleanExtra("performance_endurance",false)
                report.put("scenario",if(endurance) "visible-endurance" else "reader")
                for(animation in listOf("Realistic","Slide")) {
                    mode=animation
                    for(index in (if(endurance) listOf(0) else listOf(minOf(3,loaded.chapters.lastIndex),loaded.chapters.size/2,loaded.chapters.lastIndex).distinct().sorted())) {
                        val begin=System.nanoTime();prepare(index)
                        val first=(System.nanoTime()-begin)/1_000_000.0
                        frames.clear();val turns=mutableListOf<Double>();measuring=true
                        val turnCount=if(endurance)128 else 8;val origin=controller.page();val turnMemory=JSONArray()
                        val originChapter=current;var preparedChapter=current;val preparation=JSONArray();val trace=JSONArray()
                        val caseReport=JSONObject().put("mode",animation).put("chapter",index).put("turn_progress",trace)
                        layouts.put(caseReport)
                        repeat(turnCount) {turn ->
                            if(endurance && current!=preparedChapter) {
                                val beginPrepare=System.nanoTime()
                                val styled=styledWindow(current,true)
                                publication=loaded.copy(chapters=styled);preparedChapter=current;frame();frame()
                                preparation.put((System.nanoTime()-beginPrepare)/1_000_000.0)
                            }
                            val previous=current to controller.page();val beforeState=controller.snapshot();val tick=System.nanoTime()
                            withTimeout(5_000) {controller.turn!!.invoke(turn<turnCount/2)}
                            frame();frame();withTimeout(30_000){while(!ready)delay(5)}
                            trace.put(JSONObject().put("turn",turn+1).put("forward",turn<turnCount/2).put("from_chapter",previous.first).put("from_slot",previous.second).put("to_chapter",current).put("to_slot",controller.page()).put("before_state",beforeState).put("after_state",controller.snapshot()))
                            check((current to controller.page())!=previous) {"Turn did not commit in $animation at turn ${turn+1}: $previous -> ${current to controller.page()}"}
                            turns+=(System.nanoTime()-tick)/1_000_000.0
                            if(endurance && (turn+1)%16==0)turnMemory.put(JSONObject().put("turns",turn+1).put("memory",memory()))
                        }
                        check(!endurance || (current==originChapter && controller.page()==origin)) {"Endurance turns did not return to origin"}
                        measuring=false;delay(100)
                        val sample=synchronized(frames){frames.toList().sorted()}
                        check(sample.isNotEmpty())
                        caseReport.put("first_layout_draw_ms",first).put("turn_total_ms",JSONArray(turns)).put("turn_memory_samples",turnMemory).put("returned_to_origin",current==originChapter && controller.page()==origin).put("neighbour_preparation_ms",preparation).put("styled_resident",(publication!!.chapters as NativeChapterList).residentCount).put("frame_count",sample.size).put("frame_duration_p50_ms",sample[sample.size/2]).put("frame_duration_p95_ms",sample[(sample.size*95/100).coerceAtMost(sample.lastIndex)]).put("frame_duration_max_ms",sample.last()).put("memory",memory())
                    }
                }
                report.put("before_walk_memory",memory())
                val walkStart=minOf(3,loaded.chapters.lastIndex);val walkEnd=minOf(walkStart+200,loaded.chapters.size)
                val walkSamples=JSONArray()
                for(chunk in (walkStart until walkEnd).chunked(25)) {
                    withContext(Dispatchers.IO) {for(index in chunk){loaded.chapters[index];RustBridge.nativePresentation(path.path,index,"mixed",true,"[]")}}
                    walkSamples.put(JSONObject().put("chapters",chunk.last()-walkStart+1).put("memory",memory()))
                }
                report.put("walk_chapters",walkEnd-walkStart).put("walk_memory_samples",walkSamples)
                report.put("after_walk_memory",memory()).put("cache_stats",JSONObject(checkNotNull(RustBridge.chapterCacheStats(path.path)))).put("chapter_count",loaded.chapters.size).put("passed",true)
                val scrollCases=JSONArray();if(!endurance)mode="Scroll"
                for(index in (if(endurance) emptyList() else listOf(minOf(3,loaded.chapters.lastIndex),loaded.chapters.size/2,loaded.chapters.lastIndex).distinct().sorted())) {
                    val before=JSONObject(checkNotNull(RustBridge.chapterCacheStats(path.path))).getInt("body_loads")
                    val begin=System.nanoTime();prepare(index);delay(150)
                    val stats=JSONObject(checkNotNull(RustBridge.chapterCacheStats(path.path)))
                    val extra=stats.getInt("body_loads")-before
                    check(extra<=8) {"Scroll keys eagerly loaded $extra chapter bodies"}
                    scrollCases.put(JSONObject().put("chapter",index).put("ready_ms",(System.nanoTime()-begin)/1_000_000.0).put("extra_body_loads",extra).put("resident",stats.getInt("resident")))
                }
                report.put("scroll_cases",scrollCases)
                delay(750);val steady=JSONArray();repeat(5) {steady.put(memory());delay(150)}
                report.put("steady_memory_samples",steady)
            } catch(error:Throwable) {report.put("passed",false).put("error",error.stackTraceToString())}
            report.put("reset_trace",JSONArray(controller.resets.toList()))
            report.put("harness_schema",3).put("run_id",intent.getStringExtra("performance_run")).put("book_sha256",RustBridge.fileHash(path.path)).put("viewport",JSONObject().put("width_pixels",resources.displayMetrics.widthPixels).put("height_pixels",resources.displayMetrics.heightPixels).put("density_dpi",resources.displayMetrics.densityDpi).put("font_scale",resources.configuration.fontScale).put("refresh_rate_hz",window.decorView.display.refreshRate)).put("process_id",android.os.Process.myPid()).put("profile","release").put("layouts",layouts).put("environment","Android emulator; hardware-accelerated production PageModeContent")
            File(checkNotNull(getExternalFilesDir(null)),"release-performance.json").writeText(report.toString(2))
            thread.quitSafely()
        }
    }
}
