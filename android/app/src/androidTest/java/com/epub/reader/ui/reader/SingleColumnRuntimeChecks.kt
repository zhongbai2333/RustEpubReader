package com.zhongbai233.epub.reader.ui.reader

import android.app.Activity
import android.app.Instrumentation
import android.content.Intent
import android.graphics.Bitmap
import android.os.Bundle
import androidx.lifecycle.ViewModelProvider
import com.zhongbai233.epub.reader.MainActivity
import com.zhongbai233.epub.reader.RustBridge
import com.zhongbai233.epub.reader.model.ReaderLocation
import com.zhongbai233.epub.reader.viewmodel.ReaderViewModel
import java.io.File
import org.json.JSONObject

internal object SingleColumnRuntimeChecks {
    private fun verifyAnnotationTools(context:android.content.Context,publication:File) {
        val root=File(context.cacheDir,"annotation-check-${java.util.UUID.randomUUID()}").apply {mkdirs()}
        val entry=JSONObject(checkNotNull(RustBridge.addOrUpdateBook(root.path,"Annotations",publication.path,0,"")))
        val id=entry.getString("id");val path=entry.getString("uri")
        val blocks=JSONObject(checkNotNull(RustBridge.getChapter(path,0))).getJSONArray("blocks")
        val block=(0 until blocks.length()).first {index->blocks.getJSONObject(index).optString("type")=="paragraph"}
        val payload=JSONObject().put("chapter",0).put("start_block",block).put("end_block",block).put("start_offset",0).put("end_offset",4).put("color","Blue")
        val old=checkNotNull(RustBridge.addHighlight(root.path,id,payload.toString()))
        RustBridge.removeHighlight(root.path,id,old)
        val highlight=checkNotNull(RustBridge.addHighlight(root.path,id,payload.toString()))
        check(highlight!=old) {"Highlight identity reused after delete"}
        fun call(request:JSONObject):JSONObject {
            val envelope=JSONObject(checkNotNull(RustBridge.annotationTools(root.path,request.put("id",id).toString())))
            check(envelope.getBoolean("ok")) {envelope.optString("error")};return envelope.getJSONObject("result")
        }
        fun save(content:String,original:String?):JSONObject=call(JSONObject().put("action","note").put("highlight_id",highlight).put("content",content).put("original_note",original ?: JSONObject.NULL))
        save("Original",null);save("Remote-like edit","Original")
        val concurrent=save("Local draft","Original").getJSONArray("notes").getJSONObject(0).getString("content")
        check(concurrent.contains("Remote-like edit")&&concurrent.contains("Local draft"))
        val markers=JSONObject(checkNotNull(RustBridge.getBookConfig(root.path,id))).getJSONArray("concurrent_notes")
        check(markers.length()==1&&markers.getString(0)==highlight) {"Concurrent note marker missing"}
        val details=call(JSONObject().put("action","details").put("key","n:$highlight"))
        check(details.getBoolean("concurrent")&&details.getJSONArray("revisions").length()==2)
        check(save("Resolved",concurrent).getJSONArray("concurrent_notes").length()==0)
        val exported=call(JSONObject().put("action","export"))
        check(exported.getString("filename").endsWith(".md")&&exported.getString("markdown").contains("Resolved"))
        check(!exported.getString("markdown").contains("annotation-device"))
        val backup=File(context.cacheDir,"${java.util.UUID.randomUUID()}.readerbackup")
        val restored=File(context.cacheDir,"backup-restored-${java.util.UUID.randomUUID()}")
        fun backupCall(directory:File,request:JSONObject):JSONObject {
            val envelope=JSONObject(checkNotNull(RustBridge.backupTools(directory.path,request.toString())))
            check(envelope.getBoolean("ok")) {envelope.optString("error")};return envelope.getJSONObject("result")
        }
        try {
            backupCall(root,JSONObject().put("action","export").put("path",backup.path))
            val verified=backupCall(restored,JSONObject().put("action","inspect").put("path",backup.path))
            repeat(2) {
                backupCall(restored,JSONObject().put("action","restore").put("path",backup.path).put("expected_manifest",verified.getString("manifest_sha256")))
                val entries=org.json.JSONArray(checkNotNull(RustBridge.loadLibrary(restored.path)))
                check(entries.length()==1)
                val nextId=entries.getJSONObject(0).getString("id");check(nextId!=id)
                val cfg=JSONObject(checkNotNull(RustBridge.getBookConfig(restored.path,nextId)))
                check(cfg.getJSONArray("notes").getJSONObject(0).getString("content")=="Resolved")
            }
            val failure=JSONObject(checkNotNull(RustBridge.backupTools(restored.path,JSONObject().put("action","restore").put("path",backup.path).put("expected_manifest","0".repeat(64)).toString())))
            check(!failure.getBoolean("ok")) {"Backup fingerprint mismatch was accepted"}
        } finally {backup.delete();restored.deleteRecursively()}
        check(save("","Resolved").getJSONArray("notes").length()==0)
        RustBridge.removeHighlight(root.path,id,highlight)
        check(JSONObject(checkNotNull(RustBridge.getBookConfig(root.path,id))).optJSONArray("highlights")==null)
    }
    fun run(test: Instrumentation) {
        val context = test.targetContext
        check(context.packageName == "local.reader.columns") { "Use the isolated columns package" }
        val prefs = context.getSharedPreferences("reader_settings", 0)
        val report = JSONObject()
        fun await(message: String, ready: () -> Boolean) {
            repeat(200) { test.waitForIdleSync(); if (ready()) return; Thread.sleep(100) }
            error(message)
        }
        fun start(): Pair<MainActivity, ReaderViewModel> {
            val activity = test.startActivitySync(Intent(context, MainActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK)) as MainActivity
            lateinit var vm: ReaderViewModel
            test.runOnMainSync { vm = ViewModelProvider(activity)[ReaderViewModel::class.java] }
            return activity to vm
        }
        fun screenshot(name: String) {
            test.waitForIdleSync(); Thread.sleep(300)
            checkNotNull(test.uiAutomation.takeScreenshot()).let { image ->
                File(context.cacheDir, name).outputStream().use { image.compress(Bitmap.CompressFormat.PNG, 100, it) }
                image.recycle()
            }
        }
        try {
            // Exercise the real JNI exception boundary using disposable data only.
            val damaged = File(context.cacheDir, "storage-check-${java.util.UUID.randomUUID()}").apply { mkdirs() }
            val catalog = File(damaged, "library.json").apply { writeText("damaged") }
            val failure = runCatching { RustBridge.loadLibrary(damaged.path) }.exceptionOrNull()
            check(failure is java.io.IOException && catalog.readText() == "damaged")
            File(damaged, "library.json.bak").writeText("{\"books\":[]}")
            check(RustBridge.loadLibrary(damaged.path) == "[]")
            check(org.json.JSONObject(catalog.readText()).getJSONArray("books").length() == 0)
            check(damaged.listFiles()!!.any { it.name.contains(".corrupt.") && it.readText() == "damaged" })
            report.put("storage_failure_propagation", true).put("storage_backup_recovery", true)
            prefs.edit().clear().putBoolean("reader.introduction.v1", true).putString("reader_language", "zh_cn").commit()
            val (activity, vm) = start()
            check(!vm.forceSingleColumn)
            check(context.resources.configuration.screenWidthDp > 600) { "Run on a tablet-sized emulator" }
            val publication = File(context.cacheDir, "columns.epub")
            check(publication.isFile)
            verifyAnnotationTools(context,publication)
            report.put("backup_tools",true)
            report.put("annotation_tools",true).put("stale_note_preserved",true).put("markdown_export",true)
            val opening = RustBridge.beginBookOpen()
            val cancelStart = android.os.SystemClock.elapsedRealtimeNanos()
            RustBridge.cancelBookOpen(opening)
            val cancelMs = (android.os.SystemClock.elapsedRealtimeNanos()-cancelStart)/1_000_000.0
            val rejected = runCatching { RustBridge.openBookCancellable(publication.path, opening, 0) }.exceptionOrNull()
            check(rejected is java.io.IOException && rejected.message == "Book opening cancelled")
            RustBridge.finishBookOpen(opening)
            check(runCatching { RustBridge.openBookCancellable(publication.path, opening, 0) }.exceptionOrNull() is java.io.IOException)
            checkNotNull(RustBridge.openBook(publication.path)) // A cancelled scope cannot poison its cached book.
            report.put("opening_cancel_request_ms", cancelMs).put("cancel_before_dispatch", true).put("expired_open_rejected", true)
            test.runOnMainSync { vm.openFromPath(publication.path) }
            await("Book did not open") { vm.currentBook != null && !vm.isLoading }
            Thread.sleep(1800)
            val origin = ReaderLocation(vm.currentChapter, vm.currentBlock, vm.currentCharOffset)
            screenshot("android-auto-columns.png")
            test.runOnMainSync { vm.updateForceSingleColumn(true) }
            Thread.sleep(1200)
            check(vm.forceSingleColumn && prefs.getBoolean("force_single_column", false))
            check(ReaderLocation(vm.currentChapter, vm.currentBlock, vm.currentCharOffset) == origin) { "Column reflow changed reading position" }
            screenshot("android-single-column.png")
            for (mode in listOf("Slide", "Cover", "None", "Realistic")) {
                test.runOnMainSync { vm.updateReaderPageAnimation(mode) }
                Thread.sleep(300)
                check(vm.forceSingleColumn)
            }
            test.runOnMainSync { vm.updateForceSingleColumn(false) }
            check(!prefs.getBoolean("force_single_column", true))
            test.runOnMainSync { vm.updateForceSingleColumn(true); vm.closeBook(); activity.finish() }
            val (restarted, restored) = start()
            check(restored.forceSingleColumn) { "Preference did not survive recreation" }
            test.runOnMainSync { restored.updateForceSingleColumn(false); restarted.finish() }
            report.put("passed", true).put("default_auto", true).put("precise_reflow", true)
                .put("preference_recreation", true).put("auto_restored", true).put("all_page_animations", true)
        } catch (error: Throwable) { report.put("passed", false).put("error", error.stackTraceToString()) }
        File(context.cacheDir, "single-column-verification.json").writeText(report.toString(2))
        test.finish(if (report.optBoolean("passed")) Activity.RESULT_OK else Activity.RESULT_CANCELED,
            Bundle().apply { putString("stream", report.toString(2)) })
    }
}
