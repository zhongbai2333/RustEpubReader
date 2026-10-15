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
import org.json.JSONObject
import java.io.File

object AppearanceLinkageRuntimeChecks {
    fun run(test: Instrumentation) {
        val context=test.targetContext
        val out=File(context.cacheDir,"linkage-checks").apply {mkdirs()}
        val report=JSONObject()
        fun await(name:String, condition:()->Boolean) {repeat(160){if(condition())return;Thread.sleep(100)};error(name)}
        fun nodes():List<AccessibilityNodeInfo> {
            if(android.os.Build.VERSION.SDK_INT>=33) test.uiAutomation.clearCache()
            val result=mutableListOf<AccessibilityNodeInfo>()
            fun visit(node:AccessibilityNodeInfo?){if(node==null)return;node.refresh();result.add(node);for(i in 0 until node.childCount)visit(node.getChild(i))}
            visit(test.uiAutomation.rootInActiveWindow);return result
        }
        fun click(label:String) {
            repeat(12) {
                var node=nodes().firstOrNull {it.text?.toString()==label || it.contentDescription?.toString()==label}
                while(node!=null && !node.isClickable)node=node.parent
                if(node?.performAction(AccessibilityNodeInfo.ACTION_CLICK)==true){test.waitForIdleSync();Thread.sleep(500);return}
                nodes().filter {it.isScrollable}.maxByOrNull {n->android.graphics.Rect().also{n.getBoundsInScreen(it)}.height()}?.performAction(AccessibilityNodeInfo.ACTION_SCROLL_FORWARD)
                Thread.sleep(100)
            };error("Missing action: $label; "+nodes().joinToString {"${it.text}/${it.contentDescription}"})
        }
        fun screenshot(name:String){Thread.sleep(500);test.uiAutomation.takeScreenshot()?.let {image->File(out,"$name.png").outputStream().use {image.compress(Bitmap.CompressFormat.PNG,100,it)};image.recycle()}}
        try {
            check(context.packageName=="local.reader.appearance")
            val activity=test.startActivitySync(Intent(context,MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK)) as MainActivity
            lateinit var vm:ReaderViewModel
            val fixture=File(context.filesDir,"linkage.epub")
            test.context.assets.open("original-layout.epub").use {input->fixture.outputStream().use{input.copyTo(it)}}
            test.runOnMainSync {vm=ViewModelProvider(activity)[ReaderViewModel::class.java];vm.updateLanguage("zh_cn");vm.updateReadingAppearance(ReadingAppearance());vm.updateAppearanceLinkage(AppearanceLinkage());vm.openFromPath(fixture.path)}
            await("Reader did not open"){vm.currentBook!=null && !vm.isLoading}
            await("Set emulator system appearance dark before this check"){vm.systemAppearanceDark && vm.isDarkMode && vm.readingDark}
            Thread.sleep(1000)
            val anchor=vm.currentChapter to vm.currentBlock
            val size=vm.fontSize
            test.runOnMainSync {vm.toggleReaderDayNight(true)}
            await("Linked shortcut did not affect app and reader"){!vm.isDarkMode && !vm.readingDark && !vm.appearanceLinkage.appLinked && vm.appearanceLinkage.readingLinked}
            test.runOnMainSync {vm.closeBook()}
            await("Reader did not close"){vm.currentBook==null}
            check(!vm.isDarkMode && vm.systemAppearanceDark)
            screenshot("library-after-shortcut")
            test.runOnMainSync {vm.openFromPath(fixture.path);vm.updateAppearanceLinkage(AppearanceLinkage())}
            await("Reader reopen"){vm.currentBook!=null && !vm.isLoading}
            Thread.sleep(800)
            val metrics=context.resources.displayMetrics;val now=android.os.SystemClock.uptimeMillis()
            if(nodes().none {it.contentDescription?.toString()==I18n.t("reader.menu")}) for(action in listOf(android.view.MotionEvent.ACTION_DOWN,android.view.MotionEvent.ACTION_UP)) {
                val event=android.view.MotionEvent.obtain(now,now+30,action,metrics.widthPixels*.5f,metrics.heightPixels*.9f,0);test.sendPointerSync(event);event.recycle()
            }
            test.waitForIdleSync();Thread.sleep(300)
            click(I18n.t("reader.menu"));click(I18n.t("settings.appearance_title"))
            screenshot("linked-system")
            click(I18n.t("appearance.link_unlink_reading"))
            check(!vm.appearanceLinkage.readingLinked && vm.readingDark)
            screenshot("after-unlink")
            click(I18n.t("appearance.mode_light"))
            await("Independent reading did not apply"){vm.isDarkMode && !vm.readingDark}
            screenshot("independent-light")
            click(I18n.t("appearance.mode_automatic"))
            await("Automatic mode action did not apply"){vm.appearanceLinkage.readingMode==ReadingDisplayMode.automatic}
            screenshot("after-auto")
            click(I18n.t("appearance.link_ambient"))
            await("Ambient did not resolve light"){vm.ambientAppearanceDark==false && !vm.readingDark}
            check(vm.isDarkMode && vm.appearanceLinkage.readingSource==AutomaticAppearanceSource.ambient)
            screenshot("independent-ambient")
            click(I18n.t("appearance.link_relink_reading"))
            await("Reading relink"){vm.isDarkMode && vm.readingDark && vm.appearanceLinkage.readingLinked}
            val stored=context.getSharedPreferences("reader_settings",0).getString(AppearanceLinkage.storageKey,null)
            check(AppearanceLinkage.decode(stored)==vm.appearanceLinkage)
            check(vm.fontSize==size && vm.currentChapter to vm.currentBlock==anchor)
            report.put("passed",true).put("production_activity",true).put("linked_shortcut_and_exit",true).put("real_chain_buttons",true).put("independent_reading",true).put("ambient_conflict",true).put("relink",true).put("persisted",true).put("anchor_and_font_preserved",true)
        } catch(failure:Throwable){report.put("passed",false).put("error",failure.toString());screenshot("failure")}
        File(out,"result.json").writeText(report.toString(2))
        test.finish(if(report.optBoolean("passed"))Activity.RESULT_OK else Activity.RESULT_CANCELED,Bundle().apply{putString("stream",report.toString(2))})
    }
}
