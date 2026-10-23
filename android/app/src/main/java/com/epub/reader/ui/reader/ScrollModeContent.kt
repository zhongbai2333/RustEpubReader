package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.ui.layout.positionInRoot
import kotlinx.coroutines.flow.collectLatest

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import com.zhongbai233.epub.reader.i18n.I18n
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.boundsInRoot
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
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first

// ─── 滚动模式 ───

@Composable
internal fun ScrollModeContent(
    chapter: Chapter,
    allChapters: List<Chapter> = listOf(chapter),
    initialChapter: Int = 0,
    onChapterVisible: (Int) -> Unit = {},
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
    onCscCorrectionClick: (CscBlockCorrection, Offset) -> Unit = { _, _ -> },
    initialBlock: Int = 0,
    initialCharOffset: Int = 0,
    positionRevision: Int = 0,
    onPositionChange: (Int,Int) -> Unit = { _,_ -> },
    onPageLabel: (String) -> Unit = {},
    onContentPrepared: () -> Unit = {},
 latinFamily: FontFamily? = null,) {
    val highlightsByChapter = remember(highlights) { highlights.groupBy { it.chapter } }
    var initialRestore by remember { mutableStateOf(true) }
    var restoredSource by remember { mutableStateOf<Chapter?>(null) }
    val listState = rememberLazyListState(initialFirstVisibleItemIndex = initialChapter.coerceIn(0, allChapters.lastIndex.coerceAtLeast(0)))
    val visibleImages=remember(publicationKey(allChapters)) { VisibleReaderImages() }
    val preparedCallback by rememberUpdatedState(onContentPrepared)
    LaunchedEffect(publicationKey(allChapters),initialChapter,positionRevision,chapter) {
        withFrameNanos { }; withFrameNanos { }
        snapshotFlow { visibleImages.ready }.first { it }
        withFrameNanos { }; preparedCallback()
    }
    var suppressChapterCallback by remember { mutableStateOf(false) }
    var lastVisibleChapter by remember { mutableIntStateOf(initialChapter) }
    var lastRestoreRevision by remember { mutableIntStateOf(-1) }
    var userScrollRevision by remember { mutableIntStateOf(0) }
    val configuration = LocalConfiguration.current
    val scrollDensity = LocalDensity.current
    val hPaddingDp = configuration.screenWidthDp.dp * 0.065f
    val topPaddingDp = READER_VERTICAL_PADDING
    val bottomPaddingDp = READER_VERTICAL_PADDING
    val scrollContentWidthPx =
        with(scrollDensity) { (configuration.screenWidthDp.dp - hPaddingDp * 2f).toPx() }
    val scrollSpToPx = scrollDensity.fontScale * scrollDensity.density

    // 下拉书签手势: 在列表顶部时检测下拉
    var pullTotal by remember { mutableFloatStateOf(0f) }
    var pullTriggered by remember { mutableStateOf(false) }
    val pullThreshold = with(scrollDensity) { 120.dp.toPx() }

    LaunchedEffect(publicationKey(allChapters)) {
        suppressChapterCallback = true
        listState.scrollToItem(initialChapter.coerceIn(0, allChapters.lastIndex.coerceAtLeast(0)))
        // Let the programmatic positioning settle before user-driven chapter
        // visibility changes are reported to the parent.
        withFrameNanos {}
        suppressChapterCallback = false
    }

    LaunchedEffect(initialChapter,positionRevision,chapter) {
        if (!initialRestore && initialChapter == lastVisibleChapter && positionRevision == lastRestoreRevision && restoredSource == chapter) return@LaunchedEffect
        restoredSource = chapter
        initialRestore = false
        lastRestoreRevision=positionRevision
        suppressChapterCallback = true
        listState.scrollToItem(initialChapter.coerceIn(0, allChapters.lastIndex.coerceAtLeast(0)))
        lastVisibleChapter = initialChapter
        withFrameNanos {}; withFrameNanos {}
        val info=blockLayoutRegistry?.get(initialBlock)
        if(info?.coordinates?.isAttached==true) {
            val rootY=info.coordinates!!.positionInRoot().y
            val targetY=visibleImages.viewport.top+with(scrollDensity){topPaddingDp.toPx()}
            val line=info.layoutResult.getLineForOffset(initialCharOffset.coerceIn(0,info.text.length))
            val dy=rootY+info.layoutResult.getLineTop(line)-targetY
            listState.scrollBy(dy)
        }
        withFrameNanos {}
        suppressChapterCallback = false
    }

    LaunchedEffect(listState) {
        snapshotFlow { listState.layoutInfo.visibleItemsInfo.firstOrNull()?.index }
            .distinctUntilChanged()
            .collect { chapterIndex ->
                if (!suppressChapterCallback) {
                    chapterIndex?.let {
                        lastVisibleChapter = it
                        onChapterVisible(it)
                    }
                }
            }
    }

    val positionCallback by rememberUpdatedState(onPositionChange)
    val labelCallback by rememberUpdatedState(onPageLabel)
    LaunchedEffect(listState) {
        snapshotFlow {
            val layout = listState.layoutInfo
            val item = layout.visibleItemsInfo.firstOrNull()
            listOf(item?.offset ?: 0, item?.size ?: 0, layout.viewportEndOffset-layout.viewportStartOffset)
        }.distinctUntilChanged().collect { (offset, size, height) ->
            if (size > 0 && height > 0) {
                val total = kotlin.math.ceil(size.toDouble()/height).toInt().coerceAtLeast(1)
                val page = ((-offset).coerceAtLeast(0)/height+1).coerceAtMost(total)
                labelCallback(I18n.tf2("reader.page_info", "$page", "$total"))
            }
        }
    }
    LaunchedEffect(publicationKey(allChapters), initialChapter, positionRevision) {
        var reportedScroll = userScrollRevision
        snapshotFlow { Triple(listState.firstVisibleItemIndex,listState.firstVisibleItemScrollOffset,userScrollRevision) }
            .collectLatest { (_,_,userRevision) ->
                if (suppressChapterCallback || userRevision == reportedScroll) return@collectLatest
                val targetY=visibleImages.viewport.top+with(scrollDensity){topPaddingDp.toPx()}
                val located=blockLayoutRegistry?.entries?.filter {it.value.coordinates?.isAttached==true}
                    ?.sortedBy {it.value.coordinates!!.positionInRoot().y}
                val block=located?.lastOrNull {it.value.coordinates!!.positionInRoot().y<=targetY} ?: located?.firstOrNull()
                if(block!=null) {
                    val info=block.value
                    val y=(targetY-info.coordinates!!.positionInRoot().y).coerceAtLeast(0f)
                    val line=info.layoutResult.getLineForVerticalPosition(y)
                    reportedScroll=userRevision
                    positionCallback(block.key,info.sourceCharOffset+info.layoutResult.getLineStart(line))
                }
            }
    }

    val nestedScrollConnection = remember {
        object : androidx.compose.ui.input.nestedscroll.NestedScrollConnection {
            override fun onPreScroll(
                available: Offset,
                source: androidx.compose.ui.input.nestedscroll.NestedScrollSource,
            ): Offset {
                if (source == androidx.compose.ui.input.nestedscroll.NestedScrollSource.UserInput) userScrollRevision++
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
                source: androidx.compose.ui.input.nestedscroll.NestedScrollSource,
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
                available: androidx.compose.ui.unit.Velocity,
            ): androidx.compose.ui.unit.Velocity {
                pullTotal = 0f
                pullTriggered = false
                return super.onPostFling(consumed, available)
            }
        }
    }

    CompositionLocalProvider(LocalVisibleReaderImages provides visibleImages) {
    LazyColumn(
        state = listState,
        modifier = Modifier.fillMaxSize().background(bgColor).nestedScroll(nestedScrollConnection).onGloballyPositioned { visibleImages.viewport=it.boundsInRoot() },
        contentPadding =
            PaddingValues(
                start = hPaddingDp,
                end = hPaddingDp,
                top = topPaddingDp,
                bottom = bottomPaddingDp,
            ),
    ) {
        // Key generation must not access lazy bodies in Compose's nearby-key window.
        items(count = allChapters.size, key = { index -> "chapter-$index" }) { chapterIndex ->
            val chapterItem = allChapters[chapterIndex]
            Column(
                modifier =
                    Modifier.fillMaxWidth()
                        .padding(top = if (chapterIndex == 0) 0.dp else topPaddingDp)
            ) {
                if (shouldRenderChapterTitle(chapterItem)) {
                    Text(
                        text =
                            breakTitleIntoLines(
                                chapterItem.title,
                                scrollContentWidthPx,
                                fontSize * titleFontScale,
                                scrollSpToPx,
                            ),
                        style =
                            TextStyle(
                                fontSize = (fontSize * titleFontScale).sp,
                                lineHeight = (fontSize * titleFontScale * 1.45f).sp,
                                fontWeight = FontWeight.Bold,
                                fontFamily = fontFamily,
                                color = textColor,
                                textAlign = TextAlign.Center,
                            ),
                        modifier =
                            Modifier.fillMaxWidth()
                                .padding(
                                    top = CHAPTER_TITLE_TOP_PADDING,
                                    bottom = CHAPTER_TITLE_BOTTOM_PADDING,
                                ),
                    )
                }
                chapterItem.blocks.forEachIndexed { blockIndex, block ->
                    chapterItem.images.filter { it.block == blockIndex && !it.after }.forEach { image ->
                        ContentBlockView(chapterIndex = chapterIndex, blockIndex = null, block = image.content(), fontSize = fontSize, textColor = textColor, linkColor = linkColor, bgColor = bgColor, fontFamily = fontFamily, onLinkClick = onLinkClick, onTextTapped = onTextTapped, latinFamily=latinFamily)
                    }
                    ContentBlockView(
                        chapterIndex = chapterIndex,
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
                        blockLayoutRegistry =
                            if (chapterIndex == initialChapter) blockLayoutRegistry else null,
                        highlights = highlightsByChapter[chapterIndex].orEmpty(),
                        cscBlockCorrections =
                            if (chapterIndex == initialChapter)
                                cscBlockCorrections[blockIndex] ?: emptyList()
                            else emptyList(),
                        cscMode = cscMode,
                        ttsCurrentBlock =
                            if (chapterIndex == initialChapter) ttsCurrentBlock else -1,
                        onCscCorrectionClick = onCscCorrectionClick,
                     latinFamily=latinFamily,)
                    chapterItem.images.filter { it.block == blockIndex && it.after }.forEach { image ->
                        ContentBlockView(chapterIndex = chapterIndex, blockIndex = null, block = image.content(), fontSize = fontSize, textColor = textColor, linkColor = linkColor, bgColor = bgColor, fontFamily = fontFamily, onLinkClick = onLinkClick, onTextTapped = onTextTapped, latinFamily=latinFamily)
                    }
                }
                Spacer(Modifier.height(64.dp))
            }
        }
    }
    }
}
