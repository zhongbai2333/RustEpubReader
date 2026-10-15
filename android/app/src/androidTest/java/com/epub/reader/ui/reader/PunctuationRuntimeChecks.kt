package com.zhongbai233.epub.reader.ui.reader

import android.app.Activity
import android.app.Instrumentation
import android.os.Bundle
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.createFontFamilyResolver
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.sp
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/** Native text measurement only: no activity, gestures or changes to app settings. */
internal object PunctuationRuntimeChecks {
    fun run(test: Instrumentation) {
        val report = JSONObject()
        try {
            val samples = listOf(
                "不管怎么说，就算真有什么圣女，也还是很离谱呀，上哪和圣女说话去？",
                "他说：“今天的故事结束了！”接着翻开《下一章》，继续读下去……然后呢？",
                "读到这里（包括括号里的内容），这一章就结束了。下次再读《春江花月夜》！"
            )
            val prohibited = "，。！？；：、）》」』】〉〕〗〙〛.,!?;:)]}％‰".toSet()
            val body = JSONArray(); val titles = JSONArray()
            var checked = 0
            val density = Density(test.targetContext.resources.displayMetrics.density, 1f)
            val measurer = TextMeasurer(createFontFamilyResolver(test.targetContext), density, LayoutDirection.Ltr, cacheSize = 0)
            for ((sampleIndex, text) in samples.withIndex()) for (width in 120..420) {
                val layout = measurer.measure(AnnotatedString(text), style = TextStyle(fontSize = 18.sp), constraints = Constraints(maxWidth = (width * density.density).toInt()))
                for (line in 1 until layout.lineCount) {
                    checked++
                    val value = text.substring(layout.getLineStart(line), layout.getLineEnd(line)).trim()
                    if (value.firstOrNull() in prohibited && body.length() < 12) body.put(JSONObject().put("sample", sampleIndex).put("width", width).put("line", line).put("text", value))
                }
                val broken = breakTitleIntoLines(text, width * density.density, 18f, density.density)
                for ((line, value) in broken.lines().withIndex()) if (line > 0 && value.firstOrNull() in prohibited && titles.length() < 12) {
                    titles.put(JSONObject().put("sample", sampleIndex).put("width", width).put("line", line).put("text", value))
                }
            }
            report.put("engine", "Android Compose TextMeasurer / production title helper").put("cases", samples.size * 301)
                .put("soft_wrapped_lines", checked).put("body_failures", body).put("title_failures", titles)
                .put("body_passed", body.length() == 0).put("titles_passed", titles.length() == 0)
        } catch (failure: Throwable) { report.put("error", failure.toString()) }
        File(test.targetContext.cacheDir, "punctuation-check.json").writeText(report.toString(2))
        test.finish(Activity.RESULT_OK, Bundle().apply { putString("stream", report.toString(2)) })
    }
}
