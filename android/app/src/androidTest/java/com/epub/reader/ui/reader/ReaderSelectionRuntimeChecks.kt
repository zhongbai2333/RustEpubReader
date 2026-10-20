package com.zhongbai233.epub.reader.ui.reader

import android.app.Activity
import android.app.Instrumentation
import android.content.Intent
import android.graphics.Bitmap
import android.os.Bundle
import android.os.SystemClock
import android.view.InputDevice
import android.view.MotionEvent
import android.view.accessibility.AccessibilityNodeInfo
import androidx.compose.ui.geometry.Offset
import androidx.lifecycle.ViewModelProvider
import com.zhongbai233.epub.reader.MainActivity
import com.zhongbai233.epub.reader.RustBridge
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.viewmodel.ReaderViewModel
import java.io.File
import org.json.JSONArray
import org.json.JSONObject

/** Actual native long-press/drag, visible color action, and independent JNI reread. */
internal object ReaderSelectionRuntimeChecks {
    fun run(test: Instrumentation, reducedMotion:Boolean=false) {
        val context=test.targetContext
        check(context.packageName=="local.reader.columns") {"Use the isolated diagnostic app"}
        val output=File(context.cacheDir,if(reducedMotion) "selection-reduced-runtime" else "selection-runtime").also {check(it.mkdirs())}
        val report=JSONObject();val cases=JSONArray()
        var activity:MainActivity?=null
        try {
            val source=File(context.cacheDir,"selection.epub");check(source.isFile)
            val entry=JSONObject(checkNotNull(RustBridge.addOrUpdateBook(context.filesDir.path,"Native selection",source.path,0,"")))
            val path=entry.getString("uri");val id=entry.getString("id")
            val expected="文字 😀 中文"
            fun config()=JSONObject(checkNotNull(RustBridge.getBookConfig(context.filesDir.path,id)))
            fun highlightIDs():Set<String> {
                val values=config().optJSONArray("highlights") ?: return emptySet()
                return (0 until values.length()).map {values.getJSONObject(it).getString("id")}.toSet()
            }
            fun await(message:String,condition:()->Boolean) {
                val end=SystemClock.elapsedRealtime()+20_000
                while(SystemClock.elapsedRealtime()<end) {test.waitForIdleSync();if(condition())return;Thread.sleep(25)}
                error(message)
            }
            fun nodes():List<AccessibilityNodeInfo> {
                val result=mutableListOf<AccessibilityNodeInfo>()
                fun visit(node:AccessibilityNodeInfo?) {if(node!=null){result+=node;for(i in 0 until node.childCount)visit(node.getChild(i))}}
                visit(test.uiAutomation.rootInActiveWindow);return result
            }
            fun click(label:String) {
                await("Selection action not visible: $label") {nodes().any {it.text?.toString()==label || it.contentDescription?.toString()==label}}
                val node=nodes().first {it.text?.toString()==label || it.contentDescription?.toString()==label}
                var target:AccessibilityNodeInfo?=node
                while(target!=null && !target.isClickable)target=target.parent
                check(target?.performAction(AccessibilityNodeInfo.ACTION_CLICK)==true) {"Selection action was not clickable: $label"}
            }
            for(mode in listOf("None","Realistic","Slide","Cover","Scroll")) {
                context.getSharedPreferences("reader_settings",0).edit().clear().putBoolean("reader.introduction.v1",true).putString("reader_language","zh_cn").commit()
                activity=test.startActivitySync(Intent(context,MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK)) as MainActivity
                lateinit var vm:ReaderViewModel
                var layouts:Map<Int,BlockLayoutInfo> = emptyMap()
                var selected:TextSelectionState?=null;var excerpt=""
                test.runOnMainSync {
                    vm=ViewModelProvider(checkNotNull(activity))[ReaderViewModel::class.java]
                    vm.readerSelectionObserver={blocks,value,text->layouts=blocks;selected=value;excerpt=text}
                    vm.openFromPath(path,0)
                }
                await("Native reader did not open in $mode") {vm.currentBook!=null && !vm.isLoading}
                // ValueAnimator receives the system scale when the first real
                // WindowSession attaches; its pre-window static default is 1.
                if(reducedMotion) await("System animator duration scale is not disabled") {!android.animation.ValueAnimator.areAnimatorsEnabled()}
                report.put("system_animators_enabled",android.animation.ValueAnimator.areAnimatorsEnabled())
                test.runOnMainSync {
                    vm.updateForceSingleColumn(true);vm.updateFontSize(24f)
                    vm.updateReaderPageAnimation(if(mode=="Scroll") "None" else mode);vm.updateScrollMode(mode=="Scroll")
                    vm.goToPosition(0,1,0)
                }
                var block=-1;var first=Offset.Zero;var last=Offset.Zero;var begin=0
                await("Native selection layout missing in $mode") {
                    var found=false
                    test.runOnMainSync {
                        val value=layouts.entries.firstOrNull {it.value.coordinates.isAttached && it.value.text.contains(expected)}
                        if(value!=null) {
                            val info=value.value;val at=info.text.indexOf(expected);val end=at+expected.length
                            if(end<info.text.length) {
                                fun point(offset:Int):Offset {val r=info.layoutResult.getBoundingBox(offset);return info.coordinates.localToWindow(Offset(r.left+r.width*0.2f,r.center.y))}
                                first=point(at);last=point(end);block=value.key;begin=at+info.sourceCharOffset;found=true
                            }
                        }
                    };found
                }
                Thread.sleep(700)
                val original=highlightIDs();var down=SystemClock.uptimeMillis()
                fun event(action:Int,point:Offset) {
                    if(action==MotionEvent.ACTION_DOWN)down=SystemClock.uptimeMillis()
                    val value=MotionEvent.obtain(down,SystemClock.uptimeMillis(),action,point.x,point.y,0);value.source=InputDevice.SOURCE_TOUCHSCREEN
                    check(test.uiAutomation.injectInputEvent(value,true));value.recycle()
                }
                event(MotionEvent.ACTION_DOWN,first);Thread.sleep(android.view.ViewConfiguration.getLongPressTimeout().toLong()+200)
                for(step in 1..16) {event(MotionEvent.ACTION_MOVE,first+(last-first)*(step/16f));Thread.sleep(25)}
                event(MotionEvent.ACTION_UP,last)
                await("Native drag did not select the exact Unicode excerpt in $mode: $excerpt / $selected") {excerpt==expected && selected!=null}
                val selection=checkNotNull(selected)
                check(block==1 && begin==0) {"Selection did not target the canonical fixture prefix: $block/$begin"}
                check(selection==TextSelectionState(block,begin,block,begin+expected.length)) {"Native UTF16 range changed in $mode: $selection"}
                click(I18n.t("selection.highlight"));click(I18n.t("common.color_yellow"))
                var saved:JSONObject?=null
                await("Selected highlight was not persisted in $mode") {
                    val values=config().optJSONArray("highlights")
                    saved=if(values==null)null else (0 until values.length()).map {values.getJSONObject(it)}.firstOrNull {it.getString("id") !in original}
                    saved!=null
                }
                val value=checkNotNull(saved)
                check(value.getInt("chapter")==0 && value.getInt("start_block")==block && value.getInt("end_block")==block)
                val scalarStart=0
                check(value.getInt("start_offset")==scalarStart && value.getInt("end_offset")==scalarStart+expected.codePointCount(0,expected.length)) {"Persisted scalar range changed: $value"}
                await("Selection UI did not close after saving the highlight") {selected==null && nodes().none {it.contentDescription?.toString()==I18n.t("common.color_yellow")}}
                val frames=java.util.concurrent.CountDownLatch(1)
                test.runOnMainSync {val decor=checkNotNull(activity).window.decorView;decor.postOnAnimation {decor.postOnAnimation {frames.countDown()}}}
                check(frames.await(3,java.util.concurrent.TimeUnit.SECONDS)) {"No presented selection frame"}
                var image=test.uiAutomation.takeScreenshot()
                repeat(10) {if(image==null){Thread.sleep(60);image=test.uiAutomation.takeScreenshot()}}
                checkNotNull(image).let {bitmap->File(output,"selection-$mode.png").outputStream().use {check(bitmap.compress(Bitmap.CompressFormat.PNG,100,it))};bitmap.recycle()}
                val row=JSONObject().put("mode",mode).put("native_drag",true).put("visible_highlight_action",true).put("persisted",true).put("text",expected).put("start_scalar",value.getInt("start_offset")).put("end_scalar",value.getInt("end_offset"))
                if(mode=="Realistic") {
                    val originalPage=vm.currentPageLabel;val originalChapter=vm.currentChapter
                    check(originalPage.isNotBlank()) {"Rendered page label is not ready"}
                    val width=context.resources.displayMetrics.widthPixels.toFloat();val height=context.resources.displayMetrics.heightPixels.toFloat()
                    fun swipe(forward:Boolean) {
                        val beginPoint=Offset(width*(if(forward)0.85f else 0.15f),height*0.75f)
                        val endPoint=Offset(width*(if(forward)0.15f else 0.85f),height*0.75f)
                        event(MotionEvent.ACTION_DOWN,beginPoint)
                        for(step in 1..10){event(MotionEvent.ACTION_MOVE,beginPoint+(endPoint-beginPoint)*(step/10f));Thread.sleep(16)}
                        event(MotionEvent.ACTION_UP,endPoint)
                    }
                    swipe(true);await("Quick curl swipe stopped working after long-press arbitration") {vm.currentPageLabel!=originalPage || vm.currentChapter!=originalChapter}
                    Thread.sleep(300);swipe(false);await("Quick reverse curl swipe lost the original page") {vm.currentPageLabel==originalPage && vm.currentChapter==originalChapter}
                    row.put("quick_curl_swipe_exact_return",true)
                }
                cases.put(row)
                test.runOnMainSync {vm.readerSelectionObserver=null;vm.closeBook();activity?.finish()};activity=null
            }
            report.put("passed",true).put("cases",cases).put("physical_device",false)
        } catch(error:Throwable) {report.put("passed",false).put("error",error.stackTraceToString()).put("cases",cases)}
        finally {activity?.let {test.runOnMainSync {it.finish()}}}
        File(output,"summary.json").writeText(report.toString(2))
        test.finish(if(report.optBoolean("passed"))Activity.RESULT_OK else Activity.RESULT_CANCELED,Bundle().apply {putString("stream",report.toString(2))})
    }
}
