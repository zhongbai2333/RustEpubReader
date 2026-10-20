package com.zhongbai233.epub.reader.ui.reader

import android.app.Activity
import android.app.Instrumentation
import android.accessibilityservice.AccessibilityService
import android.content.Intent
import android.graphics.Bitmap
import android.os.Bundle
import android.os.SystemClock
import android.view.InputDevice
import android.view.MotionEvent
import androidx.lifecycle.ViewModelProvider
import com.zhongbai233.epub.reader.MainActivity
import com.zhongbai233.epub.reader.RustBridge
import com.zhongbai233.epub.reader.viewmodel.ReaderViewModel
import java.io.File
import org.json.JSONObject

/** Actual shelf input, system Back, cancellation, late completions and reopening. */
internal object BookOpeningRuntimeChecks {
    fun run(test: Instrumentation) {
        val context = test.targetContext
        check(context.packageName == "local.reader.columns") { "Use the isolated diagnostic application" }
        val output = File(context.cacheDir, "book-opening-runtime").also { check(it.mkdirs()) }
        val report = JSONObject()
        var activity: MainActivity? = null
        try {
            val source = File(context.cacheDir, "book-opening.epub")
            check(source.isFile) { "Supply an owned cache/book-opening.epub fixture" }
            val proposedTitle = "开书取消验证 ${java.util.UUID.randomUUID().toString().take(8)}"
            val entry = JSONObject(checkNotNull(RustBridge.addOrUpdateBook(context.filesDir.path, proposedTitle, source.path, 0, "")))
            // Import de-duplicates by content and preserves an existing title.
            val path = entry.getString("uri")
            context.getSharedPreferences("reader_settings", 0).edit().clear()
                .putBoolean("reader.introduction.v1", true).putString("reader_language", "zh_cn").commit()
            activity = test.startActivitySync(Intent(context, MainActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK)) as MainActivity
            lateinit var vm: ReaderViewModel
            test.runOnMainSync { vm = ViewModelProvider(activity)[ReaderViewModel::class.java] }
            val covers = java.util.concurrent.ConcurrentHashMap<String, androidx.compose.ui.geometry.Rect>()
            test.runOnMainSync {
                vm.libraryRenderObserver = object : com.zhongbai233.epub.reader.ui.library.LibraryRenderObserver {
                    override fun coverBounds(uri: String, bounds: androidx.compose.ui.geometry.Rect) {
                        if (bounds.width > 0 && bounds.height > 0) covers[uri] = bounds else covers.remove(uri)
                    }
                    override fun coverRequested(uri: String) {}
                }
                vm.refreshLibrary()
                activity.window.decorView.requestLayout()
            }
            fun await(message: String, condition: () -> Boolean) {
                val deadline = SystemClock.elapsedRealtime() + 20_000
                while (SystemClock.elapsedRealtime() < deadline) {
                    test.waitForIdleSync()
                    if (condition()) return
                    Thread.sleep(20)
                }
                error(message)
            }
            fun tapCover(): Boolean {
                // Use the existing read-only layout observer: titles can change
                // after metadata loading and can sit behind a tablet's bottom
                // floating controls. The visible cover remains the input target.
                if (test.uiAutomation.rootInActiveWindow?.packageName?.toString() != context.packageName || activity?.hasWindowFocus() != true) return false
                val rect = covers[path] ?: return false
                if (rect.width <= 0 || rect.height <= 0) return false
                val began = SystemClock.uptimeMillis()
                for (action in listOf(MotionEvent.ACTION_DOWN, MotionEvent.ACTION_UP)) {
                    val event = MotionEvent.obtain(began, SystemClock.uptimeMillis(), action, rect.center.x, rect.center.y, 0)
                    event.source = InputDevice.SOURCE_TOUCHSCREEN
                    test.uiAutomation.injectInputEvent(event, true); event.recycle()
                }
                return true
            }
            fun coverReady(): Boolean = covers[path]?.let { it.width > 0 && it.height > 0 } == true &&
                activity?.hasWindowFocus() == true && test.uiAutomation.rootInActiveWindow?.packageName?.toString() == context.packageName
            fun systemBack(): Long {
                // Core readiness can precede Compose's callback registration on
                // a cold software-rendered CI device. Wait for the real input
                // receiver, then use the OS Back action rather than a key pair.
                await("Reader did not register a focused system Back handler") {
                    var ready = false
                    test.runOnMainSync {
                        ready = activity?.hasWindowFocus() == true && activity?.onBackPressedDispatcher?.hasEnabledCallbacks() == true
                    }
                    ready && test.uiAutomation.rootInActiveWindow?.packageName?.toString() == context.packageName
                }
                if (!report.has("reader_ready_at_back")) {
                    report.put("reader_ready_at_back", vm.currentBook != null && !vm.isLoading)
                        .put("loading_at_back", vm.isLoading)
                }
                val began = SystemClock.elapsedRealtimeNanos()
                check(test.uiAutomation.performGlobalAction(AccessibilityService.GLOBAL_ACTION_BACK)) { "System Back input was rejected" }
                return began
            }
            fun screenshot(name: String, required: Boolean = true) {
                var image = test.uiAutomation.takeScreenshot()
                if (required) repeat(10) { if (image == null) { Thread.sleep(60); image = test.uiAutomation.takeScreenshot() } }
                if (image == null) { check(!required) { "No screenshot: $name" }; return }
                checkNotNull(image).let { bitmap ->
                    File(output, "$name.png").outputStream().use { check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)) }
                    bitmap.recycle()
                }
            }
            await("Owned shelf cover not visible or focused", ::coverReady)
            screenshot("shelf")
            check(tapCover())
            await("Shelf touch did not enter the reader; check foreground system dialogs") { vm.isLoading || vm.currentBook != null }
            Thread.sleep(140)
            screenshot("before-back", required = false)
            val cancelledAt = systemBack()
            await("Back did not release the reader") { vm.currentBook == null && !vm.isLoading }
            report.put("back_to_cleared_session_ms", (SystemClock.elapsedRealtimeNanos() - cancelledAt) / 1e6)
            // An old worker must not republish after returning to the shelf.
            Thread.sleep(900); test.waitForIdleSync()
            screenshot("returned-shelf")
            check(vm.currentBook == null && !vm.isLoading && vm.errorMessage == null) {
                "After cancellation: book=${vm.currentBook?.title}, loading=${vm.isLoading}, error=${vm.errorMessage}"
            }
            await("Returned shelf cover not visible or focused", ::coverReady)
            val reopenedAt = SystemClock.elapsedRealtimeNanos()
            check(tapCover())
            await("Shelf input did not reopen the same book") { vm.currentBook != null && !vm.isLoading && vm.originalBookPath == path }
            report.put("reopen_to_core_ready_ms", (SystemClock.elapsedRealtimeNanos() - reopenedAt) / 1e6)
            Thread.sleep(900)
            check(vm.originalBookPath == path && vm.errorMessage == null)
            screenshot("reopened")
            systemBack()
            await("Closing the reopened reader failed") { vm.currentBook == null && !vm.isLoading }
            report.put("passed", true).put("shelf_click", true).put("system_back", true)
                .put("late_completion_observed_ms", 900).put("reopen_exact_book", true)
                .put("physical_device", false).put("inflight_core_cancellation_proven", false)
        } catch (error: Throwable) { report.put("passed", false).put("error", error.stackTraceToString()) }
        finally { activity?.let { test.runOnMainSync { it.finish() } } }
        File(output, "summary.json").writeText(report.toString(2))
        test.finish(if (report.optBoolean("passed")) Activity.RESULT_OK else Activity.RESULT_CANCELED,
            Bundle().apply { putString("stream", report.toString(2)) })
    }
}
