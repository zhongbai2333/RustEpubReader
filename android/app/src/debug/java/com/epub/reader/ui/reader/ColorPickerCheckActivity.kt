package com.zhongbai233.epub.reader.ui.reader

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.text.font.FontFamily
import com.zhongbai233.epub.reader.model.ReadingStyle
import org.json.JSONObject

/** Opens the production editor without modifying the user's saved reading styles. */
class ColorPickerCheckActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MaterialTheme {
                ReadingStyleEditor(ReadingStyle.customTemplate, false, 18f, FontFamily.Default,
                    onDismiss = { report("cancel", ReadingStyle.customTemplate); finish() },
                    onSave = { report("save", it); finish() })
            }
        }
    }
    private fun report(action: String, style: ReadingStyle) {
        val value = JSONObject().put("action", action)
            .put("light_paper", "%06X".format(style.light.paper and 0xFFFFFF))
            .put("light_ink", "%06X".format(style.light.ink and 0xFFFFFF))
            .put("dark_paper", "%06X".format(style.dark.paper and 0xFFFFFF))
            .put("dark_ink", "%06X".format(style.dark.ink and 0xFFFFFF))
        filesDir.resolve("color-picker-verification.json").writeText(value.toString())
    }
}
