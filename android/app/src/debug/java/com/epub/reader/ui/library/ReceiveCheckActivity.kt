package com.zhongbai233.epub.reader.ui.library

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.ui.theme.EpubReaderTheme

/** Isolated explicit fixtures, not a network-transfer or camera-decode acceptance test. */
class ReceiveCheckActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState); enableEdgeToEdge(); I18n.setLanguage(intent.getStringExtra("language") ?: "zh_cn")
        setContent { EpubReaderTheme(darkTheme = intent.getBooleanExtra("dark", false)) {
            var link by remember { mutableStateOf("") }
            val scene = intent.getStringExtra("scene") ?: "entry"
            var state by remember { mutableStateOf(when(scene) {
                "preview" -> ReceiveSheetState(preview = listOf(ReceivePreviewBook("阅读与时间", 1_430_000), ReceivePreviewBook("山河来信", 2_180_000), ReceivePreviewBook("城市漫游：街巷里的故事", 4_260_000), ReceivePreviewBook("Notes on Light", 630_000)))
                "busy" -> ReceiveSheetState(busy = true, phase = "verifying", fraction = 0.63f)
                "success" -> ReceiveSheetState(result = 3 to 1)
                "error" -> ReceiveSheetState(error = I18n.t("scan.invalid"))
                else -> ReceiveSheetState()
            }) }
            val scanner = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
                if (result.resultCode == Activity.RESULT_OK) state = ReceiveSheetState(result = 0 to 0)
            }
            Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
                Column(Modifier.statusBarsPadding().padding(24.dp)) {
                    Text(I18n.t("library.title"), style = MaterialTheme.typography.headlineSmall)
                }
                ReceiveBookSheet(state, link, {link = it},
                    scan = { scanner.launch(Intent(this, BookScanActivity::class.java)) }, image = {},
                    receive = { state = ReceiveSheetState(busy = true, phase = "receiving", fraction = 0.35f) },
                    accept = { state = ReceiveSheetState(result = 4 to 0) }, cancel = { state = ReceiveSheetState() },
                    reset = {state = ReceiveSheetState();link = ""}, onDismiss = {finish()})
            }
        } }
    }
}
