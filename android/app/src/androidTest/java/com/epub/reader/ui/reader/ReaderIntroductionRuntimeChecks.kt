package com.zhongbai233.epub.reader.ui.reader

import android.app.Activity
import android.app.Instrumentation
import android.content.Intent
import android.graphics.Bitmap
import android.os.Bundle
import android.view.accessibility.AccessibilityNodeInfo
import androidx.lifecycle.ViewModelProvider
import com.zhongbai233.epub.reader.MainActivity
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.model.*
import com.zhongbai233.epub.reader.viewmodel.ReaderViewModel
import java.io.File
import org.json.JSONObject

internal object ReaderIntroductionRuntimeChecks {
    fun run(test: Instrumentation) {
        val context=test.targetContext
        check(context.packageName=="local.reader.firstuse") { "Use the isolated first-use package" }
        // Reset only preferences created by this isolated diagnostic package.
        context.getSharedPreferences("reader_settings",0).edit().clear().commit()
        val out=File(context.filesDir,"first-reader-checks").apply {mkdirs()}
        val report=JSONObject()
        fun nodes(): List<AccessibilityNodeInfo> {
            val result=mutableListOf<AccessibilityNodeInfo>()
            fun visit(node: AccessibilityNodeInfo?) {if(node!=null){result+=node;for(i in 0 until node.childCount) visit(node.getChild(i))}}
            visit(test.uiAutomation.rootInActiveWindow);return result
        }
        fun contains(key:String)=nodes().any {it.text?.toString()==I18n.t(key) || it.contentDescription?.toString()==I18n.t(key)}
        fun await(message:String, condition:()->Boolean) {repeat(250){test.waitForIdleSync();if(condition())return;Thread.sleep(100)};error(message)}
        fun screen(name:String) {test.waitForIdleSync();Thread.sleep(300);test.uiAutomation.takeScreenshot()?.let {bmp->File(out,"$name.png").outputStream().use{bmp.compress(Bitmap.CompressFormat.PNG,100,it)};bmp.recycle()}}
        fun start(): Pair<MainActivity,ReaderViewModel> {
            val activity=test.startActivitySync(Intent(context,MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK)) as MainActivity
            lateinit var vm:ReaderViewModel
            test.runOnMainSync {vm=ViewModelProvider(activity)[ReaderViewModel::class.java]}
            return activity to vm
        }
        try {
            var (activity,vm)=start()
            check(!vm.isScrollMode && vm.readerPageAnimation=="Realistic" && vm.showImmersiveStatus)
            check(vm.appearanceLinkage.source==AutomaticAppearanceSource.ambient && !vm.readerIntroductionSeen)
            val fixture=File(context.filesDir,"reading-introduction.epub")
            test.context.assets.open("first-reader.epub").use {input->fixture.outputStream().use {input.copyTo(it)}}
            test.runOnMainSync {vm.updateLanguage("zh_cn");vm.openFromPath(fixture.path)}
            await("First introduction did not appear") {contains("reader.intro_title") && vm.readerIntroductionSeen}
            screen("introduction")
            var startNode=nodes().firstOrNull {it.text?.toString()==I18n.t("reader.intro_start") || it.contentDescription?.toString()==I18n.t("reader.intro_start")}
                ?: error("Introduction start label missing")
            repeat(6) {if(!startNode.isClickable) startNode=startNode.parent ?: startNode}
            check(startNode.isClickable && startNode.performAction(AccessibilityNodeInfo.ACTION_CLICK))
            await("Introduction did not dismiss") {!contains("reader.intro_title")}
            test.runOnMainSync {vm.closeBook();vm.openFromPath(fixture.path)}
            await("Reopened book not ready") {vm.currentBook!=null && !vm.isLoading}
            test.waitForIdleSync();Thread.sleep(800)
            check(!contains("reader.intro_title"))
            test.runOnMainSync {vm.updateScrollMode(true);vm.updateReaderPageAnimation("Slide");vm.updateShowImmersiveStatus(false);vm.updateAppearanceLinkage(vm.appearanceLinkage.copy(source=AutomaticAppearanceSource.system));activity.finish()}
            val restarted=start();activity=restarted.first;vm=restarted.second
            check(vm.readerIntroductionSeen && vm.isScrollMode && vm.readerPageAnimation=="Slide" && !vm.showImmersiveStatus)
            check(vm.appearanceLinkage.source==AutomaticAppearanceSource.system)
            await("Restored reading not ready") {vm.currentBook!=null && !vm.isLoading}
            test.waitForIdleSync();Thread.sleep(800)
            check(!contains("reader.intro_title"))
            test.runOnMainSync {activity.finish()}
            context.getSharedPreferences("reader_settings",0).edit().remove("reader.introduction.v1").commit()
            val upgraded=start();activity=upgraded.first;vm=upgraded.second
            check(vm.readerIntroductionSeen && vm.isScrollMode && vm.readerPageAnimation=="Slide" && !vm.showImmersiveStatus)
            await("Upgraded reading not ready") {vm.currentBook!=null && !vm.isLoading}
            test.waitForIdleSync();Thread.sleep(500)
            check(!contains("reader.intro_title"))
            report.put("existing_reader_upgrade_no_repeat",true)
            report.put("passed",true).put("fresh_defaults",true).put("shown_marked_before_dismiss",true).put("second_book_no_repeat",true).put("restart_no_repeat",true).put("saved_choices_preserved",true)
            // Rehearse the first entry again only in this isolated diagnostic app.
            test.runOnMainSync {vm.updateScrollMode(false);vm.updateReaderPageAnimation("Realistic");vm.updateShowImmersiveStatus(true);vm.updateAppearanceLinkage(AppearanceLinkage.firstRun());activity.finish()}
            context.getSharedPreferences("reader_settings",0).edit().putBoolean("reader.introduction.v1",false).commit()
            start()
            await("Preview introduction did not appear") {contains("reader.intro_title")}
            screen("preview");report.put("preview_rearmed_after_checks",true)
        } catch(error:Throwable) {report.put("passed",false).put("error",error.toString());screen("failure")}
        File(out,"result.json").writeText(report.toString(2))
        test.finish(if(report.optBoolean("passed")) Activity.RESULT_OK else Activity.RESULT_CANCELED,Bundle().apply {putString("stream",report.toString(2))})
    }
}
