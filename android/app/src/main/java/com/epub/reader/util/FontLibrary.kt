package com.zhongbai233.epub.reader.util

import android.content.Context
import android.graphics.Typeface
import android.net.Uri
import android.provider.OpenableColumns
import androidx.compose.runtime.*
import kotlinx.coroutines.*
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.security.MessageDigest

data class DownloadableFont(val id: String, val nameZh: String, val nameEn: String,
    val filename: String, val url: String, val sha256: String, val bytes: Long,
    val license: String, val licenseFile: String, val projectUrl: String,
    val language: String, val descriptionZh: String, val descriptionEn: String, val licenseUrl: String, val preview: String, val previewText: String)

data class ExternalFont(val id: String, val nameZh: String, val nameEn: String,
    val language: String, val descriptionZh: String, val descriptionEn: String,
    val projectUrl: String, val licenseUrl: String, val checkedAt: String)

/** Downloads outlive settings sheets; only complete verified files enter discovery. */
object FontLibrary {
    private var entries: List<DownloadableFont>? = null
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    val loading = mutableStateMapOf<String, Boolean>()
    val failed = mutableStateMapOf<String, Boolean>()
    var revision by mutableIntStateOf(0); private set
    @Synchronized fun catalog(context: Context): List<DownloadableFont> {
        entries?.let { return it }
        val array = JSONArray(context.assets.open("fonts/catalog.json").bufferedReader().use { it.readText() })
        return (0 until array.length()).map { i -> array.getJSONObject(i).let {
            DownloadableFont(it.getString("id"), it.getString("name_zh"), it.getString("name_en"),
                it.getString("filename"), it.getString("url"), it.getString("sha256"), it.getLong("bytes"),
                it.getString("license"), it.getString("license_file"), it.getString("project_url"),
                it.getString("language"), it.getString("description_zh"), it.getString("description_en"), it.getString("license_url"), it.getString("preview"), it.getString("preview_text"))
        } }.also { entries = it }
    }
    fun external(context: Context): List<ExternalFont> {
        val array = JSONArray(context.assets.open("fonts/external.json").bufferedReader().use { it.readText() })
        return (0 until array.length()).map { i -> array.getJSONObject(i).let {
            ExternalFont(it.getString("id"), it.getString("name_zh"), it.getString("name_en"), it.getString("language"),
                it.getString("description_zh"), it.getString("description_en"), it.getString("project_url"), it.getString("license_url"), it.getString("checked_at"))
        } }
    }
    fun file(context: Context, font: DownloadableFont) = File(File(context.filesDir, "fonts"), font.filename)
    fun installed(context: Context, font: DownloadableFont) = file(context, font).let { it.isFile && it.length() == font.bytes }
    fun installedFonts(context: Context) = catalog(context).filter { installed(context, it) }
        .map { FontItem(it.nameZh, file(context, it).absolutePath) } + importedFonts(context)
    private fun importedFonts(context: Context): List<FontItem> {
        val dir = File(context.filesDir, "fonts/imports")
        return dir.listFiles()?.filter { it.extension == "json" }?.mapNotNull { metadata -> runCatching {
            val json = JSONObject(metadata.readText()); val name = json.getString("filename")
            require(name == File(name).name && name.matches(Regex("[0-9a-f]{64}\\.(ttf|otf)")))
            val file = File(dir, name); require(file.isFile)
            FontItem(json.getString("name"), file.absolutePath)
        }.getOrNull() }?.sortedBy { it.displayName } ?: emptyList()
    }
    fun startImport(context: Context, uris: List<Uri>) { scope.launch { importFonts(context.applicationContext, uris) } }
    /** Copies a provider URI while access is valid; never trusts provider size or path. */
    suspend fun importFonts(context: Context, uris: List<Uri>) {
        if (loading["import"] == true) return
        loading["import"] = true; failed.remove("import")
        try {
            withContext(Dispatchers.IO) { uris.forEach { importFont(context, it) } }
        } catch (e: CancellationException) { throw e }
        catch (_: Exception) { failed["import"] = true }
        finally { revision++; loading.remove("import") }
    }
    internal fun importFont(context: Context, uri: Uri) {
        val display = context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
            if (it.moveToFirst()) it.getString(0) else null
        } ?: uri.lastPathSegment.orEmpty()
        val ext = display.substringAfterLast('.', "").lowercase(); require(ext in listOf("ttf", "otf"))
        val dir = File(context.filesDir, "fonts/imports").also { it.mkdirs() }
        val temp = File.createTempFile("import-", ".part", dir)
        try {
            val hash = MessageDigest.getInstance("SHA-256"); var count = 0L
            requireNotNull(context.contentResolver.openInputStream(uri)).use { input -> temp.outputStream().buffered().use { output ->
                val buffer = ByteArray(256 * 1024)
                while (true) { val n = input.read(buffer); if (n < 0) break
                    count += n; require(count <= 64L * 1024 * 1024); hash.update(buffer, 0, n); output.write(buffer, 0, n)
                }
            } }
            require(count > 0); validateNativeFont(temp)
            val digest = hash.digest().joinToString("") { "%02x".format(it) }
            val target = File(dir, "$digest.$ext")
            Files.move(temp.toPath(), target.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
            val metadata = JSONObject().put("filename", target.name).put("name", display.substringBeforeLast('.').take(100) + " · " + digest.take(6))
            val staged = File.createTempFile("meta-", ".part", dir)
            try { staged.writeText(metadata.toString()); Files.move(staged.toPath(), File(dir, "$digest.json").toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING) }
            finally { staged.delete() }
        } finally { temp.delete() }
    }
    fun start(context: Context, font: DownloadableFont) {
        if (loading[font.id] == true || installed(context, font)) return
        loading[font.id] = true; failed.remove(font.id)
        val app = context.applicationContext
        scope.launch {
            try { withContext(Dispatchers.IO) { download(app, font) }; revision++ }
            catch (e: CancellationException) { throw e }
            catch (_: Exception) { failed[font.id] = true }
            finally { loading.remove(font.id) }
        }
    }
    private fun validateNativeFont(file: File) {
        // Some Android releases silently return the default Typeface for invalid input.
        java.io.RandomAccessFile(file, "r").use { input ->
            require(input.length() >= 12)
            val signature = input.readInt(); require(signature == 0x00010000 || signature == 0x4f54544f)
            val count = input.readUnsignedShort(); require(count in 1..256 && 12L + count * 16L <= input.length())
            input.skipBytes(6); val tags = mutableSetOf<Int>()
            repeat(count) {
                tags.add(input.readInt()); input.readInt()
                val offset = input.readInt().toLong() and 0xffffffffL
                val size = input.readInt().toLong() and 0xffffffffL
                require(offset + size <= input.length())
            }
            require(tags.containsAll(listOf(0x636d6170, 0x68656164, 0x6d617870, 0x68686561, 0x686d7478)))
            require(tags.contains(0x676c7966) || tags.contains(0x43464620) || tags.contains(0x43464632))
        }
        if (android.os.Build.VERSION.SDK_INT >= 29) android.graphics.fonts.Font.Builder(file).build()
        require(Typeface.createFromFile(file) != Typeface.DEFAULT)
    }
    internal fun download(context: Context, font: DownloadableFont) {
        val directory = File(context.filesDir, "fonts").also { it.mkdirs() }
        require(File(font.filename).name == font.filename && font.bytes in 1..64L*1024*1024)
        val temp = File.createTempFile("font-", ".part", directory)
        val connection = URL(font.url).openConnection() as HttpURLConnection
        try {
            connection.connectTimeout = 20_000; connection.readTimeout = 60_000
            connection.setRequestProperty("User-Agent", "RustEpubReader-fonts")
            check(connection.responseCode == 200)
            val hash = MessageDigest.getInstance("SHA-256"); var count = 0L
            connection.inputStream.buffered().use { input -> temp.outputStream().buffered().use { output ->
                val buffer = ByteArray(64 * 1024)
                while (true) {
                    val n = input.read(buffer); if (n < 0) break
                    count += n; check(count <= font.bytes)
                    hash.update(buffer, 0, n); output.write(buffer, 0, n)
                }
            } }
            check(count == font.bytes && hash.digest().joinToString("") { "%02x".format(it) } == font.sha256)
            validateNativeFont(temp)
            Files.move(temp.toPath(), file(context, font).toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
            context.assets.open("fonts/${font.licenseFile}").use { input -> File(directory, font.licenseFile).outputStream().use { input.copyTo(it) } }
        } finally { connection.disconnect(); temp.delete() }
    }
}
