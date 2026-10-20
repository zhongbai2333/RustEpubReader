package com.zhongbai233.epub.reader.ui.reader

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.model.ContentBlock
import eu.wewox.pagecurl.ExperimentalPageCurlApi
import eu.wewox.pagecurl.page.*
import eu.wewox.pagecurl.config.rememberPageCurlConfig
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.util.concurrent.atomic.AtomicInteger

/** Diagnostic only: exercises the production curl and illustration composables. */
@OptIn(ExperimentalPageCurlApi::class)
class CurlImageCheckActivity : ComponentActivity() {
    @Volatile var progress = 0f
    @Volatile var ready = false
    @Volatile var finished = false
    val mounts = AtomicInteger()
    private var startTurn: (() -> Unit)? = null
    fun turn() { startTurn?.invoke() }
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (intent.getBooleanExtra("layoutBounds", false)) { setContent { Box(Modifier.fillMaxSize()) }; return }
        val data = intent.getStringExtra("image")!!
        val spread = intent.getBooleanExtra("spread", false)
        val backward = intent.getBooleanExtra("backward", false)
        requestedOrientation = if (spread) android.content.pm.ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE else android.content.pm.ActivityInfo.SCREEN_ORIENTATION_PORTRAIT
        setContent {
            val state = rememberPageCurlState(1)
            val scope = rememberCoroutineScope()
            val config = rememberPageCurlConfig(isBookSpread = spread)
            val currentProgress = state.progress
            SideEffect {
                progress = currentProgress
                startTurn = { scope.launch {
                    val animation: suspend androidx.compose.animation.core.Animatable<Edge, androidx.compose.animation.core.AnimationVector4D>.(androidx.compose.ui.geometry.Size) -> Unit = { size ->
                        for (step in 1..12) {
                            val fraction = step / 16f
                            val x = if (spread) size.width * (1 - fraction / 2) else size.width * (if (backward) fraction else 1 - fraction)
                            snapTo(Edge(Offset(x, 0f), Offset(x, size.height)))
                            delay(100)
                        }
                    }
                    if (backward) state.prev(animation) else state.next(animation)
                    finished = true
                } }
                ready = true
            }
            PageCurl(4, Modifier.fillMaxSize().background(Color.White), state, config) { page ->
                DisposableEffect(page) { mounts.incrementAndGet(); onDispose {} }
                Row(Modifier.fillMaxSize().background(Color.White)) {
                    repeat(if (spread) 2 else 1) {
                        Box(Modifier.weight(1f).fillMaxHeight(), contentAlignment = androidx.compose.ui.Alignment.Center) {
                            ContentBlockView(chapterIndex = 0, blockIndex = 0, block = ContentBlock.Image(data = data, alt = "curl illustration"),
                                imageMaxHeight = 300.dp, fontSize = 18f, textColor = Color.Black, linkColor = Color.Blue,
                                fontFamily = FontFamily.Default, onLinkClick = {}, onTextTapped = {})
                        }
                    }
                }
            }
        }
    }
}
