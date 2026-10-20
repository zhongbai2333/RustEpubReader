package com.zhongbai233.epub.reader.ui.reader

import android.os.Bundle
import android.graphics.BitmapFactory
import androidx.activity.ComponentActivity
import androidx.lifecycle.lifecycleScope
import com.zhongbai233.epub.reader.BuildConfig
import com.zhongbai233.epub.reader.RustBridge
import kotlinx.coroutines.*
import org.json.JSONArray
import org.json.JSONObject
import org.json.JSONTokener
import com.zhongbai233.epub.reader.model.ChapterDto
import com.zhongbai233.epub.reader.model.ContentBlock
import kotlinx.serialization.json.Json
import java.io.File
import java.security.MessageDigest

/** Opt-in Release bridge/decoder A/B, not a reader-frame benchmark. */
class ImageBridgePerformanceActivity : ComponentActivity() {
    private fun memory():JSONObject {
        val info=android.os.Debug.MemoryInfo();android.os.Debug.getMemoryInfo(info)
        val status=File("/proc/self/status").readLines()
        fun bytes(key:String)=status.firstOrNull {it.startsWith("$key:")}?.split(Regex("\\s+"))?.getOrNull(1)?.toLongOrNull()?.times(1024)
        return JSONObject().put("rss_bytes",bytes("VmRSS") ?: JSONObject.NULL).put("peak_rss_bytes",bytes("VmHWM") ?: JSONObject.NULL).put("pss_bytes",info.totalPss.toLong()*1024)
    }
    private fun command(path:String,action:String,session:String="",chapter:Int=0):Any = JSONTokener(checkNotNull(RustBridge.imageTransport(path,JSONObject().put("action",action).put("session",session).put("chapter",chapter).toString()))).nextValue()
    private fun images(chapter:JSONObject):List<JSONObject> = buildList {
        val blocks=chapter.getJSONArray("blocks")
        for(i in 0 until blocks.length()) if(blocks.getJSONObject(i).getString("type")=="image") add(blocks.getJSONObject(i))
        val extra=chapter.getJSONArray("images");for(i in 0 until extra.length()) add(extra.getJSONObject(i))
    }
    private fun bytes(path:String,image:JSONObject,binary:Boolean):ByteArray = if(binary) checkNotNull(RustBridge.imageBytes(path,image.getString("data"))) else android.util.Base64.decode(image.getString("data"),android.util.Base64.DEFAULT)
    private fun sha(bytes:ByteArray)=MessageDigest.getInstance("SHA-256").digest(bytes).joinToString(""){"%02x".format(it)}
    private suspend fun memoryProbe(root:File,config:JSONObject):JSONObject {
        val transport=config.getString("memory_transport");check(transport in listOf("base64","resource"))
        val spec=config.getJSONArray("books").getJSONObject(0)
        val file=File(root,spec.getString("file"));check(file.parentFile==root && file.extension=="epub")
        val path=file.path;val index=spec.getInt("chapter");checkNotNull(RustBridge.openBook(path))
        val session=if(transport=="resource") (command(path,"begin") as String).also {check(command(path,"prepare",it)==true)} else null
        try {
            // Deliberate probe-only GC; no such request runs in normal reading.
            System.gc();delay(300);val baseline=memory()
            fun load():Pair<ChapterDto,Int> {
                val text=if(session!=null) checkNotNull(RustBridge.imageTransport(path,JSONObject().put("action","chapter").put("session",session).put("chapter",index).toString())) else checkNotNull(RustBridge.getChapter(path,index))
                return Json {ignoreUnknownKeys=true}.decodeFromString<ChapterDto>(text) to text.toByteArray(Charsets.UTF_8).size
            }
            val (chapter,jsonBytes)=load()
            fun count()=chapter.blocks.count {it is ContentBlock.Image}+chapter.images.size
            check(count()>1);val after=memory();val samples=JSONArray()
            repeat(5){System.gc();delay(300);check(count()>1);samples.put(memory())}
            // ChapterDto remains reachable throughout the last observation.
            val liveHeap=Runtime.getRuntime().totalMemory()-Runtime.getRuntime().freeMemory()
            val nativeHeap=android.os.Debug.getNativeHeapAllocatedSize()
            return JSONObject().put("measurement","retained-native-chapter").put("transport",transport).put("book",spec.getString("label")).put("book_sha256",RustBridge.fileHash(path)).put("chapter",index).put("image_count",count()).put("json_bytes",jsonBytes).put("baseline_memory",baseline).put("after_chapter_memory",after).put("steady_memory_samples",samples).put("java_heap_used_bytes",liveHeap).put("native_heap_allocated_bytes",nativeHeap).put("bitmap_decode_measured",false).put("probe_gc_requested",true).put("passed",true)
        }finally{if(session!=null) command(path,"close",session)}
    }
    override fun onCreate(savedInstanceState:Bundle?) {
        super.onCreate(savedInstanceState)
        check(BuildConfig.READER_PERFORMANCE_CHECKS && !BuildConfig.DEBUG && packageName=="local.reader.performance")
        val root=checkNotNull(getExternalFilesDir(null))
        lifecycleScope.launch(Dispatchers.Default) {
            val report=JSONObject().put("schema",2).put("run_id",intent.getStringExtra("performance_run")).put("profile",BuildConfig.READER_NATIVE_PROFILE).put("physical_device",false).put("process_id",android.os.Process.myPid()).put("cases",JSONArray())
            try {
                report.put("isolated_blank_host",true).put("release_network_checks",false)
                delay(2000)
                val config=JSONObject(File(root,"image-bridge-config.json").readText());val books=config.getJSONArray("books")
                if(config.has("memory_transport")) {
                    val value=memoryProbe(root,config);for(key in value.keys()) report.put(key,value.get(key))
                    File(root,"image-bridge-performance.json").writeText(report.toString(2));return@launch
                }
                for(bookIndex in 0 until books.length()) {
                    val spec=books.getJSONObject(bookIndex);val file=File(root,spec.getString("file"));check(file.parentFile==root && file.extension=="epub")
                    val path=file.path;val index=spec.getInt("chapter");val start=System.nanoTime()
                    checkNotNull(RustBridge.openBook(path));val baseline=JSONObject(checkNotNull(RustBridge.getChapter(path,index)))
                    val prepareMs=(System.nanoTime()-start)/1e6
                    val session=command(path,"begin") as String
                    try {
                        check(command(path,"prepare",session)==true)
                        val header=command(path,"chapter",session,index) as JSONObject
                        val baseImages=images(baseline);val newImages=images(header);check(baseImages.isNotEmpty() && baseImages.size==newImages.size)
                        val signatures=JSONArray()
                        for(i in baseImages.indices) {
                            val expected=bytes(path,baseImages[i],false);val actual=bytes(path,newImages[i],true);check(expected.contentEquals(actual))
                            signatures.put(sha(actual));newImages[i].put("data",baseImages[i].getString("data"))
                        }
                        // Compare the complete DTO, including text, order, anchors and alt text.
                        check(JSONObject(header.toString()).toString()==JSONObject(baseline.toString()).toString())
                        val fresh=command(path,"chapter",session,index) as JSONObject;val firstReference=images(fresh).first().getString("data")
                        check(runCatching {RustBridge.imageBytes(path+".foreign",firstReference)}.isFailure)
                        check(runCatching {RustBridge.imageBytes(path,"img1:malformed")}.isFailure)
                        val warmupAt=System.nanoTime()
                        for(binary in listOf(false,true)) {
                            val image=if(binary) images(fresh).first() else baseImages.first();val data=bytes(path,image,binary)
                            val options=BitmapFactory.Options().apply {inJustDecodeBounds=true};BitmapFactory.decodeByteArray(data,0,data.size,options);check(options.outWidth>0 && options.outHeight>0)
                            var sample=1;while(maxOf(options.outWidth/sample,options.outHeight/sample)>2048 || (options.outWidth.toLong()/sample)*(options.outHeight/sample)>4*1024*1024) sample*=2
                            checkNotNull(BitmapFactory.decodeByteArray(data,0,data.size,BitmapFactory.Options().apply {inSampleSize=sample})).recycle()
                        }
                        val decoderWarmupMS=(System.nanoTime()-warmupAt)/1e6
                        for(selected in listOf(1,baseImages.size)) for(iteration in 0 until 7) {
                            val order=if(iteration%2==0) listOf(false,true) else listOf(true,false)
                            for(binary in order) {
                                val before=memory();val begin=System.nanoTime()
                                val text=if(binary) checkNotNull(RustBridge.imageTransport(path,JSONObject().put("action","chapter").put("session",session).put("chapter",index).toString())) else checkNotNull(RustBridge.getChapter(path,index))
                                val decoded=JSONObject(text);val selectedImages=images(decoded).take(selected);val parsed=System.nanoTime()
                                var transfer=0L;var bounds=0L;var decode=0L;var totalBytes=0L;val dimensions=JSONArray()
                                for(image in selectedImages) {
                                    var at=System.nanoTime();val data=bytes(path,image,binary);transfer+=System.nanoTime()-at;totalBytes+=data.size
                                    at=System.nanoTime();val options=BitmapFactory.Options().apply {inJustDecodeBounds=true};BitmapFactory.decodeByteArray(data,0,data.size,options);bounds+=System.nanoTime()-at
                                    check(options.outWidth>0 && options.outHeight>0);dimensions.put(JSONArray().put(options.outWidth).put(options.outHeight))
                                    var sample=1;while(maxOf(options.outWidth/sample,options.outHeight/sample)>2048 || (options.outWidth.toLong()/sample)*(options.outHeight/sample)>4*1024*1024) sample*=2
                                    at=System.nanoTime();val bitmap=checkNotNull(BitmapFactory.decodeByteArray(data,0,data.size,BitmapFactory.Options().apply {inSampleSize=sample}));check(bitmap.width>0);bitmap.recycle();decode+=System.nanoTime()-at
                                }
                                val end=System.nanoTime()
                                report.getJSONArray("cases").put(JSONObject().put("book",spec.getString("label")).put("chapter",index).put("image_count",baseImages.size).put("selected",selected).put("iteration",iteration).put("transport",if(binary) "resource" else "base64").put("json_bytes",text.toByteArray(Charsets.UTF_8).size).put("image_bytes",totalBytes).put("encode_parse_ms",(parsed-begin)/1e6).put("transfer_ms",transfer/1e6).put("bounds_ms",bounds/1e6).put("decode_ms",decode/1e6).put("total_ms",(end-begin)/1e6).put("memory_before",before).put("memory_after",memory()).put("dimensions",dimensions))
                            }
                        }
                        report.put(spec.getString("label"),JSONObject().put("prepare_and_first_legacy_chapter_ms",prepareMs).put("decoder_warmup_ms",decoderWarmupMS).put("sha256",RustBridge.fileHash(path)).put("image_sha256",signatures))
                        val next=command(path,"begin") as String
                        check(! (command(path,"close",session) as Boolean));check(runCatching {RustBridge.imageBytes(path,firstReference)}.isFailure)
                        check(runCatching {command(path,"prepare",session)}.isFailure)
                        check(command(path,"prepare",next)==true);check(command(path,"close",next)==true)
                        check(runCatching {command(path,"chapter",next,index)}.isFailure)
                    } finally {command(path,"close",session)}
                }
                report.put("lifetime_checks",true).put("byte_and_metadata_parity",true).put("passed",true)
            } catch(error:Throwable){report.put("passed",false).put("error",error.stackTraceToString())}
            File(root,"image-bridge-performance.json").writeText(report.toString(2))
        }
    }
}
