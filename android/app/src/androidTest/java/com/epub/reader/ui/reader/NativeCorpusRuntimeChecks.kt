package com.zhongbai233.epub.reader.ui.reader

import android.app.Activity
import android.app.Instrumentation
import android.content.Intent
import android.os.Bundle
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.createFontFamilyResolver
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.RustBridge
import com.zhongbai233.epub.reader.model.*
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import org.json.JSONArray
import org.json.JSONObject

/** Uses the production style adapter, paginator, image cache and Compose block renderer. */
internal object NativeCorpusRuntimeChecks {
    fun run(test: Instrumentation) {
        val context=test.targetContext
        check(context.packageName.startsWith("local.reader.")) {"Corpus checks require an isolated package"}
        val output=JSONObject().put("passed",false)
        val results=JSONArray()
        var activity:CurlImageCheckActivity?=null
        try {
            val directory=File(context.cacheDir,"corpus")
            val manifest=JSONArray(File(directory,"cases.json").readText())
            check(manifest.length()>0) {"Empty corpus manifest"}
            val decoder=Json {ignoreUnknownKeys=true}
            val density=Density(context.resources.displayMetrics.density,context.resources.configuration.fontScale)
            val measurer=TextMeasurer(createFontFamilyResolver(context),density,LayoutDirection.Ltr,cacheSize=16)
            val scalarSource="😀😀😀😀😀😀中文结尾。"
            val scalarChapter=Chapter("Scalar fragments",listOf(ContentBlock.Paragraph(listOf(TextSpan(scalarSource)))))
            for(width in listOf(1,10,320)) {
                val fragments=paginateContent(scalarChapter,18f,1.dp,width.dp,density,false,textMeasurer=measurer).flatten()
                val pieces=fragments.map {text(it.block)?:error("Non-text scalar probe fragment")}
                check(pieces.joinToString("")==scalarSource && pieces.all {it.isNotEmpty() && Charsets.UTF_8.newEncoder().canEncode(it)}) {"Native minimum fragment divided a Unicode scalar"}
            }
            output.put("minimum_scalar_fragment_integrity",true)

            val host=test.startActivitySync(Intent(context,CurlImageCheckActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK).putExtra("layoutBounds",true)) as CurlImageCheckActivity
            activity=host
            test.waitForIdleSync()
            val insets=checkNotNull(androidx.core.view.ViewCompat.getRootWindowInsets(host.window.decorView))
                .getInsets(androidx.core.view.WindowInsetsCompat.Type.systemBars())
            val availableHeight=kotlin.math.floor(minOf(650f,(host.window.decorView.height-insets.top-insets.bottom)/density.density-32f)).toInt()
            check(availableHeight>=200) {"Corpus test viewport is too short"}
            output.put("available_height_dp",availableHeight).put("density",density.density).put("font_scale",density.fontScale)
            for(caseIndex in 0 until manifest.length()) {
                val case=manifest.getJSONObject(caseIndex)
                val name=case.getString("name")
                check(name.matches(Regex("[A-Za-z0-9_-]+")))
                val chapterIndex=case.getInt("chapter")
                val baseFont=case.optDouble("font_size",18.0).toFloat();check(baseFont.isFinite() && baseFont in 8f..96f)
                val path=File(directory,"$name.epub");check(path.isFile)
                val dto=decoder.decodeFromString<ChapterDto>(checkNotNull(RustBridge.getChapter(path.path,chapterIndex)))
                check(dto.blocks.isNotEmpty())
                for(policy in listOf("none","mixed","publisher")) {
                    val presentation=decoder.decodeFromString<NativePresentation>(checkNotNull(RustBridge.nativePresentation(path.path,chapterIndex,policy,false,"[]")))
                    check(presentation.errors.isEmpty()) {presentation.errors.toString()}
                    check(presentation.styles.size==dto.blocks.size)
                    val source=Chapter(dto.title,dto.blocks,dto.sourceHref,dto.images)
                    val chapter=NativeChapterList(listOf(source),mapOf(0 to presentation))[0]
                    val images=chapter.blocks.filterIsInstance<ContentBlock.Image>()+chapter.images.map {it.content()}
                    check(images.size>=case.optInt("min_images")) {"Missing images: $name/$chapterIndex"}
                    val tables=presentation.styles.count {it.table!=null}
                    check(tables>=case.optInt("min_tables")) {"Missing tables: $name/$chapterIndex"}
                    val notes=presentation.styles.flatMap {it.inline}.mapNotNull {it.note}.distinct()
                    check(notes.size>=case.optInt("min_notes")) {"Missing notes: $name/$chapterIndex"}
                    runBlocking {for(image in images)checkNotNull(ReaderImageCache.load(image.data)) {"Undecodable corpus illustration"}}
                    for(note in notes) {
                        val resolved=decoder.decodeFromString<NativeNote>(checkNotNull(RustBridge.nativeNote(path.path,chapterIndex,note)))
                        check(resolved.text.isNotBlank()) {"Empty native note: $name/${note}"}
                    }
                    for(width in listOf(320,640)) {
                        val pages=paginateContent(chapter,baseFont,availableHeight.dp,width.dp,density,false,textMeasurer=measurer)
                        check(pages.isNotEmpty())
                        val flat=pages.flatten()
                        for((blockIndex,block)in chapter.blocks.withIndex()) {
                            val original=text(block)?:continue
                            if(block.nativeStyle().hidden)continue
                            val fragments=flat.filter {it.sourceBlockIndex==blockIndex && it.block !is ContentBlock.Image}
                            check(fragments.joinToString(""){text(it.block)?:""}==original) {"Source coverage differs: $name/$policy/$width/$blockIndex"}
                        }
                        check(flat.count {it.block is ContentBlock.Image}==images.count {!it.nativeStyle.hidden}) {"Image pagination lost or duplicated a figure"}
                        val semantic=presentation.styles.indices.filter {at->val style=presentation.styles[at];style.table!=null || style.inline.any {it.note!=null}}.toSet()
                        val selected=(listOf(0,pages.size/2,pages.lastIndex)+pages.indices.filter {page->pages[page].any {item->
                            val style=item.block.nativeStyle();item.block is ContentBlock.Image || item.sourceBlockIndex in semantic || style.table!=null || style.inline.any {it.note!=null}
                        }}).distinct().sorted()
                        val screenshots=JSONArray()
                        var maximumHeight=0
                        for(page in selected) {
                            val measured=AtomicInteger(-1);val drawn=AtomicBoolean(false)
                            val blockHeights=java.util.concurrent.ConcurrentHashMap<Int,Int>()
                            val layouts=java.util.concurrent.ConcurrentHashMap<Int,BlockLayoutInfo>()
                            test.runOnMainSync {host.setContent {
                                Box(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.systemBars).padding(16.dp)) {
                                Column(Modifier.requiredWidth(width.dp).wrapContentHeight(unbounded=true)
                                    .background(Color.White).onGloballyPositioned {measured.set(it.size.height)}
                                    .drawWithContent {drawContent();drawn.set(true)}) {
                                    for((itemIndex,item) in pages[page].withIndex()) androidx.compose.foundation.layout.Box(Modifier.onGloballyPositioned {blockHeights[itemIndex]=it.size.height}) {ContentBlockView(chapterIndex=chapterIndex,blockIndex=item.sourceBlockIndex,
                                        block=item.block,sourceCharOffset=item.sourceCharOffset,imageMaxHeight=item.imageMaxHeight,
                                        fontSize=baseFont,textColor=Color.Black,linkColor=Color.Blue,fontFamily=FontFamily.Default,
                                        onLinkClick={},onTextTapped={},lineSpacing=1.5f,paraSpacing=0.5f,blockLayoutRegistry=layouts)}
                                }
                                }
                            }}
                            val deadline=System.currentTimeMillis()+5000
                            while((measured.get()<0 || !drawn.get()) && System.currentTimeMillis()<deadline) {test.waitForIdleSync();Thread.sleep(10)}
                            check(drawn.get() && measured.get()>0) {"Native page did not draw: $name/$policy/$width/$page"}
                            if(measured.get()>availableHeight*density.density+2) {
                                output.put("overflow_blocks",JSONArray(pages[page].mapIndexed {i,item->
                                    val style=item.block.nativeStyle();val font=style.font(baseFont)
                                    JSONObject().put("block",item.sourceBlockIndex).put("offset",item.sourceCharOffset)
                                        .put("painted",blockHeights[i]).put("predicted",measuredBlockHeight(item.block,font,font*style.lineScale(1.5f)*density.density*density.fontScale,width*density.density,density,style.after?:0.5f,2,1.5f,measurer,FontFamily.Default,item.sourceCharOffset))
                                        .put("source",text(item.block)).put("style",style.toString())
                                }))
                            }
                            check(measured.get()<=availableHeight*density.density+2) {"Native painted page clipped: $name/$policy/$width/$page height=${measured.get()} budget=${availableHeight*density.density}"}
                            maximumHeight=maxOf(maximumHeight,measured.get())
                            if(policy=="mixed") {
                                // Layout/draw callbacks can precede the compositor transaction.
                                // Cross two real display frames before retaining a labelled screenshot.
                                val frames=java.util.concurrent.CountDownLatch(1)
                                test.runOnMainSync {host.window.decorView.postOnAnimation {host.window.decorView.postOnAnimation {frames.countDown()}}}
                                check(frames.await(3,java.util.concurrent.TimeUnit.SECONDS)) {"No presented corpus frame"}
                                val bitmap=checkNotNull(test.uiAutomation.takeScreenshot())
                                val screenshotName="corpus-$name-$chapterIndex-$width-$page.png"
                                screenshots.put(screenshotName)
                                File(context.cacheDir,screenshotName).outputStream().use {bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG,100,it)}
                                bitmap.recycle()
                            }
                        }
                        results.put(JSONObject().put("name",name).put("chapter",chapterIndex).put("policy",policy).put("width",width)
                            .put("base_font_size",baseFont).put("available_height_dp",availableHeight).put("pages",pages.size).put("drawn_pages",JSONArray(selected)).put("screenshot_names",screenshots).put("canonical_text_coverage",true)
                            .put("images",images.size).put("tables",tables).put("notes_resolved",notes.size).put("maximum_painted_height_px",maximumHeight))
                    }
                }
            }
            output.put("passed",true)
        }catch(error:Throwable) {output.put("error",error.toString())}
        finally {activity?.let {host->test.runOnMainSync {host.finish()}}}
        output.put("layouts",results)
        File(context.cacheDir,"corpus-verification.json").writeText(output.toString(2))
        test.finish(if(output.optBoolean("passed"))Activity.RESULT_OK else Activity.RESULT_CANCELED,Bundle().apply {putString("stream",output.toString(2))})
    }
    private fun text(block:ContentBlock):String?=when(block) {
        is ContentBlock.Paragraph->block.spans.joinToString(""){it.text}
        is ContentBlock.Heading->block.spans.joinToString(""){it.text}
        else->null
    }
}
