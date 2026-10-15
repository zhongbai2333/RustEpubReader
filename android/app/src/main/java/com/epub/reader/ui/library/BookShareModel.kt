package com.zhongbai233.epub.reader.ui.library

import android.content.Context
import android.net.Uri
import androidx.compose.runtime.*
import com.zhongbai233.epub.reader.RustBridge
import kotlinx.coroutines.*
import org.json.JSONObject
import java.io.File

/** Network work stays off the UI thread; job IDs fence cancelled/late operations. */
class BookShareModel(private val root: String, private val beforeImport: () -> Unit = {}, private val imported: () -> Unit) {
    var state by mutableStateOf(JSONObject()); private set
    var busy by mutableStateOf(false); private set
    var error by mutableStateOf<String?>(null); private set
    var result by mutableStateOf<Pair<Int, Int>?>(null); private set
    var link by mutableStateOf("")
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var intent = 0L
    private var jobId: String? = null
    private var currentOperation = ""
    val hasGitHubStatus: Boolean get() = currentOperation in listOf("publish", "delete_share")
    fun cancelAuthenticatedWork() { if (busy && currentOperation in listOf("publish", "delete_share")) cancel() }
    val phase: String get() = state.optJSONObject("progress")?.optString("phase", "idle") ?: "idle"
    val activeLan: Boolean get() = state.optJSONObject("outgoing")?.optString("kind") == "lan" && phase in listOf("waiting", "transferring")
    private fun call(request: JSONObject): JSONObject {
        val envelope = JSONObject(RustBridge.bookShare(root, request.toString()) ?: error("Reader core returned no result"))
        check(envelope.optBoolean("ok")) { envelope.optString("error") }
        return envelope.getJSONObject("result")
    }
    fun command(op: String, fields: JSONObject = JSONObject(), removeFile: File? = null) {
        if (busy) return
        if (op == "accept") beforeImport()
        val request = fields.put("op", op)
        currentOperation = op
        if (op in listOf("start_lan", "receive", "publish", "delete_share")) {
            try { jobId = call(JSONObject().put("op", "arm").put("operation", op)).getString("job_id"); request.put("job_id", jobId)
                state = JSONObject().put("job_id", jobId).put("history", state.optJSONArray("history") ?: org.json.JSONArray()).put("progress", JSONObject().put("phase", when(op) { "receive" -> "receiving"; "delete_share" -> "deleting"; else -> "preparing" }).put("done", 0).put("total", 0))
            }
            catch (failure: Exception) { error = failure.message; return }
        }
        val generation = ++intent
        busy = true; error = null; result = null
        if (op == "accept") state = JSONObject(state.toString()).also { it.optJSONObject("progress")?.put("phase", "importing") }
        scope.launch {
            try {
                val value = withContext(Dispatchers.IO) { call(request) }
                if (generation != intent) return@launch
                val snapshot = withContext(Dispatchers.IO) { call(JSONObject().put("op", "state")) }
                // Cancel/reset may run while the final state request is in flight.
                if (generation != intent) return@launch
                state = snapshot
                jobId = state.optString("job_id")
                busy = false
                if (op == "accept") { result = value.optInt("imported") to value.optInt("skipped"); imported() }
                if (op == "decode_image") { link = value.getString("link"); command("receive", JSONObject().put("link", link)) }
            } catch (failure: Exception) {
                if (generation == intent) { error = failure.message; busy = false; refresh() }
            } finally { removeFile?.delete() }
        }
    }
    fun receive(value: String) { link = value; command("receive", JSONObject().put("link", value)) }
    suspend fun poll() {
        val generation = intent
        runCatching { withContext(Dispatchers.IO) { call(JSONObject().put("op", "state")) } }.onSuccess { if (generation == intent) { state = it; jobId = it.optString("job_id") } }
    }
    fun refresh() { scope.launch { poll() } }
    fun cancel() {
        intent++; busy = false; error = null
        val stopping = jobId
        if (stopping == null) return
        scope.launch {
            val value = withContext(Dispatchers.IO) { runCatching { call(JSONObject().put("op", "cancel").put("job_id", stopping)) }.getOrNull() }
            if (jobId == stopping && value != null) state = value
        }
    }
    fun scanImage(context: Context, uri: Uri) {
        if (busy) return
        busy = true; error = null
        val generation = ++intent
        scope.launch {
            var file: File? = null
            try {
                file = withContext(Dispatchers.IO) {
                    val output = File.createTempFile("book-share-qr-", ".image", context.cacheDir)
                    try {
                        context.contentResolver.openInputStream(uri)?.use { input -> output.outputStream().use { stream ->
                            val buffer = ByteArray(65536); var total = 0L
                            while (true) { val read = input.read(buffer); if (read < 0) break; total += read; check(total <= 16 * 1024 * 1024) { "QR image is too large" }; stream.write(buffer, 0, read) }
                        } } ?: error("Cannot read QR image")
                    } catch (failure: Exception) { output.delete(); throw failure }
                    output
                }
                if (generation != intent) { file.delete(); return@launch }
                busy = false; command("decode_image", JSONObject().put("path", file.absolutePath), file)
            } catch (failure: Exception) { file?.delete(); if (generation == intent) { error = failure.message; busy = false } }
        }
    }
    fun resetReceiver() {
        cancel(); jobId = null; state = JSONObject(); result = null; error = null; link = ""
    }
    fun close() { cancel() }
}
