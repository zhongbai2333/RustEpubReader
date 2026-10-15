package com.zhongbai233.epub.reader.ui.library

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.*
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.i18n.I18n
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first

// One navigation surface; page curls and native reader contents are never scaled.
@Stable
class BookPresentation(private val scope: CoroutineScope) {
    val expansion = Animatable(1f)
    val veil = Animatable(0f)
    var active by mutableStateOf(false); private set
    var closing by mutableStateOf(false); private set
    var showShelf by mutableStateOf(false); private set
    var mountReader by mutableStateOf(false); private set
    var ready by mutableStateOf(false)
    var source by mutableStateOf<Rect?>(null)
    var sourceUri: String? = null
    val covers = mutableMapOf<String, Rect>()
    val shelfOrder = StableShelfOrder()
    var shelfRevision by mutableIntStateOf(0); private set
    private var job: Job? = null
    fun open(uri: String, reduceMotion: Boolean, action: () -> Unit) {
        if (active) return
        sourceUri=uri; source=covers[uri]; active=true; closing=false; showShelf=true; ready=false; mountReader=false
        job=scope.launch {
            expansion.snapTo(if(reduceMotion || source==null) 1f else 0f); veil.snapTo(if(reduceMotion || source==null) 1f else 0f)
            mountReader=true
            action()
            if(!reduceMotion && source!=null) veil.animateTo(1f,tween(100))
            if(!reduceMotion && source!=null) {
                expansion.animateTo(0.94f,tween(300,easing=FastOutSlowInEasing))
                while(!ready) {
                    val next=expansion.value+(0.99f-expansion.value)*0.45f
                    expansion.animateTo(next,tween(160,easing=FastOutSlowInEasing))
                }
                expansion.animateTo(1f,tween(80,easing=FastOutSlowInEasing))
            } else snapshotFlow { ready }.first { it }
            showShelf=false
            veil.animateTo(0f,tween(160))
            active=false
        }
    }
    fun close(reduceMotion: Boolean, action: () -> Unit, after: () -> Unit = {}) {
        if(closing) return
        job?.cancel(); active=true; closing=true
        job=scope.launch {
            veil.animateTo(1f,tween(140))
            action(); showShelf=true
            withFrameNanos { }
            source=sourceUri?.let { covers[it] }
            if(reduceMotion || source==null) veil.animateTo(0f,tween(160))
            else { expansion.animateTo(0f,tween(300,easing=FastOutSlowInEasing)); veil.animateTo(0f,tween(100)) }
            active=false; closing=false; showShelf=false
            // The cover has finished dissolving before any recent-order movement.
            shelfOrder.reset(); shelfRevision++
            after()
        }
    }
}

@Composable
fun BookPresentationOverlay(state: BookPresentation, paper: Color) {
    var delayed by remember { mutableStateOf(false) }
    var viewport by remember { mutableStateOf(Rect.Zero) }
    LaunchedEffect(Unit) { delay(600); delayed=true }
    Box(Modifier.fillMaxSize().onGloballyPositioned { viewport=it.boundsInRoot() }
        .pointerInput(Unit) { awaitPointerEventScope { while(true) { awaitPointerEvent(PointerEventPass.Initial).changes.forEach { it.consume() } } } }) {
        Canvas(Modifier.fillMaxSize()) {
            val full=Rect(0f,0f,size.width,size.height)
            val source=state.source?.translate(-viewport.left,-viewport.top)?.takeIf { full.contains(it.topLeft) && full.contains(it.bottomRight) }
            val t=state.expansion.value
            val frame=if(source==null) full else Rect(
                source.left*(1-t),source.top*(1-t),
                source.right+(full.right-source.right)*t,source.bottom+(full.bottom-source.bottom)*t)
            drawRoundRect(paper.copy(alpha=state.veil.value),topLeft=frame.topLeft,size=frame.size,
                cornerRadius=CornerRadius(12.dp.toPx()*(1-t)))
        }
        if(delayed && !state.ready && !state.closing) CircularProgressIndicator(
            Modifier.size(18.dp).align(Alignment.Center).semantics { contentDescription=I18n.t("manage.opening") },
            color=if(paper.luminance()<0.45f) Color.LightGray else Color.Gray,strokeWidth=1.5.dp)
    }
}

fun readerPaperColor(index: Int, custom: Int, dark: Boolean): Color =
    listOf(Color(0xFFF5F0E8),Color(0xFFF1F3F5),Color(0xFFE8F0E8),Color(0xFF1A1A1A),Color(0xFF24262B)).getOrNull(index)
        ?: if(index==5) Color(custom) else if(dark) Color(0xFF1A1A1A) else Color(0xFFF5F0E8)
