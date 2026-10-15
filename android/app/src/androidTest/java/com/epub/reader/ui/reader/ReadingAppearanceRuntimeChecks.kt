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

/** Drives the production activity, settings and editor in a diagnostic application ID. */
object ReadingAppearanceRuntimeChecks {
    fun run(test: Instrumentation, checkAmbient: Boolean = false) {
        val context = test.targetContext
        val report = JSONObject()
        val out = File(context.cacheDir, "appearance-checks").apply { mkdirs() }
        val ambientStage = File(out,"ambient-stage.txt")
        if(checkAmbient) ambientStage.writeText("starting")
        fun tree(): List<AccessibilityNodeInfo> {
            val nodes = mutableListOf<AccessibilityNodeInfo>()
            fun visit(node: AccessibilityNodeInfo?) { if(node == null) return; nodes.add(node); for(i in 0 until node.childCount) visit(node.getChild(i)) }
            visit(test.uiAutomation.rootInActiveWindow); return nodes
        }
        fun await(message: String, predicate: () -> Boolean) { repeat(150) { if(predicate()) return; Thread.sleep(100) }; error(message) }
        fun click(label: String) {
            var found = false
            repeat(10) {
                if(found) return@repeat
                val match = tree().firstOrNull { it.text?.toString() == label || it.contentDescription?.toString() == label }
                var node = match
                while(node != null && !node.isClickable) node = node.parent
                if(node?.performAction(AccessibilityNodeInfo.ACTION_CLICK) == true) { found = true; test.waitForIdleSync(); Thread.sleep(350) }
                else {
                    val direction = if(label == I18n.t("appearance.edit_dark") || label == I18n.t("appearance.edit_light")) AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD else AccessibilityNodeInfo.ACTION_SCROLL_FORWARD
                    tree().filter { it.isScrollable }.maxByOrNull { node -> android.graphics.Rect().also { node.getBoundsInScreen(it) }.height() }
                        ?.performAction(direction)
                    Thread.sleep(200)
                }
            }
            check(found) { "Missing action: $label; tree=" + tree().map { "${it.text}/${it.contentDescription}" } }
        }
        fun screenshot(name: String) { Thread.sleep(800); test.uiAutomation.takeScreenshot()?.let { image -> File(out,name+".png").outputStream().use { image.compress(Bitmap.CompressFormat.PNG,100,it) }; image.recycle() } }
        try {
            check(context.packageName == "local.reader.appearance") { "Use the isolated diagnostic application ID" }
            val activity = test.startActivitySync(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK)) as MainActivity
            lateinit var vm: ReaderViewModel
            val fixture = File(context.filesDir, "appearance.epub")
            test.context.assets.open("original-layout.epub").use { input -> fixture.outputStream().use { input.copyTo(it) } }
            test.runOnMainSync {
                vm = ViewModelProvider(activity)[ReaderViewModel::class.java]
                vm.updateLanguage("zh_cn"); vm.updateReadingAppearance(ReadingAppearance()); vm.updateAppearanceLinkage(AppearanceLinkage(readingLinked=false,readingMode=ReadingDisplayMode.light)); vm.openFromPath(fixture.path)
            }
            await("Book did not open") { vm.currentBook != null && !vm.isLoading }
            test.waitForIdleSync(); Thread.sleep(800)
            val anchor = vm.currentChapter to vm.currentBlock
            val size = vm.fontSize
            test.runOnMainSync { vm.updateReadingAppearance(vm.readingAppearance.copy(selected = "gray")); vm.updateAppearanceLinkage(vm.appearanceLinkage.copy(readingLinked=false,readingMode=ReadingDisplayMode.dark)) }
            test.waitForIdleSync(); Thread.sleep(500); screenshot("reader-dark")
            check(vm.currentChapter to vm.currentBlock == anchor && vm.fontSize == size)
            test.runOnMainSync { vm.updateAppearanceLinkage(vm.appearanceLinkage.copy(readingLinked=false,readingMode=ReadingDisplayMode.light)) }
            test.waitForIdleSync(); Thread.sleep(500)
            // A reader starts with its chrome hidden; tap an empty part of the page.
            val metrics = context.resources.displayMetrics
            val now = android.os.SystemClock.uptimeMillis()
            for(action in listOf(android.view.MotionEvent.ACTION_DOWN, android.view.MotionEvent.ACTION_UP)) {
                val event = android.view.MotionEvent.obtain(now, now + 30, action, metrics.widthPixels * .5f, metrics.heightPixels * .7f, 0)
                test.sendPointerSync(event); event.recycle()
            }
            test.waitForIdleSync(); Thread.sleep(300)
            click(I18n.t("reader.menu")); click(I18n.t("settings.appearance_title")); screenshot("settings")
            click(I18n.t("appearance.adjust")); screenshot("editor-light")
            val imageFixture = File(context.filesDir, "appearance-background.png")
            val baselineImages = File(context.filesDir,"reader-appearance").listFiles().orEmpty().map { it.name }.toSet()
            Bitmap.createBitmap(120,180,Bitmap.Config.ARGB_8888).let { bitmap ->
                bitmap.eraseColor(0xFF507860.toInt()); imageFixture.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG,100,it) }; bitmap.recycle()
            }
            // Deliver a document-picker result through the real ActivityResult contract.
            val filter = android.content.IntentFilter(Intent.ACTION_OPEN_DOCUMENT).apply { addCategory(Intent.CATEGORY_OPENABLE); addCategory(Intent.CATEGORY_DEFAULT); addDataType("*/*") }
            val picker = test.addMonitor(filter, Instrumentation.ActivityResult(Activity.RESULT_OK, Intent().setData(android.net.Uri.fromFile(imageFixture))), true)
            click(I18n.t("common.select_from_files"))
            await("Background import did not finish") { tree().any { it.text?.toString() == I18n.t("appearance.strength") } }
            screenshot("editor-light-image")
            click(I18n.t("appearance.copy_dark")); click(I18n.t("appearance.edit_dark")); screenshot("editor-dark-image")
            click(I18n.t("common.remove_image"))
            click(I18n.t("common.cancel")); check(vm.readingAppearance.custom == null)
            check(File(context.filesDir,"reader-appearance").listFiles().orEmpty().map { it.name }.toSet() == baselineImages) { "Cancelled draft left orphan images" }
            click(I18n.t("appearance.adjust")); click(I18n.t("common.select_from_files"))
            await("Second background import did not finish") { tree().any { it.text?.toString() == I18n.t("appearance.strength") } }
            click(I18n.t("appearance.copy_dark")); click(I18n.t("common.save"))
            test.removeMonitor(picker)
            check(vm.readingAppearance.selected == "gray" && vm.readingAppearance.overrides["gray"] != null && vm.readingAppearance.custom == null)
            val persisted = context.getSharedPreferences("reader_settings",0).getString(ReadingAppearance.storageKey,null)
            check(ReadingAppearance.decode(persisted) == vm.readingAppearance)
            val savedImage = File(android.net.Uri.parse(vm.readingAppearance.style.light.image).path!!)
            check(savedImage.exists() && vm.readingAppearance.style.light.image == vm.readingAppearance.style.dark.image)
            click(I18n.t("appearance.adjust")); click(I18n.t("common.remove_image")); click(I18n.t("common.cancel"))
            check(savedImage.exists() && vm.readingAppearance.encoded() == persisted) { "Cancel removed a saved background" }
            check(vm.fontSize == size && vm.currentChapter to vm.currentBlock == anchor)
            screenshot("settings-saved")
            report.put("passed",true).put("production_activity",true).put("cancel_discards",true).put("save_persisted",true).put("mode_preserves_anchor_and_font",true).put("document_picker_import",true).put("cancel_cleans_new_images",true).put("cancel_preserves_saved_image",true).put("paired_image_saved",true)
            if(checkAmbient) {
                repeat(3) { tree().filter { it.isScrollable }.maxByOrNull { node -> android.graphics.Rect().also { node.getBoundsInScreen(it) }.height() }?.performAction(AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD); Thread.sleep(100) }
                test.runOnMainSync { vm.updateAppearanceLinkage(vm.appearanceLinkage.copy(readingLinked=false,readingMode=ReadingDisplayMode.automatic,readingSource=AutomaticAppearanceSource.ambient)) }
                ambientStage.writeText("dark")
                val darkStart = android.os.SystemClock.elapsedRealtime()
                await("Ambient dark transition did not arrive") { vm.readingDark }
                val darkWait = android.os.SystemClock.elapsedRealtime() - darkStart
                screenshot("settings-auto-dark")
                ambientStage.writeText("light")
                val lightStart = android.os.SystemClock.elapsedRealtime()
                await("Ambient light transition did not arrive") { !vm.readingDark }
                val lightWait = android.os.SystemClock.elapsedRealtime() - lightStart
                screenshot("settings-auto-light")
                report.put("ambient_native_transitions",true).put("dark_wait_ms",darkWait).put("light_wait_ms",lightWait)
                ambientStage.writeText("done")
            }
        } catch(failure: Throwable) { report.put("passed",false).put("error",failure.toString()); screenshot("failure") }
        File(out,"result.json").writeText(report.toString(2))
        test.finish(if(report.optBoolean("passed")) Activity.RESULT_OK else Activity.RESULT_CANCELED, Bundle().apply { putString("stream",report.toString(2)) })
    }
}
