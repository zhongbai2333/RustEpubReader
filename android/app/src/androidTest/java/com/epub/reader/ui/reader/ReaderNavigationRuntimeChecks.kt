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
import org.json.JSONArray

internal object ReaderNavigationRuntimeChecks {
    fun run(test: Instrumentation) {
        val context = test.targetContext
        check(context.packageName in setOf("local.reader.navigation", "local.reader.columns")) { "Use the isolated navigation package" }
        val report = JSONObject()
        val states = JSONArray()
        var activeActivity: MainActivity? = null
        var activeModel: ReaderViewModel? = null
        File(context.cacheDir,"navigation-failure.png").delete()
        val preferences = context.getSharedPreferences("reader_settings", 0)
        fun record(label: String) {
            val vm=activeModel
            val uri=preferences.getString("last_book_uri",null)
            val progress=JSONArray(RustBridge.getSyncedProgress(context.filesDir.path))
            val saved=(0 until progress.length()).map {progress.getJSONObject(it)}.firstOrNull {it.optString("path")==uri}
            states.put(JSONObject().put("event",label).put("activity_destroyed",activeActivity?.isDestroyed)
                .put("has_book",vm?.currentBook!=null).put("loading",vm?.isLoading)
                .put("chapter",vm?.currentChapter).put("block",vm?.currentBlock).put("offset",vm?.currentCharOffset)
                .put("error",vm?.errorMessage).put("saved_uri",uri)
                .put("saved_chapter",preferences.getInt("last_book_chapter",-1))
                .put("committed_chapter",saved?.optInt("chapter",-1))
                .put("committed_block",saved?.optInt("block",-1))
                .put("committed_offset",saved?.optInt("char_offset",-1)))
        }
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
            activeActivity=activity;activeModel=vm
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
            val committed = org.json.JSONArray(RustBridge.getSyncedProgress(context.filesDir.path))
            val notice = (0 until committed.length()).map { committed.getJSONObject(it) }
                .first { it.getString("path") == managedPath }
            check(notice.getInt("chapter") == 19 && notice.getInt("char_offset") == 37) {
                "Committed JNI progress changed the local scalar locator"
            }
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
            // Exercise the production JNI transport/commit/notification chain with
            // an app-owned second peer. No real account or external device is used.
            val peerRoot = File(context.cacheDir, "nearby-runtime-${java.util.UUID.randomUUID()}").also { it.mkdirs() }
            val remoteBook = JSONObject(checkNotNull(RustBridge.addOrUpdateBook(peerRoot.path,
                "Remote fixture", publication.path, 19, "第20章")))
            RustBridge.updatePosition(peerRoot.path, remoteBook.getString("uri"), 21, "第22章", 3, 3)
            val peerVersion = JSONObject(checkNotNull(RustBridge.getBookConfig(peerRoot.path, remoteBook.getString("id"))))
                .getJSONObject("position_version")
            try {
                val server = JSONObject(checkNotNull(RustBridge.startSharingServer(peerRoot.path,
                    File(peerRoot,"books").path, "1234")))
                check(server.optBoolean("ok")) { server.toString() }
                val address = "127.0.0.1:" + server.getString("addr").substringAfterLast(":")
                val synced = JSONObject(checkNotNull(RustBridge.syncProgressWithPeer(address, "1234", "",
                    context.filesDir.path, File(context.filesDir,"books").path)))
                check(synced.optBoolean("ok")) { synced.toString() }
                test.runOnMainSync { vm.refreshLibrary() }
                await("Committed peer progress did not automatically navigate") {
                    vm.currentChapter == 21 && vm.currentBlock == 3 && vm.currentCharOffset == 3
                }
            } finally {
                RustBridge.stopSharingServer()
                peerRoot.deleteRecursively()
            }
            val journal = File(context.filesDir, "github-sync-local.json")
            val remoteVersion = JSONObject(journal.readText()).getJSONObject("own").getJSONObject("progress_versions")
            check((0 until remoteVersion.names()!!.length()).any {
                val version = remoteVersion.getJSONObject(remoteVersion.names()!!.getString(it))
                version.getLong("modified_ms") == peerVersion.getLong("modified_ms") &&
                    version.getString("device") == peerVersion.getString("device") &&
                    version.optBoolean("legacy") == peerVersion.optBoolean("legacy")
            }) { "Receiving peer progress changed its original revision" }
            val remoteVersionText = remoteVersion.toString()
            check(vm.returnHistory.last().location.chapter == 20)
            record("before restart")
            val previousActivity=activity
            val previousModel=vm
            test.runOnMainSync { vm.persistReadingSession(); activity.finish() }
            await("Previous reader Activity did not finish destruction") {previousActivity.isDestroyed}
            record("previous Activity destroyed")
            val restarted = start(); activity = restarted.first; vm = restarted.second
            check(activity!==previousActivity && vm!==previousModel) {"Restart reused the destroyed reader"}
            await("Restart did not reopen reader") { vm.currentBook != null && !vm.isLoading && vm.currentChapter == 21 }
            record("reader restored")
            check(vm.currentBlock == 3 && vm.currentCharOffset == 3)
            check(vm.returnHistory.last().location.chapter == 20)
            check(remoteVersionText == JSONObject(journal.readText()).getJSONObject("own").optJSONObject("progress_versions").toString()) { "Remote position created local edit" }
            val paperTitle = vm.currentBook!!.chapters[vm.currentChapter].title
            check(paperTitle.isNotBlank())
            report.put("paper_title", paperTitle)
            val paperTitleKey = paperTitle.filterNot { it.isWhitespace() }
            await("Restored native paper is not ready for input") {
                var focused = false
                test.runOnMainSync { focused = activity.hasWindowFocus() && activity.window.decorView.isShown }
                focused && nodes().any { it.isVisibleToUser && it.text?.toString()?.filterNot { char -> char.isWhitespace() }?.contains(paperTitleKey) == true }
            }
            val viewport = android.graphics.Rect()
            test.runOnMainSync { check(activity.window.decorView.getGlobalVisibleRect(viewport)) }
            // The exact spine is a boundary between the controls and forward
            // turn zones. Use the interior of the controls zone in the actual
            // resumed window, including after an inset/orientation change.
            val controlX = viewport.left + viewport.width() * 0.4f
            val controlY = viewport.exactCenterY()
            report.put("control_tap", JSONObject().put("x", controlX).put("y", controlY)
                .put("window", viewport.toShortString()).put("paper_title", paperTitle)
                .put("display_width", context.resources.displayMetrics.widthPixels)
                .put("display_height", context.resources.displayMetrics.heightPixels))
            tap(controlX, controlY)
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
            await("Closed reader Activity did not finish destruction") {activity.isDestroyed}
            val closed = start().second
            Thread.sleep(700)
            check(closed.currentBook == null) { "Explicit close reopened reader" }
            report.put("passed",true).put("exact_startup_resume",true).put("cross_and_same_chapter_return",true).put("sequential_turn_has_no_history",true).put("remote_jump",true).put("encrypted_peer_progress",true).put("restart_history",true).put("explicit_close",true).put("long_press_location",true).put("round_button_click",true).put("continuous_reader",true)
        } catch (failure: Throwable) {
            report.put("passed",false).put("error",failure.stackTraceToString())
            runCatching {record("failure")}.onFailure {report.put("state_capture_error",it.toString())}
            runCatching {
                val hierarchy = JSONArray()
                nodes().take(200).forEach { node ->
                    val bounds = android.graphics.Rect(); node.getBoundsInScreen(bounds)
                    hierarchy.put(JSONObject().put("text",node.text?.toString()?.take(300))
                        .put("description",node.contentDescription?.toString()?.take(300))
                        .put("visible",node.isVisibleToUser).put("bounds",bounds.toShortString()))
                }
                report.put("failure_ui",hierarchy)
            }.onFailure {report.put("hierarchy_capture_error",it.toString())}
            runCatching {
                test.uiAutomation.takeScreenshot()?.let {image->
                    try {File(context.cacheDir,"navigation-failure.png").outputStream().use {image.compress(Bitmap.CompressFormat.PNG,100,it)}}
                    finally {image.recycle()}
                }
            }.onFailure {report.put("screenshot_error",it.toString())}
        }
        report.put("states",states)
        File(context.cacheDir,"navigation-verification.json").writeText(report.toString(2))
        test.finish(if(report.optBoolean("passed")) Activity.RESULT_OK else Activity.RESULT_CANCELED, Bundle().apply {putString("stream",report.toString(2))})
    }
}
