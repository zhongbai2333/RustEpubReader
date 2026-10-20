package com.zhongbai233.epub.reader.ui.reader

import android.accessibilityservice.AccessibilityServiceInfo
import android.app.Activity
import android.app.Instrumentation
import android.app.UiAutomation
import android.content.Intent
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
        val cases=JSONArray()
        var activity:MainActivity?=null
        val automation=test.getUiAutomation(UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES)
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
            fun location()=ReaderLocation(vm.currentChapter,vm.currentBlock,vm.currentCharOffset)
            fun action(label:String):Pair<AccessibilityNodeInfo,AccessibilityNodeInfo.AccessibilityAction>? = nodes()
                .asSequence().filter {it.isVisibleToUser && it.packageName?.toString()==context.packageName}.mapNotNull {node->node.actionList.firstOrNull {it.label?.toString()==label}?.let {node to it}}.firstOrNull()
            for(mode in listOf("Realistic","Slide","Cover","None")) {
                test.runOnMainSync {vm.updateScrollMode(false);vm.updateReaderPageAnimation(mode);vm.goToPosition(1,0,0,remember=false)}
                await("Reader reflow did not settle for $mode") {location()==ReaderLocation(1,0,0) && action(I18n.t("reader.next_page"))!=null}
                Thread.sleep(350) // Let the production opening/reflow gate release; not timing data.
                val origin=location()
                val text=nodes().firstOrNull {it.isVisibleToUser && it.packageName?.toString()==context.packageName && (it.text?.length?:0)>24}
                    ?:error("No readable native text node: $mode")
                text.performAction(AccessibilityNodeInfo.ACTION_CLEAR_ACCESSIBILITY_FOCUS)
                check(text.performAction(AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS)) {"Native text cannot receive accessibility focus: $mode"}
                val next=checkNotNull(action(I18n.t("reader.next_page")))
                check(next.first.performAction(next.second.id)) {"Accessible next rejected: $mode"}
                await("Accessible next did not move: $mode") {location()!=origin}
                val previous=checkNotNull(action(I18n.t("reader.previous_page")))
                check(previous.first.performAction(previous.second.id)) {"Accessible previous rejected: $mode"}
                await("Accessible previous did not return: $mode") {location()==origin}
                cases.put(JSONObject().put("mode",mode).put("native_text_focus",true).put("custom_actions",true).put("exact_return",true))
            }
            report.put("passed",true)
        } catch(error:Throwable) {report.put("error",error.toString())}
        finally {
            activity?.let {host->test.runOnMainSync {host.finish()}}
        }
        report.put("cases",cases)
        File(context.cacheDir,"accessibility-verification.json").writeText(report.toString(2))
        test.finish(if(report.optBoolean("passed")) Activity.RESULT_OK else Activity.RESULT_CANCELED,
            android.os.Bundle().apply {putString("stream",report.toString(2))})
    }
}
