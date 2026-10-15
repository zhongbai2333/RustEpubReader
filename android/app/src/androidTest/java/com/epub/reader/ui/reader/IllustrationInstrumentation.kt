package com.zhongbai233.epub.reader.ui.reader

import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.requiredWidth
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.ui.layout.onGloballyPositioned
import android.app.Activity
import android.app.Instrumentation
import android.graphics.BitmapFactory
import android.graphics.Color
import android.os.Bundle
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.RustBridge
import com.zhongbai233.epub.reader.model.*
import kotlinx.serialization.json.Json
import org.json.JSONObject

/** Verifies the actual JNI bridge and Android bitmap decoder on a device. */
class IllustrationInstrumentation : Instrumentation() {
    private var punctuation = false
    private var firstReader = false
    private var linkage = false
    private var appearance = false
    private var appearanceAmbient = false
    private var i18n = false
    private var bounds = false
    private var corpus = false
    private var typography = false
    private var native = false
    private var curl = false
    private var bookShare = false
    override fun onCreate(arguments: Bundle?) { punctuation=arguments?.getString("check")=="punctuation"; firstReader=arguments?.getString("check")=="first-reader"; linkage=arguments?.getString("check")=="appearance-linkage"; appearance=arguments?.getString("check") in listOf("appearance","appearance-ambient"); appearanceAmbient=arguments?.getString("check")=="appearance-ambient"; bookShare=arguments?.getString("check")=="book-share"; i18n=arguments?.getString("check")=="i18n"; bounds=arguments?.getString("check")=="bounds"; corpus=arguments?.getString("check")=="corpus"; typography = arguments?.getString("check") == "typography"; native = arguments?.getString("check") == "native"; curl = arguments?.getString("check") == "curl"; super.onCreate(arguments); start() }
    override fun onStart() {
        // Existing isolated UI checks exercise the already-onboarded reader.
        if(!firstReader && targetContext.packageName.startsWith("local.reader.")) {
            targetContext.getSharedPreferences("reader_settings",0).edit().putBoolean("reader.introduction.v1",true).commit()
        }
        if(punctuation) { PunctuationRuntimeChecks.run(this); return }
        if(firstReader) {ReaderIntroductionRuntimeChecks.run(this);return}
        if(linkage) {AppearanceLinkageRuntimeChecks.run(this);return}
        if(appearance) { ReadingAppearanceRuntimeChecks.run(this,appearanceAmbient); return }
        if (bookShare) { BookShareRuntimeChecks.run(this); return }
        if(i18n) {com.zhongbai233.epub.reader.i18n.I18nRuntimeChecks.run(this);return}
        if (bounds) {verifyPageBounds();return}
        if (corpus) {verifyCorpus();return}
        if (typography) { verifyTypography();return }
        if (native) { verifyNative(); return }
        if (curl) { verifyCurl(); return }
        val report = JSONObject()
        try {
            val path = java.io.File(targetContext.cacheDir, "illustration-fixture.epub")
            context.assets.open("illustrations.epub").use { input -> path.outputStream().use { input.copyTo(it) } }
            check(RustBridge.openBook(path.path) != null)
            val decoder = Json { ignoreUnknownKeys = true }
            var checked = 0
            for (chapterIndex in 0 until 8) {
                val dto = decoder.decodeFromString<ChapterDto>(checkNotNull(RustBridge.getChapter(path.path, chapterIndex)))
                val images = dto.blocks.filterIsInstance<ContentBlock.Image>() + dto.images.map { it.content() }
                check(images.size == 1) { "Chapter $chapterIndex image count ${images.size}" }
                val bytes = android.util.Base64.decode(images.single().data, android.util.Base64.DEFAULT)
                val bitmap = checkNotNull(BitmapFactory.decodeByteArray(bytes, 0, bytes.size))
                val top = bitmap.getPixel(bitmap.width / 2, bitmap.height / 6)
                val middle = bitmap.getPixel(bitmap.width / 2, bitmap.height / 2)
                val bottom = bitmap.getPixel(bitmap.width / 2, bitmap.height * 5 / 6)
                check(Color.red(top) > Color.green(top) + 40)
                check(Color.green(middle) > Color.red(middle) + 30)
                check(Color.blue(bottom) > Color.red(bottom) + 40)
                bitmap.recycle()
                val chapter = Chapter(dto.title, dto.blocks, dto.sourceHref, dto.images)
                for (width in listOf(320, 640)) {
                    val density = Density(targetContext.resources.displayMetrics.density)
                    val rendered = paginateContent(chapter, 18f, 600.dp, width.dp, density, shouldRenderChapterTitle(chapter))
                    val pagedImages = rendered.flatten().filter { it.block is ContentBlock.Image }
                    check(pagedImages.size == 1)
                    check(rendered.first().any { it.block is ContentBlock.Image }) { "Chapter heading orphaned from image: $chapterIndex" }
                    check(pagedImages.single().imageMaxHeight.value in 1f..600f)
                    val estimate = estimateImageBlockHeight(images.single().data, width * density.density, density, 18f,
                        pagedImages.single().imageMaxHeight.value * density.density)
                    check(estimate <= 600 * density.density)
                }
                checked++
            }
            report.put("passed", true).put("decoded_images", checked).put("page_widths", "320,640")
        } catch (failure: Throwable) {
            report.put("passed", false).put("error", failure.toString())
        }
        java.io.File(targetContext.cacheDir, "illustration-verification.json").writeText(report.toString(2))
        finish(if (report.optBoolean("passed")) Activity.RESULT_OK else Activity.RESULT_CANCELED,
            Bundle().apply { putString("stream", report.toString(2)) })
    }

