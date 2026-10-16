package com.zhongbai233.epub.reader.ui.reader

import android.app.Activity
import android.app.Instrumentation
import android.os.Bundle
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.RustBridge
import com.zhongbai233.epub.reader.model.*
import kotlinx.serialization.json.Json
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/** Caller supplies a local real EPUB; no fixture or publication is uploaded. */
internal object LargePublicationRuntimeChecks {
    fun run(instrumentation: Instrumentation) {
        val report=JSONObject(); val cases=JSONArray()
        try {
            val path=File(instrumentation.targetContext.cacheDir,"large-publication.epub")
            check(path.isFile) { "Supply cache/large-publication.epub before running" }
            val decoder=Json {ignoreUnknownKeys=true}
            val start=System.nanoTime()
            val metadataJson=checkNotNull(RustBridge.openBook(path.path))
            val metadata=decoder.decodeFromString<BookMetadataDto>(metadataJson)
            report.put("open_ms",(System.nanoTime()-start)/1_000_000).put("metadata_bytes",metadataJson.toByteArray().size)
            check(metadata.chapterCount>3000 && metadata.chapterTitles.size==metadata.chapterCount)
            var loads=0
            val source=LazyChapterList(metadata.chapterCount) {index ->
                loads++
                val dto=decoder.decodeFromString<ChapterDto>(checkNotNull(RustBridge.getChapter(path.path,index)))
                Chapter(dto.title,dto.blocks,dto.sourceHref,dto.images)
            }
            val publication=EpubBook(metadata.title,source,emptyList(),chapterTitles=metadata.chapterTitles)
            check(publication==publication.copy() && publication.hashCode()==publication.copy().hashCode())
            check(loads==0) { "Publication comparison eagerly read bodies" }
            for(index in listOf(1,metadata.chapterCount/2,metadata.chapterCount-1)) {
                val chapter=source[index];check(chapter.blocks.isNotEmpty())
                for(policy in listOf("none","mixed","publisher")) {
                    val styles=decoder.decodeFromString<NativePresentation>(checkNotNull(RustBridge.nativePresentation(path.path,index,policy,false,"[]")))
                    check(styles.errors.isEmpty())
                    val styled=NativeChapterList(source,mapOf(index to styles))[index]
                    for(width in listOf(320,640)) {
                        val pages=paginateContent(styled,18f,650.dp,width.dp,Density(1f),shouldRenderChapterTitle(styled))
                        check(pages.isNotEmpty())
                        fun text(block:ContentBlock)=when(block) {is ContentBlock.Paragraph -> block.spans.joinToString(""){it.text};is ContentBlock.Heading -> block.spans.joinToString(""){it.text};else->""}
                        for((blockIndex,block) in styled.blocks.withIndex()) {
                            if(block.nativeStyle().hidden) continue
                            val reconstructed=pages.flatten().filter {it.sourceBlockIndex==blockIndex && it.block !is ContentBlock.Image}.joinToString(""){text(it.block)}
                            check(reconstructed==text(block)) { "Chapter $index/$policy lost block $blockIndex" }
                        }
                        cases.put(JSONObject().put("chapter",index).put("policy",policy).put("width",width).put("pages",pages.size).put("source_preserved",true))
                    }
                }
            }
            for(index in 20..36) source[index]
            check(source.residentCount==8)
            if (instrumentation.targetContext.packageName == "local.reader.chapters") {
                val middle=metadata.chapterCount/2
                checkNotNull(RustBridge.addOrUpdateBook(instrumentation.targetContext.filesDir.path,metadata.title,path.path,middle,metadata.chapterTitles[middle]))
                report.put("preview_book_registered",true)
            }
            report.put("chapter_count",metadata.chapterCount).put("resident_bodies",source.residentCount).put("catalogue_does_not_load_bodies",true).put("passed",true)
        } catch(error:Throwable) {report.put("passed",false).put("error",error.toString())}
        report.put("layouts",cases)
        File(instrumentation.targetContext.cacheDir,"large-publication-verification.json").writeText(report.toString(2))
        instrumentation.finish(if(report.optBoolean("passed")) Activity.RESULT_OK else Activity.RESULT_CANCELED,Bundle().apply {putString("stream",report.toString(2))})
    }
}
