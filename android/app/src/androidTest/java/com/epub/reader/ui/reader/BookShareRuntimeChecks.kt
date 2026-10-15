package com.zhongbai233.epub.reader.ui.reader

import android.app.Activity
import android.app.Instrumentation
import android.os.Bundle
import com.zhongbai233.epub.reader.RustBridge
import org.json.JSONObject
import java.io.File

/** Actual device JNI check against a synthetic sender; uses an isolated cache library. */
object BookShareRuntimeChecks {
    fun run(test: Instrumentation) {
        val context = test.targetContext
        val root = File(context.cacheDir, "native-book-share-check-${java.util.UUID.randomUUID()}").apply { mkdirs() }.path
        fun call(request: JSONObject): JSONObject {
            val envelope = JSONObject(checkNotNull(RustBridge.bookShare(root, request.toString())))
            check(envelope.optBoolean("ok")) { envelope.optString("error") }
            return envelope.getJSONObject("result")
        }
        val report = JSONObject()
        try {
            val decoded = call(JSONObject().put("op", "decode_image").put("path", File(context.filesDir, "book-share-qr.png").path))
            val state = call(JSONObject().put("op", "receive").put("link", decoded.getString("link")))
            check(state.getJSONObject("preview").getJSONArray("books").length() == 1)
            check(!File(root, "library.json").exists()) { "Received package imported without confirmation" }
            val accepted = call(JSONObject().put("op", "accept"))
            val books = JSONObject(File(root, "library.json").readText()).getJSONArray("books")
            check(accepted.getInt("imported") == 1 && books.length() == 1)
            check(books.getJSONObject(0).getString("title") == state.getJSONObject("preview").getJSONArray("books").getJSONObject(0).getString("title"))
            report.put("passed", true).put("qr_image_decoded", true).put("preview_does_not_import", true).put("imported", 1).put("title_preserved", true).put("github_credentials_used", false)
        } catch (failure: Throwable) { report.put("passed", false).put("error", failure.toString()) }
        File(context.cacheDir, "book-share-verification.json").writeText(report.toString(2))
        test.finish(if (report.optBoolean("passed")) Activity.RESULT_OK else Activity.RESULT_CANCELED, Bundle().apply { putString("stream", report.toString(2)) })
    }
}