    private fun verifyCurl() {
        val report = JSONObject()
        val results = org.json.JSONArray()
        try {
            val source = android.graphics.Bitmap.createBitmap(240, 360, android.graphics.Bitmap.Config.ARGB_8888)
            for (y in 0 until source.height) for (x in 0 until source.width) {
                source.setPixel(x, y, when { y < 120 -> Color.rgb(230, 30, 30); y < 240 -> Color.rgb(30, 220, 30); else -> Color.rgb(30, 30, 230) })
            }
            val bytes = java.io.ByteArrayOutputStream().apply { source.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, this) }.toByteArray()
            source.recycle()
            val data = android.util.Base64.encodeToString(bytes, android.util.Base64.NO_WRAP)
            kotlinx.coroutines.runBlocking(kotlinx.coroutines.Dispatchers.IO) {
                val first = checkNotNull(ReaderImageCache.load(data))
                check(first === ReaderImageCache.load(data) && first === ReaderImageCache.peek(data))
            }
            fun imagePixels(bitmap: android.graphics.Bitmap): Int {
                var count = 0
                for (y in 0 until bitmap.height step 4) for (x in 0 until bitmap.width step 4) {
                    val c = bitmap.getPixel(x, y)
                    val r = Color.red(c); val g = Color.green(c); val b = Color.blue(c)
                    if ((r > g + 60 && r > b + 60) || (g > r + 60 && g > b + 60) || (b > r + 60 && b > g + 60)) count++
                }
                return count
            }
            for (spread in listOf(false, true)) for (backward in listOf(false, true)) {
                val intent = android.content.Intent(targetContext, CurlImageCheckActivity::class.java)
                    .addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK).putExtra("image", data).putExtra("spread", spread).putExtra("backward", backward)
                val activity = startActivitySync(intent) as CurlImageCheckActivity
                for (i in 0 until 100) { if (activity.ready) break; Thread.sleep(30) }
                check(activity.ready)
                Thread.sleep(300); waitForIdleSync()
                runOnMainSync { activity.turn() }
                var frames = 0; var minimumPixels = Int.MAX_VALUE
                var peakMounts = 0
                while (!activity.finished) {
                    Thread.sleep(75)
                    if (kotlin.math.abs(activity.progress) in 0.05f..0.74f) {
                        val bitmap = checkNotNull(uiAutomation.takeScreenshot())
                        val pixels = imagePixels(bitmap)
                        minimumPixels = minOf(minimumPixels, pixels)
                        if (frames == 3) java.io.File(targetContext.cacheDir, "curl-${if(spread) "spread" else "portrait"}-${if(backward) "backward" else "forward"}.png").outputStream().use { bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
                        bitmap.recycle()
                        check(pixels > 100) { "Illustration disappeared mid-curl at ${activity.progress}" }
                        peakMounts = maxOf(peakMounts, activity.mounts.get())
                        frames++
                    }
                }
                check(frames >= 4) { "Insufficient animated frames: $frames" }
                // Idle->animated book spread adds layers once; coordinates must not remount them every frame.
                check(peakMounts <= 10) { "Content remounted every frame: $peakMounts" }
                results.put(JSONObject().put("spread", spread).put("backward", backward).put("sampled_frames", frames).put("minimum_image_pixels", minimumPixels).put("content_mounts_during_animation", peakMounts))
                runOnMainSync { activity.finish() }; waitForIdleSync()
            }
            report.put("passed", true).put("cache_reuses_bitmap", true).put("animations", results)
        } catch (error: Throwable) { report.put("passed", false).put("error", error.toString()).put("animations", results) }
        java.io.File(targetContext.cacheDir, "curl-image-verification.json").writeText(report.toString(2))
        finish(if(report.optBoolean("passed")) Activity.RESULT_OK else Activity.RESULT_CANCELED, Bundle().apply { putString("stream", report.toString(2)) })
    }

    private fun verifyPageBounds() {
        val report=JSONObject();val results=org.json.JSONArray()
        var activity:CurlImageCheckActivity?=null
        try {
            val intent=android.content.Intent(targetContext,CurlImageCheckActivity::class.java)
                .addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK).putExtra("layoutBounds",true)
            val renderActivity=startActivitySync(intent) as CurlImageCheckActivity
            activity=renderActivity
            val decoder=Json {ignoreUnknownKeys=true}
            val density=Density(targetContext.resources.displayMetrics.density,targetContext.resources.configuration.fontScale)
            val measurer=androidx.compose.ui.text.TextMeasurer(androidx.compose.ui.text.font.createFontFamilyResolver(targetContext),density,androidx.compose.ui.unit.LayoutDirection.Ltr,cacheSize=16)
            for((name,index)in listOf("pagination-stress" to 0,"native-typography" to 0,"aq" to 1)) {
                val path=java.io.File(targetContext.cacheDir,"corpus/$name.epub")
                val source=decoder.decodeFromString<ChapterDto>(checkNotNull(RustBridge.getChapter(path.path,index)))
                val presentation=decoder.decodeFromString<NativePresentation>(checkNotNull(RustBridge.nativePresentation(path.path,index,"mixed",false,"[]")))
                val blocks=source.blocks.mapIndexed {at,block->val raw=when(block){is ContentBlock.Paragraph->block.spans.joinToString(""){it.text};is ContentBlock.Heading->block.spans.joinToString(""){it.text};else->""};val style=presentation.styles[at].inUtf16(raw);when(block){is ContentBlock.Paragraph->block.copy(nativeStyle=style);is ContentBlock.Heading->block.copy(nativeStyle=style);is ContentBlock.Image->block.copy(nativeStyle=style);else->block}}
                val chapter=Chapter(source.title,blocks,source.sourceHref,source.images.map {it.copy(nativeStyle=presentation.styles[it.block])})
                kotlinx.coroutines.runBlocking { for(image in blocks.filterIsInstance<ContentBlock.Image>()+chapter.images.map {it.content()}) {ReaderImageCache.load(image.data)} }
                for(width in listOf(320,640)) {
                    val pages=paginateContent(chapter,18f,650.dp,width.dp,density,false,textMeasurer=measurer)
                    val selected=if(name!="aq")pages.indices.toList()else listOf(0,pages.size/2,pages.size-1).distinct()
                    var maxHeight=0
                    for(page in selected) {
                        val height=java.util.concurrent.atomic.AtomicInteger(-1)
                        val content=pages[page]
                        runOnMainSync { renderActivity.setContent {
                            androidx.compose.foundation.layout.Column(androidx.compose.ui.Modifier
                                .requiredWidth(width.dp).wrapContentHeight(unbounded=true)
                                .onGloballyPositioned {height.set(it.size.height)}) {
                                for(item in content) ContentBlockView(blockIndex=item.sourceBlockIndex,block=item.block,
                                    sourceCharOffset=item.sourceCharOffset,imageMaxHeight=item.imageMaxHeight,fontSize=18f,
                                    textColor=androidx.compose.ui.graphics.Color.Black,linkColor=androidx.compose.ui.graphics.Color.Blue,
                                    fontFamily=androidx.compose.ui.text.font.FontFamily.Default,onLinkClick={},onTextTapped={},lineSpacing=1.5f,paraSpacing=0.5f)
                            }
                        } }
                        waitForIdleSync()
                        val deadline=System.currentTimeMillis()+3000
                        while(height.get()<0&&System.currentTimeMillis()<deadline)Thread.sleep(10)
                        check(height.get()>0){"Page did not compose: $name/$width/$page"}
                        val budget=650*density.density
                        check(height.get()<=budget+2){"Painted content clipped: $name/$width/$page height=${height.get()} budget=$budget blocks=${content.map {item->"${item.block::class.simpleName}/${item.sourceBlockIndex}/${item.sourceCharOffset}/${item.block.nativeStyle()}"}}"}
                        maxHeight=maxOf(maxHeight,height.get())
                    }
                    results.put(JSONObject().put("book",name).put("width",width).put("checkedPages",selected.size).put("maximumPaintedHeightPx",maxHeight).put("availableHeightPx",650*density.density))
                }
            }
            report.put("passed",true).put("layouts",results)
        }catch(error:Throwable){report.put("passed",false).put("error",error.toString()).put("layouts",results)}
        activity?.let {runOnMainSync {it.finish()}}
        java.io.File(targetContext.cacheDir,"page-bounds-verification.json").writeText(report.toString(2))
        finish(if(report.optBoolean("passed"))Activity.RESULT_OK else Activity.RESULT_CANCELED,Bundle().apply {putString("stream",report.toString(2))})
    }

    private fun verifyCorpus() {
        val report=JSONObject();val results=org.json.JSONArray()
        try {
            val decoder=Json {ignoreUnknownKeys=true}
            for((name,index)in listOf("pride-and-prejudice" to 1,"alice-standard" to 2,"accessible-epub-3" to 18,"hefty-water" to 0,"kusamakura" to 2,"hongloumeng" to 1,"aq" to 1,"zhaohuaxishi" to 1,"tiangongkaiwu" to 1,"pagination-stress" to 0)) {
                val path=java.io.File(targetContext.cacheDir,"corpus/$name.epub");check(path.exists()) {"Corpus file missing: $name"}
                val source=decoder.decodeFromString<ChapterDto>(checkNotNull(RustBridge.getChapter(path.path,index)))
                val styles=decoder.decodeFromString<NativePresentation>(checkNotNull(RustBridge.nativePresentation(path.path,index,"mixed",false,"[]")))
                check(styles.errors.isEmpty())
                val blocks=source.blocks.mapIndexed {at,block->val raw=when(block){is ContentBlock.Paragraph->block.spans.joinToString(""){it.text};is ContentBlock.Heading->block.spans.joinToString(""){it.text};else->""};val style=styles.styles[at].inUtf16(raw);when(block){is ContentBlock.Paragraph->block.copy(nativeStyle=style);is ContentBlock.Heading->block.copy(nativeStyle=style);is ContentBlock.Image->block.copy(nativeStyle=style);else->block}}
                val ch=Chapter(source.title,blocks,source.sourceHref,source.images.map {it.copy(nativeStyle=styles.styles[it.block])})
                for(width in listOf(320,640)) {
                    val pages=paginateContent(ch,18f,650.dp,width.dp,Density(1f),false)
                    check(pages.isNotEmpty())
                    for((blockIndex,block)in blocks.withIndex()) {
                        val original=when(block){is ContentBlock.Paragraph->block.spans.joinToString(""){it.text};is ContentBlock.Heading->block.spans.joinToString(""){it.text};else->null}?:continue
                        if(block.nativeStyle().hidden)continue
                        val fragments=pages.flatten().filter {it.sourceBlockIndex==blockIndex&&it.block !is ContentBlock.Image}
                        val reconstructed=fragments.joinToString(""){when(val b=it.block){is ContentBlock.Paragraph->b.spans.joinToString(""){it.text};is ContentBlock.Heading->b.spans.joinToString(""){it.text};else->""}}
                        check(original==reconstructed){"Native pagination changed source $name:$blockIndex"}
                    }
                    results.put(JSONObject().put("name",name).put("chapter",index).put("width",width).put("pages",pages.size).put("stable_source",true))
                }
            }
            report.put("passed",true).put("layouts",results)
        }catch(error:Throwable){report.put("passed",false).put("error",error.toString()).put("layouts",results)}
        java.io.File(targetContext.cacheDir,"corpus-verification.json").writeText(report.toString(2))
        finish(if(report.optBoolean("passed"))Activity.RESULT_OK else Activity.RESULT_CANCELED,Bundle().apply {putString("stream",report.toString(2))})
    }

    private fun verifyTypography() {
        val report=JSONObject()
        try {
            val path=java.io.File(targetContext.cacheDir,"native-typography.epub")
            context.assets.open("native-typography.epub").use {input->path.outputStream().use {input.copyTo(it)}}
            val decoder=Json {ignoreUnknownKeys=true}
            val ch=decoder.decodeFromString<ChapterDto>(checkNotNull(RustBridge.getChapter(path.path,0)))
            var layouts=0
            for(mode in listOf("none","mixed","publisher")) {
                val view=decoder.decodeFromString<NativePresentation>(checkNotNull(RustBridge.nativePresentation(path.path,0,mode,false,"[]")))
                check(view.errors.isEmpty());check(view.styles.count {it.table!=null}==3);check(view.styles.sumOf {it.ruby.size}==3);check(view.styles.any {it.list.any {line->line.depth==2}})
                val blocks=ch.blocks.mapIndexed {i,b->val source=when(b){is ContentBlock.Paragraph->b.spans.joinToString(""){it.text};is ContentBlock.Heading->b.spans.joinToString(""){it.text};else->""};val style=view.styles[i].inUtf16(source);when(b){is ContentBlock.Paragraph->b.copy(nativeStyle=style);is ContentBlock.Heading->b.copy(nativeStyle=style);is ContentBlock.Image->b.copy(nativeStyle=style);else->b}}
                val inline=blocks.filterIsInstance<ContentBlock.Paragraph>().first {it.spans.joinToString(""){it.text}.contains("文字 😀 中文")}
                val annotated=buildSpanAnnotatedString(inline.spans,18f,androidx.compose.ui.graphics.Color.Black,androidx.compose.ui.graphics.Color.Blue,androidx.compose.ui.text.font.FontFamily.Default,null,native=inline.nativeStyle)
                check(annotated.text==inline.spans.joinToString(""){it.text});check(annotated.spanStyles.any {it.item.baselineShift!=null});check(annotated.getStringAnnotations("URL",0,annotated.length).any {it.item.startsWith("reader-note:")})
                for(width in listOf(320,640)) {
                    val density=Density(1f)
                    val display=Chapter(ch.title,blocks,ch.sourceHref,ch.images.map {it.copy(nativeStyle=view.styles[it.block])})
                    val pages=paginateContent(display,18f,650.dp,width.dp,density,false)
                    check(pages.isNotEmpty());check(pages.flatten().count {it.block.nativeStyle().table!=null}==3)
                    for(page in pages){val height=page.sumOf {item->val style=item.block.nativeStyle();val font=style.font(18f);(if(item.block is ContentBlock.Image)estimateImageBlockHeight(item.block.data,width.toFloat(),density,font,item.imageMaxHeight.value,style)else estimateBlockHeight(item.block,font,font*style.lineScale(1.5f),width.toFloat(),density)).toDouble()};check(height<=650f+30f){"Typography page overflows: $mode/$width $height"}}
                    layouts++
                }
            }
            val note=decoder.decodeFromString<NativeNote>(checkNotNull(RustBridge.nativeNote(path.path,0,"notes.xhtml#n%E4%B8%80")))
            check(note.text.contains("这是跨章节的注释原文"))
            report.put("passed",true).put("typography_layouts",layouts).put("unicode_source",true).put("native_note",true)
        }catch(error:Throwable){report.put("passed",false).put("error",error.toString())}
        java.io.File(targetContext.cacheDir,"typography-verification.json").writeText(report.toString(2))
        finish(if(report.optBoolean("passed"))Activity.RESULT_OK else Activity.RESULT_CANCELED,Bundle().apply {putString("stream",report.toString(2))})
    }

    private fun verifyNative() {
        val report=JSONObject()
        try {
            val path=java.io.File(targetContext.cacheDir,"native-fixture.epub")
            context.assets.open("original-layout.epub").use {input->path.outputStream().use {input.copyTo(it)}}
            val decoder=Json {ignoreUnknownKeys=true}
            val chapter=decoder.decodeFromString<ChapterDto>(checkNotNull(RustBridge.getChapter(path.path,0)))
            val styled=chapter.blocks.indexOfFirst { it is ContentBlock.Paragraph && it.anchor_id=="styled" }
            val revealed=chapter.blocks.indexOfFirst {it is ContentBlock.Paragraph && it.anchor_id=="revealed"}
            for(mode in listOf("none","mixed","publisher")) {
                val initial=decoder.decodeFromString<NativePresentation>(checkNotNull(RustBridge.nativePresentation(path.path,0,mode,true,"[]")))
                check(initial.errors.isEmpty()){initial.errors.toString()}
                check(initial.styles[revealed].hidden)
                if(mode=="none")check(initial.styles[styled].align==null) else check(initial.styles[styled].align=="right")
                val action=checkNotNull(initial.styles.firstOrNull {it.action!=null}?.action)
                val events="[{\"node\":$action}]"
                val opened=decoder.decodeFromString<NativePresentation>(checkNotNull(RustBridge.nativePresentation(path.path,0,mode,true,events)))
                check(!opened.styles[revealed].hidden)
                val disabled=decoder.decodeFromString<NativePresentation>(checkNotNull(RustBridge.nativePresentation(path.path,0,mode,false,events)))
                check(disabled.styles[revealed].hidden && disabled.styles.all {it.action==null})
                val display=chapter.blocks.mapIndexed {i,b->when(b){is ContentBlock.Paragraph->b.copy(nativeStyle=opened.styles[i]);is ContentBlock.Heading->b.copy(nativeStyle=opened.styles[i]);else->b}}
                check(paginateContent(Chapter(chapter.title,display,chapter.sourceHref,chapter.images),18f,600.dp,320.dp,Density(1f),false).isNotEmpty())
            }
            report.put("passed",true)
        }catch(error:Throwable){report.put("passed",false);report.put("error",error.toString())}
        java.io.File(targetContext.cacheDir,"native-verification.json").writeText(report.toString(2))
        finish(if(report.optBoolean("passed"))Activity.RESULT_OK else Activity.RESULT_CANCELED,Bundle().apply{putString("report",report.toString())})
    }
}
