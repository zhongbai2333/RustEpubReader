package eu.wewox.pagecurl.page

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.graphicsLayer
import eu.wewox.pagecurl.ExperimentalPageCurlApi
import eu.wewox.pagecurl.config.PageCurlConfig
import eu.wewox.pagecurl.config.rememberPageCurlConfig

/**
 * Shows the pages which may be turned by drag or tap gestures.
 *
 * @param count The count of pages.
 * @param modifier The modifier for this composable.
 * @param state The state of the PageCurl. Use this to programmatically change the current page or observe changes.
 * @param config The configuration for PageCurl.
 * @param content The content lambda to provide the page composable. Receives the page number.
 */
@ExperimentalPageCurlApi
@Composable
public fun PageCurl(
    count: Int,
    modifier: Modifier = Modifier,
    state: PageCurlState = rememberPageCurlState(),
    config: PageCurlConfig = rememberPageCurlConfig(),
    content: @Composable (Int) -> Unit
) {
    val scope = rememberCoroutineScope()

    BoxWithConstraints(modifier) {
        state.setup(count, constraints, config.isBookSpread)

        val updatedCurrent by rememberUpdatedState(state.current)
        val internalState by rememberUpdatedState(state.internalState ?: return@BoxWithConstraints)

        val updatedConfig by rememberUpdatedState(config)
        val isBookSpread = updatedConfig.isBookSpread
        val spineX = constraints.maxWidth / 2f

        val dragGestureModifier = when (val interaction = updatedConfig.dragInteraction) {
            is PageCurlConfig.GestureDragInteraction ->
                Modifier
                    .dragGesture(
                        dragInteraction = interaction,
                        state = internalState,
                        enabledForward = updatedConfig.dragForwardEnabled && updatedCurrent < state.max - 1,
                        enabledBackward = updatedConfig.dragBackwardEnabled && updatedCurrent > 0,
                        scope = scope,
                        onChange = { state.current = updatedCurrent + it }
                    )

            is PageCurlConfig.StartEndDragInteraction ->
                Modifier
                    .dragStartEnd(
                        dragInteraction = interaction,
                        state = internalState,
                        enabledForward = updatedConfig.dragForwardEnabled && updatedCurrent < state.max - 1,
                        enabledBackward = updatedConfig.dragBackwardEnabled && updatedCurrent > 0,
                        scope = scope,
                        onChange = { state.current = updatedCurrent + it }
                    )
        }

        Box(
            Modifier
                .then(dragGestureModifier)
                .tapGesture(
                    config = updatedConfig,
                    scope = scope,
                    onTapForward = state::next,
                    onTapBackward = state::prev,
                )
        ) {
            if (isBookSpread) {
                val isForwardAnimating = internalState.forward.value != internalState.rightEdge
                val isBackwardAnimating = internalState.backward.value != internalState.rightEdge
                val isMirrored = isBackwardAnimating
                val mirrorMod = if (isMirrored) Modifier.graphicsLayer { scaleX = -1f } else Modifier
                val contentMirrorMod = if (isMirrored) Modifier.graphicsLayer { scaleX = -1f } else Modifier

                // Curl coordinates change every frame. They must not replace the
                // content composition and restart its asynchronous image loads.
                key(updatedCurrent) {
                    if (isForwardAnimating || isBackwardAnimating) {
                        val edge = if (isForwardAnimating) internalState.forward.value else internalState.backward.value
                        val bottomIndex = if (isForwardAnimating) updatedCurrent + 1 else updatedCurrent - 1

                        Box(Modifier.fillMaxSize().then(mirrorMod)) {
                            // Layer 1 (bottom): next/prev spread fully rendered, visible where not covered
                            if (bottomIndex in 0 until state.max) {
                                Box(Modifier.fillMaxSize().then(contentMirrorMod)) {
                                    content(bottomIndex)
                                }
                            }

                            // Layer 2 (front-clip): current spread clipped by curl line
                            Box(
                                Modifier
                                    .fillMaxSize()
                                    .drawCurlFrontBookSpread(edge.top, edge.bottom, spineX)
                            ) {
                                Box(Modifier.fillMaxSize().then(contentMirrorMod)) {
                                    content(updatedCurrent)
                                }
                            }

                            // Layer 3 (back-flap): curled back of the page, left column only
                            if (bottomIndex in 0 until state.max) {
                                Box(
                                    Modifier
                                        .fillMaxSize()
                                        .graphicsLayer { clip = false }
                                        .drawCurlBackFlapBookSpread(updatedConfig, edge.top, edge.bottom, spineX)
                                ) {
                                    Box(
                                        Modifier
                                            .fillMaxSize()
                                            .graphicsLayer { scaleX = -1f }
                                            .drawWithContent {
                                                clipRect(right = size.width / 2f) {
                                                    this@drawWithContent.drawContent()
                                                }
                                            }
                                            .then(contentMirrorMod)
                                    ) {
                                        content(bottomIndex)
                                    }
                                }
                            }
                        }
                    } else {
                        // Idle: full content, no curl
                        content(updatedCurrent)
                    }
                }
            } else {
                // Original single-page rendering
                key(updatedCurrent) {
                    if (updatedCurrent + 1 < state.max) {
                        content(updatedCurrent + 1)
                    }

                    if (updatedCurrent < state.max) {
                        val forward = internalState.forward.value
                        Box(Modifier.drawCurl(updatedConfig, forward.top, forward.bottom)) {
                            content(updatedCurrent)
                        }
                    }

                    if (updatedCurrent > 0) {
                        val backward = internalState.backward.value
                        Box(Modifier.drawCurl(updatedConfig, backward.top, backward.bottom)) {
                            content(updatedCurrent - 1)
                        }
                    }
                }
            }
        }
    }
}

/**
 * Shows the pages which may be turned by drag or tap gestures.
 *
 * @param count The count of pages.
 * @param key The lambda to provide stable key for each item. Useful when adding and removing items before current page.
 * @param modifier The modifier for this composable.
 * @param state The state of the PageCurl. Use this to programmatically change the current page or observe changes.
 * @param config The configuration for PageCurl.
 * @param content The content lambda to provide the page composable. Receives the page number.
 */
@ExperimentalPageCurlApi
@Composable
public fun PageCurl(
    count: Int,
    key: (Int) -> Any,
    modifier: Modifier = Modifier,
    state: PageCurlState = rememberPageCurlState(),
    config: PageCurlConfig = rememberPageCurlConfig(),
    content: @Composable (Int) -> Unit
) {
    var lastKey by remember(state.current) { mutableStateOf(if (count > 0) key(state.current) else null) }

    remember(count) {
        val newKey = if (count > 0) key(state.current) else null
        if (newKey != lastKey) {
            val index = List(count, key).indexOf(lastKey).coerceIn(0, count - 1)
            lastKey = newKey
            state.current = index
        }
        count
    }

    PageCurl(
        count = count,
        state = state,
        config = config,
        content = content,
        modifier = modifier,
    )
}

/**
 * Shows the pages which may be turned by drag or tap gestures.
 *
 * @param state The state of the PageCurl. Use this to programmatically change the current page or observe changes.
 * @param modifier The modifier for this composable.
 * @param content The content lambda to provide the page composable. Receives the page number.
 */
@ExperimentalPageCurlApi
@Composable
@Deprecated("Specify 'max' as 'count' in PageCurl composable.")
public fun PageCurl(
    state: PageCurlState,
    modifier: Modifier = Modifier,
    content: @Composable (Int) -> Unit
) {
    PageCurl(
        count = state.max,
        state = state,
        modifier = modifier,
        content = content,
    )
}
