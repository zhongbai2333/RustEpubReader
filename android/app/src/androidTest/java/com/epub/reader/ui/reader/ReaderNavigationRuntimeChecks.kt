package com.zhongbai233.epub.reader.ui.reader

import android.app.Activity
import android.app.Instrumentation
import android.content.Intent
import android.graphics.Bitmap
import android.os.Bundle
import android.os.SystemClock
import android.view.MotionEvent
import android.view.InputDevice
import android.view.accessibility.AccessibilityNodeInfo
import com.zhongbai233.epub.reader.i18n.I18n
import androidx.lifecycle.ViewModelProvider
import com.zhongbai233.epub.reader.MainActivity
import com.zhongbai233.epub.reader.RustBridge
import com.zhongbai233.epub.reader.model.*
import com.zhongbai233.epub.reader.viewmodel.ReaderViewModel
import java.io.File
import org.json.JSONObject

internal object ReaderNavigationRuntimeChecks {
    fun run(test: Instrumentation) {
        val context = test.targetContext
        check(context.packageName == "local.reader.navigation") { "Use the isolated navigation package" }
        val report = JSONObject()
        val preferences = context.getSharedPreferences("reader_settings", 0)
        fun await(message: String, condition: () -> Boolean) {
            repeat(200) { test.waitForIdleSync(); if(condition()) return; Thread.sleep(50) }; error(message)
        }
        fun nodes(): List<AccessibilityNodeInfo> {
            val found = mutableListOf<AccessibilityNodeInfo>()
            fun visit(node: AccessibilityNodeInfo?) { if (node != null) { found += node; for (index in 0 until node.childCount) visit(node.getChild(index)) } }
            visit(test.uiAutomation.rootInActiveWindow); return found
        }
        fun tap(x: Float, y: Float, hold: Long = 30) {
            val began = SystemClock.uptimeMillis()
            fun event(action: Int) { MotionEvent.obtain(began,SystemClock.uptimeMillis(),action,x,y,0).also { it.source=InputDevice.SOURCE_TOUCHSCREEN;test.uiAutomation.injectInputEvent(it,true);it.recycle() } }
            event(MotionEvent.ACTION_DOWN);Thread.sleep(hold);event(MotionEvent.ACTION_UP)
        }
        fun start(): Pair<MainActivity, ReaderViewModel> {
            val activity = test.startActivitySync(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK)) as MainActivity
            lateinit var vm: ReaderViewModel
            test.runOnMainSync { vm = ViewModelProvider(activity)[ReaderViewModel::class.java] }
            return activity to vm
        }
        try {
            val publication = File(context.cacheDir, "navigation.epub")
            check(publication.isFile) { "Supply cache/navigation.epub" }
            preferences.edit().clear().putBoolean("reader.introduction.v1", true).putString("reader_language", "zh_cn").commit()
            val entry = JSONObject(checkNotNull(RustBridge.addOrUpdateBook(context.filesDir.path, "原生阅读跳转验证", publication.path, 19, "第20章")))
            val managedPath = entry.getString("uri")
            RustBridge.updatePosition(context.filesDir.path, managedPath, 19, "第20章", 3, 37)
            preferences.edit().putString("last_book_uri", managedPath).putInt("last_book_chapter", 19).commit()
            var (activity, vm) = start()
            await("Initial auto-resume failed") { vm.currentBook != null && !vm.isLoading && vm.currentChapter == 19 }
            Thread.sleep(700)
            val origin = ReaderLocation(vm.currentChapter, vm.currentBlock, vm.currentCharOffset)
            check(origin == ReaderLocation(19,3,37)) { "Initial exact position lost: $origin" }
            test.runOnMainSync { vm.goToPosition(21,3,3) }
            await("Cross-chapter jump failed") { vm.currentChapter == 21 && vm.currentBlock == 3 && vm.currentCharOffset == 3 }
            Thread.sleep(500)
            check(vm.returnHistory.last().location == origin)
            test.uiAutomation.takeScreenshot()?.let { image -> File(context.cacheDir,"navigation-expanded.png").outputStream().use {image.compress(Bitmap.CompressFormat.PNG,100,it)}; image.recycle() }
            test.runOnMainSync { vm.goToPosition(21,3,80) }
            await("Same-chapter jump failed") { vm.currentCharOffset == 80 }
            test.runOnMainSync { vm.goBackChapter() }
            await("Same-chapter return failed") { vm.currentChapter == 21 && vm.currentCharOffset == 3 }
            test.runOnMainSync { vm.goBackChapter() }
            await("Cross-chapter return failed") { vm.currentChapter == 19 && vm.currentCharOffset == 37 }
            check(vm.returnHistory.isEmpty())
            test.runOnMainSync { vm.goToChapter(20, remember = false) }
            await("Sequential chapter failed") { vm.currentChapter == 20 }
            check(vm.returnHistory.isEmpty())
            // A remote position has already been accepted by sync. Applying it must not mint a local revision.
            RustBridge.applyRemotePosition(context.filesDir.path, managedPath, 21, "第22章", 3, 3)
            val journal = File(context.filesDir, "github-sync-local.json")
            val remoteVersion = JSONObject(journal.readText()).getJSONObject("own").optJSONObject("progress_versions").toString()
            test.runOnMainSync { vm.refreshLibrary() }
            Thread.sleep(500)
            test.runOnMainSync { vm.goToPosition(21,3,3,remote=true,scalarOffset=true) }
            await("Remote jump failed") { vm.currentChapter == 21 && vm.currentCharOffset == 3 }
            check(vm.returnHistory.last().location.chapter == 20)
            test.runOnMainSync { vm.persistReadingSession(); activity.finish() }
            Thread.sleep(300)
            val restarted = start(); activity = restarted.first; vm = restarted.second
            await("Restart did not reopen reader") { vm.currentBook != null && !vm.isLoading && vm.currentChapter == 21 }
            check(vm.currentBlock == 3 && vm.currentCharOffset == 3)
            check(vm.returnHistory.last().location.chapter == 20)
            check(remoteVersion == JSONObject(journal.readText()).getJSONObject("own").optJSONObject("progress_versions").toString()) { "Remote position created local edit" }
            Thread.sleep(1200)
            tap(context.resources.displayMetrics.widthPixels / 2f, context.resources.displayMetrics.heightPixels / 2f)
            val returnLabel = I18n.t("reader.return_position")
            await("Return button not reachable") { nodes().any { it.contentDescription?.toString() == returnLabel } }
            Thread.sleep(250)
            test.uiAutomation.takeScreenshot()?.let { image -> File(context.cacheDir,"navigation-collapsed.png").outputStream().use {image.compress(Bitmap.CompressFormat.PNG,100,it)}; image.recycle() }
            val button = nodes().first { it.contentDescription?.toString() == returnLabel }
            val bounds = android.graphics.Rect(); button.getBoundsInScreen(bounds)
            val expectedTitle = vm.returnHistory.last().chapterTitle
            tap(bounds.exactCenterX(), bounds.exactCenterY(), 750)
            await("Long press did not reveal source location") { nodes().any { it.text?.toString()?.contains(expectedTitle) == true } }
            check(vm.currentChapter == 21) { "Long press unexpectedly navigated" }
            test.uiAutomation.takeScreenshot()?.let { image -> File(context.cacheDir,"navigation-tooltip.png").outputStream().use {image.compress(Bitmap.CompressFormat.PNG,100,it)}; image.recycle() }
            Thread.sleep(1600)
            tap(bounds.exactCenterX(), bounds.exactCenterY())
            await("Return button click did not navigate") { vm.currentChapter == 20 }
            test.runOnMainSync { vm.updateScrollMode(true) }
            Thread.sleep(500)
            test.runOnMainSync { vm.goToPosition(19,3,37) }
            await("Continuous reader jump failed") { vm.currentChapter == 19 && vm.currentBlock == 3 && vm.currentCharOffset == 37 }
            Thread.sleep(650)
            test.runOnMainSync { vm.goBackChapter() }
            await("Continuous reader return failed") { vm.currentChapter == 20 }
            test.runOnMainSync { vm.updateScrollMode(false); vm.closeBook(); activity.finish() }
            val closed = start().second
            Thread.sleep(700)
            check(closed.currentBook == null) { "Explicit close reopened reader" }
            report.put("passed",true).put("exact_startup_resume",true).put("cross_and_same_chapter_return",true).put("sequential_turn_has_no_history",true).put("remote_jump",true).put("restart_history",true).put("explicit_close",true).put("long_press_location",true).put("round_button_click",true).put("continuous_reader",true)
        } catch (failure: Throwable) { report.put("passed",false).put("error",failure.stackTraceToString()) }
        File(context.cacheDir,"navigation-verification.json").writeText(report.toString(2))
        test.finish(if(report.optBoolean("passed")) Activity.RESULT_OK else Activity.RESULT_CANCELED, Bundle().apply {putString("stream",report.toString(2))})
    }
}
