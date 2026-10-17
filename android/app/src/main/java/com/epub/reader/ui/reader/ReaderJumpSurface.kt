package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.ui.graphics.Color
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.rememberGraphicsLayer
import androidx.compose.ui.graphics.layer.drawLayer
import androidx.compose.ui.unit.IntSize

private class JumpPaper(var pulse: Int) {
    var current = 0
    var recorded = false
    var frozen = false
    var size = IntSize.Zero
}

/** Two bounded native display lists, with no bitmap readback on page turns. */
@Composable
internal fun ReaderJumpSurface(pulse: Int, ready: Boolean, animated: Boolean, paperColor: Color, content: @Composable () -> Unit) {
    val first = rememberGraphicsLayer()
    val second = rememberGraphicsLayer()
    val paper = remember { JumpPaper(pulse) }
    val opacity = remember { Animatable(0f) }
    var fadingPulse by remember { mutableIntStateOf(pulse) }
    LaunchedEffect(pulse, ready, animated) {
        if (ready && animated && pulse != fadingPulse) {
            opacity.snapTo(1f); fadingPulse = pulse
            opacity.animateTo(0f, tween(220))
        }
    }
    Box(Modifier.fillMaxSize().drawWithContent {
        if (paper.pulse != pulse) {
            paper.frozen = paper.recorded && animated
            paper.current = 1 - paper.current; paper.pulse = pulse
        }
        val dimensions = IntSize(size.width.toInt(), size.height.toInt())
        if (paper.size != dimensions) { paper.frozen = false; paper.size = dimensions }
        val live = if (paper.current == 0) first else second
        val source = if (paper.current == 0) second else first
        live.alpha = 1f
        live.record { this@drawWithContent.drawContent() }
        drawLayer(live); paper.recorded = true
        if (paper.frozen) {
            val alpha = if (!ready || fadingPulse != pulse) 1f else opacity.value
            if (alpha > 0f) { source.alpha = alpha; drawLayer(source) }
            else paper.frozen = false
        }
    }.background(paperColor)) { content() }
}
