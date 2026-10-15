package com.zhongbai233.epub.reader.ui.reader

import android.app.Activity
import android.app.Instrumentation
import android.graphics.Paint
import android.graphics.Typeface
import android.os.Bundle
import com.zhongbai233.epub.reader.util.FontLibrary
import org.json.JSONObject
import java.io.File
import java.security.MessageDigest

internal object FontLibraryRuntimeChecks {
    fun run(runner: Instrumentation) {
        val report = JSONObject()
        try {
            val context = runner.targetContext
            runner.startActivitySync(android.content.Intent(context, com.zhongbai233.epub.reader.MainActivity::class.java).addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK))
            runner.waitForIdleSync()
            val fixtures = File(context.filesDir, "font-catalog")
            if (fixtures.isDirectory) {
                FontLibrary.catalog(context).forEach { entry ->
                    val image = context.assets.open("fonts/${entry.preview}").use { android.graphics.BitmapFactory.decodeStream(it) }
                    check(image != null && image.width == 1200 && image.height == 216 && android.graphics.Color.alpha(image.getPixel(0, 0)) == 0)
                    image.recycle()
                    val source = File(fixtures, entry.filename)
                    check(source.length() == entry.bytes)
                    check(MessageDigest.getInstance("SHA-256").digest(source.readBytes()).joinToString("") { "%02x".format(it) } == entry.sha256)
                    val typeface = Typeface.createFromFile(source)
                    check(Paint().apply { this.typeface = typeface; textSize = 20f }.hasGlyph("A"))
                }
                report.put("catalog_native_faces", FontLibrary.catalog(context).size)
                report.put("bundled_preview_images", FontLibrary.catalog(context).size)
                val source = File(fixtures, "atkinson-hyperlegible.ttf")
                FontLibrary.importFont(context, android.net.Uri.fromFile(source))
                val count = File(context.filesDir, "fonts/imports").listFiles()!!.size
                FontLibrary.importFont(context, android.net.Uri.fromFile(source))
                check(File(context.filesDir, "fonts/imports").listFiles()!!.size == count)
                check(FontLibrary.installedFonts(context).any { it.path.contains("fonts/imports") })
                val invalid = File(context.filesDir, "invalid.ttf").also { it.writeText("not a font") }
                check(runCatching { FontLibrary.importFont(context, android.net.Uri.fromFile(invalid)) }.isFailure)
                check(File(context.filesDir, "fonts/imports").listFiles()!!.size == count)
                check(FontLibrary.external(context).size == 2)
                report.put("import_duplicate_and_invalid", true).put("official_only_entries", true)
            }
            val font = FontLibrary.catalog(context).first { it.id == "atkinson-hyperlegible" }
            FontLibrary.download(context, font)
            val file = FontLibrary.file(context, font)
            check(file.length() == font.bytes)
            val hash = MessageDigest.getInstance("SHA-256")
            file.inputStream().use { input -> val buffer = ByteArray(65536); while (true) { val n = input.read(buffer); if (n < 0) break; hash.update(buffer, 0, n) } }
            check(hash.digest().joinToString("") { "%02x".format(it) } == font.sha256)
            val paint = Paint().apply { typeface = Typeface.createFromFile(file); textSize = 20f }
            check("Aa0123".all { paint.hasGlyph(it.toString()) })
            check(FontLibrary.installedFonts(context).any { it.displayName == font.nameZh && it.path == file.absolutePath })
            report.put("passed", true).put("real_download_checksum", true).put("native_glyphs", true).put("persistent_discovery", true)
        } catch (error: Throwable) { report.put("passed", false).put("error", error.stackTraceToString()) }
        File(runner.targetContext.filesDir, "font-library-check.json").writeText(report.toString(2))
        runner.finish(if (report.optBoolean("passed")) Activity.RESULT_OK else Activity.RESULT_CANCELED,
            Bundle().apply { putString("font_library", report.toString()) })
    }
}
