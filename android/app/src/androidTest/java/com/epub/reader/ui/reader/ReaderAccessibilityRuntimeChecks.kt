package com.zhongbai233.epub.reader.ui.reader

import android.accessibilityservice.AccessibilityServiceInfo
import android.app.Activity
import android.app.Instrumentation
import android.app.UiAutomation
import android.content.Intent
import android.graphics.Bitmap
import android.view.accessibility.AccessibilityManager
import android.view.accessibility.AccessibilityNodeInfo
import androidx.lifecycle.ViewModelProvider
import com.zhongbai233.epub.reader.MainActivity
import com.zhongbai233.epub.reader.RustBridge
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.model.ReaderLocation
import com.zhongbai233.epub.reader.viewmodel.ReaderViewModel
import java.io.File
import org.json.JSONArray
import org.json.JSONObject

/** A real enabled TalkBack service must coexist with this public UiAutomation session. */
internal object ReaderAccessibilityRuntimeChecks {
    fun run(test: Instrumentation) {
        val context=test.targetContext
        check(context.packageName in setOf("local.reader.accessibility","local.reader.columns")) {"Use the isolated accessibility package"}
        val report=JSONObject().put("passed",false).put("actual_audio_verified",false)
        File(context.cacheDir,"accessibility-failure.png").delete()
        val cases=JSONArray()
        val events=JSONArray()
        var activity:MainActivity?=null
        val automation=test.getUiAutomation(UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES)
        ReaderAccessibilityTurnTrace.start()
        fun await(message:String,condition:()->Boolean) {
            repeat(200) {test.waitForIdleSync();if(condition())return;Thread.sleep(50)}
            error(message)
        }
        fun nodes():List<AccessibilityNodeInfo> {
            val found=mutableListOf<AccessibilityNodeInfo>()
            fun visit(node:AccessibilityNodeInfo?) {
                if(node!=null) {found+=node;for(i in 0 until node.childCount)visit(node.getChild(i))}
            }
            visit(automation.rootInActiveWindow);return found
        }
        try {
            val manager=checkNotNull(context.getSystemService(AccessibilityManager::class.java))
            await("TalkBack did not coexist with UiAutomation") {
                manager.getEnabledAccessibilityServiceList(AccessibilityServiceInfo.FEEDBACK_SPOKEN).any {it.id.contains("talkback",ignoreCase=true)} && manager.isTouchExplorationEnabled
            }
            report.put("spoken_services",JSONArray(manager.getEnabledAccessibilityServiceList(AccessibilityServiceInfo.FEEDBACK_SPOKEN).map {it.id}))
                .put("touch_exploration",manager.isTouchExplorationEnabled)
                .put("dont_suppress_accessibility_services",true)
            val input=File(context.cacheDir,"accessibility.epub");check(input.isFile)
            val entry=JSONObject(checkNotNull(RustBridge.addOrUpdateBook(context.filesDir.path,"Accessibility native paging",input.path,1,"第2章")))
            val path=entry.getString("uri")
            RustBridge.updatePosition(context.filesDir.path,path,1,"第2章",0,0)
            context.getSharedPreferences("reader_settings",0).edit()
                .putBoolean("reader.introduction.v1",true).putString("reader_language","en")
                .putString("last_book_uri",path).putInt("last_book_chapter",1).commit()
            val host=test.startActivitySync(Intent(context,MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK)) as MainActivity
            activity=host
            lateinit var vm:ReaderViewModel
            test.runOnMainSync {vm=ViewModelProvider(host)[ReaderViewModel::class.java];vm.updateForceSingleColumn(true)}
            await("Reader did not open") {vm.currentBook!=null && !vm.isLoading && vm.currentChapter==1}
            test.runOnMainSync {vm.updateFontSize(24f);vm.updateLineSpacing(1.5f)}
            fun location()=ReaderLocation(vm.currentChapter,vm.currentBlock,vm.currentCharOffset)
            fun record(event:String) {
                events.put(JSONObject().put("event",event).put("mode",vm.readerPageAnimation)
                    .put("location",location().toString()).put("page_label",vm.currentPageLabel)
                    .put("active_package",automation.rootInActiveWindow?.packageName?.toString()))
            }
            fun action(label:String):Pair<AccessibilityNodeInfo,AccessibilityNodeInfo.AccessibilityAction>? = nodes()
                .asSequence().filter {it.isVisibleToUser && it.packageName?.toString()==context.packageName}.mapNotNull {node->node.actionList.firstOrNull {it.label?.toString()==label}?.let {node to it}}.firstOrNull()
            fun perform(label:String) {
                // A settled locator can precede completion of the animation.
                // Retry only rejected commands; after acceptance never issue a
                // second turn, and independently assert the resulting location.
                await("Accessible action not accepted: $label") {
                    action(label)?.let {(node,command)->
                        ReaderAccessibilityTurnTrace.record("instrumentation action=$label id=${command.id} node=${node.viewIdResourceName}")
                        node.performAction(command.id)
                    } == true
                }
            }
            for(mode in listOf("Realistic","Slide","Cover","None")) {
                test.runOnMainSync {vm.updateScrollMode(false);vm.updateReaderPageAnimation(mode);vm.goToPosition(1,0,0,remember=false)}
                await("Reader reflow did not settle for $mode") {location()==ReaderLocation(1,0,0) && action(I18n.t("reader.next_page"))!=null}
                Thread.sleep(350) // Let the production opening/reflow gate release; not timing data.
                val origin=location()
                record("origin")
                var text:AccessibilityNodeInfo?=null
                await("No readable native text node: $mode") {
                    text=nodes().firstOrNull {it.isVisibleToUser && it.packageName?.toString()==context.packageName && (it.text?.length?:0)>24}
                    text!=null
                }
                checkNotNull(text).performAction(AccessibilityNodeInfo.ACTION_CLEAR_ACCESSIBILITY_FOCUS)
                check(checkNotNull(text).performAction(AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS)) {"Native text cannot receive accessibility focus: $mode"}
                perform(I18n.t("reader.next_page"))
                await("Accessible next did not move: $mode") {location()!=origin}
                record("next settled")
                var outgoing:AccessibilityNodeInfo?=null
                if(mode=="Cover") await("Missing outgoing synthetic page text") {
                    outgoing=nodes().firstOrNull {it.isVisibleToUser && it.packageName?.toString()==context.packageName && it.text?.contains("第6段")==true}
                    outgoing!=null
                }
                perform(I18n.t("reader.previous_page"))
                record("previous accepted")
                var outgoingShowAccepted=false
                outgoing?.let {oldParagraph->
                    // TalkBack can ask to reveal its old focused paragraph while
                    // a custom page return is running. Exercise that exact race.
                    Thread.sleep(50)
                    outgoingShowAccepted=oldParagraph.performAction(android.R.id.accessibilityActionShowOnScreen)
                    ReaderAccessibilityTurnTrace.record("instrumentation outgoing SHOW_ON_SCREEN accepted=$outgoingShowAccepted")
                    record("outgoing focus visibility request")
                }
                var last=location()
                await("Accessible previous did not return: $mode, origin=$origin") {
                    if(location()!=last) {last=location();record("previous position changed")}
                    location()==origin
                }
                record("returned")
                check(!outgoingShowAccepted) {"Outgoing focus must not restart a page scroll: $mode"}
                cases.put(JSONObject().put("mode",mode).put("native_text_focus",true).put("custom_actions",true).put("exact_return",true)
                    .put("outgoing_focus_scroll_rejected",!outgoingShowAccepted))
            }
            report.put("passed",true)
        } catch(error:Throwable) {
            report.put("error",error.toString())
            report.put("failure_active_package",automation.rootInActiveWindow?.packageName?.toString())
            report.put("failure_visible_nodes",JSONArray(nodes().filter {it.isVisibleToUser && it.packageName?.toString()==context.packageName}.map {node->
                val bounds=android.graphics.Rect();node.getBoundsInScreen(bounds)
                JSONObject().put("text",node.text?.toString()).put("description",node.contentDescription?.toString())
                    .put("focused",node.isAccessibilityFocused).put("bounds",bounds.toShortString())
            }))
            try {
                automation.takeScreenshot()?.let { image ->
                    try {File(context.cacheDir,"accessibility-failure.png").outputStream().use {image.compress(Bitmap.CompressFormat.PNG,100,it)}}
                    finally {image.recycle()}
                }
            } catch(captureError:Throwable) {report.put("failure_screenshot_error",captureError.toString())}
        }
        finally {
            report.put("turn_trace",JSONArray(ReaderAccessibilityTurnTrace.finish()))
            activity?.let {host->test.runOnMainSync {host.finish()}}
        }
        report.put("cases",cases).put("events",events)
        File(context.cacheDir,"accessibility-verification.json").writeText(report.toString(2))
        test.finish(if(report.optBoolean("passed")) Activity.RESULT_OK else Activity.RESULT_CANCELED,
            android.os.Bundle().apply {putString("stream",report.toString(2))})
    }
}
