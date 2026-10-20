@file:Suppress("MatchingDeclarationName")

package eu.wewox.pagecurl.page

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.AnimationVector4D
import androidx.compose.animation.core.VectorConverter
import androidx.compose.animation.core.calculateTargetValue
import androidx.compose.animation.splineBasedDecay
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.awaitTouchSlopOrCancellation
import androidx.compose.foundation.gestures.drag
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.PointerInputChange
import androidx.compose.ui.input.pointer.PointerInputScope
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.input.pointer.util.VelocityTracker
import androidx.compose.ui.unit.IntSize
import eu.wewox.pagecurl.utils.rotate
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import kotlin.math.PI

internal data class DragConfig(
    val edge: Animatable<Edge, AnimationVector4D>,
    val start: Edge,
    val end: Edge,
    val isEnabled: () -> Boolean,
    val isDragSucceed: (Offset, Offset) -> Boolean,
    val onChange: () -> Unit,
    val edgeCreatorOverride: NewEdgeCreator? = null,
)

internal suspend fun PointerInputScope.detectCurlGestures(
    scope: CoroutineScope,
    newEdgeCreator: NewEdgeCreator,
    getConfig: (Offset, Offset) -> DragConfig?,
) {
    // Use velocity tracker to support flings
    val velocityTracker = VelocityTracker()

    var config: DragConfig? = null
    var startOffset: Offset = Offset.Zero

    detectCustomDragGestures(
        onDragStart = { start, end ->
            startOffset = start
            config = getConfig(start, end)
            config != null
        },
        onDragEnd = { endOffset, complete ->
            config?.apply {
                val velocity = velocityTracker.calculateVelocity()
                val decay = splineBasedDecay<Offset>(this@detectCurlGestures)
                val flingEndOffset = decay.calculateTargetValue(
                    Offset.VectorConverter,
                    endOffset,
                    Offset(velocity.x, velocity.y)
                ).let {
                    Offset(
                        it.x.coerceIn(0f, size.width.toFloat() - 1),
                        it.y.coerceIn(0f, size.height.toFloat() - 1)
                    )
                }

                scope.launch {
                    if (complete && isDragSucceed(startOffset, flingEndOffset)) {
                        try {
                            edge.animateTo(end)
                        } finally {
                            onChange()
                            edge.snapTo(start)
                        }
                    } else {
                        try {
                            edge.animateTo(start)
                        } finally {
                            edge.snapTo(start)
                        }
                    }
                }
            }
        },
        onDrag = { change, _ ->
            config?.apply {
                if (!isEnabled()) {
                    throw CancellationException()
                }

                velocityTracker.addPosition(System.currentTimeMillis(), change.position)

                scope.launch {
                    val creator = edgeCreatorOverride ?: newEdgeCreator
                    val target = creator.createNew(size, startOffset, change.position)
                    edge.animateTo(target)
                }
            }
        }
    )
}

internal suspend fun PointerInputScope.detectCustomDragGestures(
    onDragStart: (Offset, Offset) -> Boolean,
    onDragEnd: (Offset, Boolean) -> Unit,
    onDrag: (change: PointerInputChange, dragAmount: Offset) -> Unit
) {
    awaitEachGesture {
        val down = awaitFirstDown(requireUnconsumed = false)
        var overSlop = Offset.Zero
        // A held pointer belongs to the reader's long-press selection. Do not
        // claim its first later movement as a page curl and cancel that owner.
        val withinDragWindow = withTimeoutOrNull(viewConfiguration.longPressTimeoutMillis) {
            var candidate: PointerInputChange?
            do {
                candidate = awaitTouchSlopOrCancellation(down.id) { change, over ->
                    change.consume()
                    overSlop = over
                }
            } while (candidate != null && !candidate.isConsumed)
            candidate
        }
        var drag = withinDragWindow
        if (drag != null) {
            if (!onDragStart.invoke(down.position, drag.position)) {
                return@awaitEachGesture
            }
            onDrag(drag, overSlop)
            val completed = drag(drag.id) {
                drag = it
                onDrag(it, it.positionChange())
                it.consume()
            }
            onDragEnd(drag?.position ?: down.position, completed)
        }
    }
}

internal abstract class NewEdgeCreator {

    abstract fun createNew(size: IntSize, startOffset: Offset, currentOffset: Offset): Edge

    protected fun createVectors(size: IntSize, startOffset: Offset, currentOffset: Offset): Pair<Offset, Offset> {
        val vector = Offset(size.width.toFloat(), startOffset.y) - currentOffset
        val rotatedVector = vector.rotate(PI.toFloat() / 2)
        return vector to rotatedVector
    }

    class Default : NewEdgeCreator() {
        override fun createNew(size: IntSize, startOffset: Offset, currentOffset: Offset): Edge {
            val vectors = createVectors(size, startOffset, currentOffset)
            return Edge(currentOffset - vectors.second, currentOffset + vectors.second)
        }
    }

    class PageEdge : NewEdgeCreator() {
        override fun createNew(size: IntSize, startOffset: Offset, currentOffset: Offset): Edge {
            val (vector, rotatedVector) = createVectors(size, startOffset, currentOffset)
            return Edge(currentOffset - rotatedVector + vector / 2f, currentOffset + rotatedVector + vector / 2f)
        }
    }

    /**
     * Wraps a delegate creator: optionally mirrors input coordinates horizontally,
     * then clamps output Edge x to [spineX, width]. Used for bookSpread mode.
     */
    class BookSpread(
        private val delegate: NewEdgeCreator,
        private val spineX: Float,
        private val mirrored: Boolean = false,
    ) : NewEdgeCreator() {
        override fun createNew(size: IntSize, startOffset: Offset, currentOffset: Offset): Edge {
            val w = size.width.toFloat()
            val s = if (mirrored) Offset(w - startOffset.x, startOffset.y) else startOffset
            val c = if (mirrored) Offset(w - currentOffset.x, currentOffset.y) else currentOffset
            val edge = delegate.createNew(size, s, c)
            return Edge(
                Offset(edge.top.x.coerceIn(spineX, w), edge.top.y),
                Offset(edge.bottom.x.coerceIn(spineX, w), edge.bottom.y)
            )
        }
    }
}
