package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.zhongbai233.epub.reader.model.*

// ─── 滚动模式 ───

@Composable
internal fun ScrollModeContent(
    chapter: Chapter,
    fontSize: Float,
    textColor: Color,
    linkColor: Color,
    bgColor: Color,
    fontFamily: FontFamily,
    onLinkClick: (String) -> Unit,
    onTextTapped: () -> Unit,
    onOverscrollDown: () -> Unit = {},
    lineSpacing: Float = 1.5f,
    paraSpacing: Float = 0.5f,
    textIndent: Int = 2,
    titleFontScale: Float = 1.5f,
    textSelection: TextSelectionState? = null,
    onSelectionChange: (TextSelectionState?) -> Unit = {},
    blockLayoutRegistry: MutableMap<Int, BlockLayoutInfo>? = null,
    highlights: List<HighlightDto> = emptyList(),
    cscBlockCorrections: Map<Int, List<CscBlockCorrection>> = emptyMap(),
    cscMode: String = "none",
    ttsCurrentBlock: Int = -1,
    onCscCorrectionClick: (CscBlockCorrection, Offset) -> Unit = { _, _ -> }
) {
    val listState = rememberLazyListState()
    val showChapterTitle = remember(chapter) { shouldRenderChapterTitle(chapter) }
    val configuration = LocalConfiguration.current
    val scrollDensity = LocalDensity.current
    val hPaddingDp = configuration.screenWidthDp.dp * 0.065f
    val topPaddingDp = configuration.screenHeightDp.dp * 0.06f
    val bottomPaddingDp = configuration.screenHeightDp.dp * 0.03f
    val scrollContentWidthPx = with(scrollDensity) { (configuration.screenWidthDp.dp - hPaddingDp * 2f).toPx() }
    val scrollSpToPx = scrollDensity.fontScale * scrollDensity.density

    // 下拉书签手势: 在列表顶部时检测下拉
    var pullTotal by remember { mutableFloatStateOf(0f) }
    var pullTriggered by remember { mutableStateOf(false) }
    val pullThreshold = with(scrollDensity) { 120.dp.toPx() }

    val nestedScrollConnection = remember {
        object : androidx.compose.ui.input.nestedscroll.NestedScrollConnection {
            override fun onPreScroll(
                available: Offset,
                source: androidx.compose.ui.input.nestedscroll.NestedScrollSource
            ): Offset {
                // 向上滚回时重置
                if (available.y < 0 && pullTotal > 0) {
                    pullTotal = (pullTotal + available.y).coerceAtLeast(0f)
                    return Offset(0f, available.y)
                }
                return Offset.Zero
            }

            override fun onPostScroll(
                consumed: Offset,
                available: Offset,
                source: androidx.compose.ui.input.nestedscroll.NestedScrollSource
            ): Offset {
                if (available.y > 0 && !pullTriggered) {
                    pullTotal += available.y
                    if (pullTotal > pullThreshold) {
                        pullTriggered = true
                        onOverscrollDown()
                    }
                }
                return Offset.Zero
            }

            override suspend fun onPostFling(
                consumed: androidx.compose.ui.unit.Velocity,
                available: androidx.compose.ui.unit.Velocity
            ): androidx.compose.ui.unit.Velocity {
                pullTotal = 0f
                pullTriggered = false
                return super.onPostFling(consumed, available)
            }
        }
    }

    LazyColumn(
        state = listState,
        modifier = Modifier
            .fillMaxSize()
            .background(bgColor)
            .nestedScroll(nestedScrollConnection),
        contentPadding = PaddingValues(start = hPaddingDp, end = hPaddingDp, top = topPaddingDp, bottom = bottomPaddingDp)
    ) {
        if (showChapterTitle) {
            // 章节标题
            item {
                Text(
                    text = breakTitleIntoLines(chapter.title, scrollContentWidthPx, fontSize * titleFontScale, scrollSpToPx),
                    style = TextStyle(
                        fontSize = (fontSize * titleFontScale).sp,
                        lineHeight = (fontSize * titleFontScale * 1.45f).sp,
                        fontWeight = FontWeight.Bold,
                        fontFamily = fontFamily,
                        color = textColor,
                        textAlign = TextAlign.Center
                    ),
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(top = topPaddingDp * 0.5f, bottom = topPaddingDp * 2.0f)
                )
            }
        }

        // 内容块
        itemsIndexed(chapter.blocks) { blockIndex, block ->
            ContentBlockView(
                blockIndex = blockIndex,
                block = block,
                fontSize = fontSize,
                textColor = textColor,
                linkColor = linkColor,
                bgColor = bgColor,
                fontFamily = fontFamily,
                onLinkClick = onLinkClick,
                onTextTapped = onTextTapped,
                lineSpacing = lineSpacing,
                paraSpacing = paraSpacing,
                textIndentChars = textIndent,
                titleFontScale = titleFontScale,
                textSelection = textSelection,
                onSelectionChange = onSelectionChange,
                blockLayoutRegistry = blockLayoutRegistry,
                highlights = highlights,
                cscBlockCorrections = cscBlockCorrections[blockIndex] ?: emptyList(),
                cscMode = cscMode,
                ttsCurrentBlock = ttsCurrentBlock,
                onCscCorrectionClick = onCscCorrectionClick
            )
        }

        // 底部留白
        item { Spacer(Modifier.height(64.dp)) }
    }
}
