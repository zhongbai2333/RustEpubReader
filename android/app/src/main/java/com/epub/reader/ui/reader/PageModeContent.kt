package com.zhongbai233.epub.reader.ui.reader

import android.util.Log
import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.zIndex
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.model.*
import eu.wewox.pagecurl.ExperimentalPageCurlApi
import eu.wewox.pagecurl.config.PageCurlConfig
import eu.wewox.pagecurl.config.rememberPageCurlConfig
import eu.wewox.pagecurl.page.PageCurl
import eu.wewox.pagecurl.page.rememberPageCurlState
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.util.Collections
import java.util.LinkedHashMap
import kotlin.math.absoluteValue
import kotlin.math.ceil

// ─── 页面交互常量 ───
private const val CHROME_INSET_RATIO = 0.12f
private const val TAP_ZONE_RATIO = 1f / 3f
private const val FLIP_COOLDOWN_MS = 300L
private const val SELECTION_TAP_SUPPRESS_MS = 700L

// ─── 翻页模式 ───

@Composable
@OptIn(ExperimentalPageCurlApi::class)
internal fun PageModeContent(
    chapter: Chapter,
    currentChapter: Int,
    totalChapters: Int,
    allChapters: List<Chapter>,
    fontSize: Float,
    textColor: Color,
    linkColor: Color,
    bgColor: Color,
    fontFamily: FontFamily,
    pageAnimation: String,
    controlsVisible: Boolean,
    settingsVisible: Boolean,
    immersiveStatusText: String? = null,
    immersiveBatteryPercent: Int? = null,
    startAtLastPageRef: BooleanArray = booleanArrayOf(false),
    onPrevChapter: () -> Unit,
    onNextChapter: () -> Unit,
    onToggleControls: () -> Unit,
    onLinkClick: (String) -> Unit,
    onTextTapped: () -> Unit,
    lineSpacing: Float = 1.5f,
    paraSpacing: Float = 0.5f,
    textIndent: Int = 2,
    titleFontScale: Float = 1.5f,
    textSelection: TextSelectionState? = null,
    onSelectionChange: (TextSelectionState?) -> Unit = {},
    blockLayoutRegistry: MutableMap<Int, BlockLayoutInfo>? = null,
    onSelectionDragStart: (Offset) -> Unit = {},
    onSelectionDrag: (Offset) -> Unit = {},
    onSelectionDragEnd: () -> Unit = {},
    onSelectionDragCancel: () -> Unit = {},
    highlights: List<HighlightDto> = emptyList(),
    cscBlockCorrections: Map<Int, List<CscBlockCorrection>> = emptyMap(),
    cscMode: String = "none",
    ttsCurrentBlock: Int = -1,
    onCscCorrectionClick: (CscBlockCorrection, Offset) -> Unit = { _, _ -> }
) {
    val configuration = LocalConfiguration.current
    val density = LocalDensity.current
    val screenHeightDp = configuration.screenHeightDp.dp
    val screenWidthDp = configuration.screenWidthDp.dp
    // 按屏幕比例计算边距，适配不同尺寸设备
    val isTwoColumn = screenWidthDp > 600.dp // 平板或宽屏双列模式
    val hPaddingDp = screenWidthDp * 0.065f      // 左右各约 6.5%
    val topPaddingDp = screenHeightDp * 0.075f   // 顶部留白略大
    val bottomPaddingDp = screenHeightDp * 0.035f // 底部留白（缩小以便节省空间）
    val titleVPaddingDp = topPaddingDp * 2.5f    // 标题上下合计（上 0.5x + 下 2.0x，加大正文间距）
    val availableHeightDp = (screenHeightDp - topPaddingDp - bottomPaddingDp).coerceAtLeast(280.dp)
    
    val contentWidthDp = if (isTwoColumn) {
        ((screenWidthDp - hPaddingDp * 3f) / 2f).coerceAtLeast(180.dp)
    } else {
        (screenWidthDp - hPaddingDp * 2f).coerceAtLeast(180.dp)
    }
    val contentWidthPx = with(density) { contentWidthDp.toPx() }
    val spToPx = density.fontScale * density.density
    val showChapterTitle = remember(chapter) { shouldRenderChapterTitle(chapter) }

    // 预加载缓存（以章节索引为 key，布局参数变化时清空，LRU 限制最多 10 章防止 OOM）
    val paginationCache = remember { lruCache<Int, List<List<ContentBlock>>>(PAGINATION_CACHE_MAX_SIZE) }
    val layoutTag = "$fontSize-${availableHeightDp.value}-${contentWidthDp.value}-$lineSpacing-$paraSpacing-$textIndent-$titleFontScale"
    val prevLayoutTag = remember { mutableStateOf(layoutTag) }
    if (prevLayoutTag.value != layoutTag) {
        prevLayoutTag.value = layoutTag
        paginationCache.clear()
    }

    // 将内容分页（优先从缓存取，避免主线程重复计算）
    val pages = remember(currentChapter, fontSize, availableHeightDp, contentWidthDp, showChapterTitle, lineSpacing, paraSpacing, textIndent, titleFontScale) {
        paginationCache.getOrPut(currentChapter) {
            paginateContent(chapter, fontSize, availableHeightDp, contentWidthDp, density, showChapterTitle, titleVPaddingDp, lineSpacing, paraSpacing, textIndent, titleFontScale)
        }
    }
    
    val pairedPages = remember(pages, isTwoColumn) {
        if (isTwoColumn) pages.chunked(2) else pages.map { listOf(it) }
    }

    // 预加载相邻章节，消除跨章翻页时的白屏闪烁
    LaunchedEffect(currentChapter, fontSize, availableHeightDp, contentWidthDp, lineSpacing, paraSpacing, textIndent, titleFontScale) {
        withContext(Dispatchers.Default) {
            for (adjIdx in listOf(currentChapter - 1, currentChapter + 1)) {
                val adjChapter = allChapters.getOrNull(adjIdx) ?: continue
                paginationCache.getOrPut(adjIdx) {
                    val adjShowTitle = shouldRenderChapterTitle(adjChapter)
                    paginateContent(adjChapter, fontSize, availableHeightDp, contentWidthDp, density, adjShowTitle, titleVPaddingDp, lineSpacing, paraSpacing, textIndent, titleFontScale)
                }
            }
        }
    }

    val hasPrevChapter = currentChapter > 0
    val hasNextChapter = currentChapter < totalChapters - 1
    val leadingVirtual = if (hasPrevChapter) 1 else 0
    val trailingVirtual = if (hasNextChapter) 1 else 0
    val totalSlots = (pairedPages.size + leadingVirtual + trailingVirtual).coerceAtLeast(1)

    val pagerState = rememberPagerState(pageCount = { totalSlots })
    val pageCurlState = rememberPageCurlState()
    val bookSpreadState = com.epub.reader.ui.pagecurl.rememberBookSpreadState()
    val bookSpreadPageCurlState = rememberPageCurlState()
    val isBookSpread = isTwoColumn && pageAnimation == "Realistic"
    val coroutineScope = rememberCoroutineScope()
    // 初始值 true：防止首次挂载时边界检测意外触发
    var chapterJumpTriggered by remember { mutableStateOf(true) }
    var prevChapterKey by remember { mutableIntStateOf(currentChapter) }
    val chapterAlpha = remember { Animatable(1f) }
    // 跟踪当前翻页动画 Job，跨章时取消残留协程防止快速翻页导致状态错乱
    var flipJob by remember { mutableStateOf<Job?>(null) }
    // 强制限制连续翻页间隔，防止过快卡死
    val lastFlipTime = remember { mutableLongStateOf(0L) }


    // 保持回调引用最新 (用于 pointerInput 内部)
    val currentOnPrevChapter by rememberUpdatedState(onPrevChapter)
    val currentOnNextChapter by rememberUpdatedState(onNextChapter)
    val currentOnToggleControls by rememberUpdatedState(onToggleControls)
    val currentOnTextTapped by rememberUpdatedState(onTextTapped)

    val currentHasPrevChapter by rememberUpdatedState(hasPrevChapter)
    val currentHasNextChapter by rememberUpdatedState(hasNextChapter)
    val currentLeadingVirtual by rememberUpdatedState(leadingVirtual)
    val currentTrailingVirtual by rememberUpdatedState(trailingVirtual)
    val currentPairedPages by rememberUpdatedState(pairedPages)
    val currentSettingsVisible by rememberUpdatedState(settingsVisible)
    val currentControlsVisible by rememberUpdatedState(controlsVisible)
    val currentTextSelection by rememberUpdatedState(textSelection)
    val suppressPageTapUntil = remember { mutableLongStateOf(0L) }

    fun markSelectionGesture() {
        suppressPageTapUntil.longValue = System.currentTimeMillis() + SELECTION_TAP_SUPPRESS_MS
    }

    fun consumeSelectionTapIfNeeded(): Boolean {
        if (System.currentTimeMillis() < suppressPageTapUntil.longValue) return true
        if (currentTextSelection == null) return false
        if (currentTextSelection != null) {
            currentOnTextTapped()
        }
        return true
    }

    val pageCurlConfig = rememberPageCurlConfig(
        backPageColor = bgColor,
        dragInteraction = PageCurlConfig.GestureDragInteraction(
            pointerBehavior = PageCurlConfig.DragInteraction.PointerBehavior.PageEdge
        ),
        onCustomTap = { size, position ->
            if (consumeSelectionTapIfNeeded()) {
                return@rememberPageCurlConfig true
            }

            if (currentSettingsVisible) {
                return@rememberPageCurlConfig true
            }

            val chromeInset = size.height * CHROME_INSET_RATIO
            if (currentControlsVisible) {
                if (position.y < chromeInset || position.y > size.height - chromeInset) {
                    return@rememberPageCurlConfig true
                }
                currentOnToggleControls()
                return@rememberPageCurlConfig true
            }

            val tapZone = size.width * TAP_ZONE_RATIO
            val firstReadableSlot = currentLeadingVirtual
            val lastReadableSlot = currentLeadingVirtual + currentPairedPages.lastIndex
            when {
                position.x < tapZone -> {
                    val now = System.currentTimeMillis()
                    if (now - lastFlipTime.longValue < FLIP_COOLDOWN_MS) return@rememberPageCurlConfig true
                    lastFlipTime.longValue = now

                    if (pageCurlState.current <= firstReadableSlot) {
                        if (currentHasPrevChapter && currentLeadingVirtual > 0) {
                            flipJob?.cancel()
                            flipJob = coroutineScope.launch { pageCurlState.prev() }
                        } else {
                            currentOnPrevChapter()
                        }
                    } else {
                        flipJob?.cancel()
                        flipJob = coroutineScope.launch { pageCurlState.prev() }
                    }
                    true
                }
                position.x > size.width - tapZone -> {
                    val now = System.currentTimeMillis()
                    if (now - lastFlipTime.longValue < FLIP_COOLDOWN_MS) return@rememberPageCurlConfig true
                    lastFlipTime.longValue = now

                    if (pageCurlState.current >= lastReadableSlot) {
                        if (currentHasNextChapter && currentTrailingVirtual > 0) {
                            flipJob?.cancel()
                            flipJob = coroutineScope.launch { pageCurlState.next() }
                        } else {
                            currentOnNextChapter()
                        }
                    } else {
                        flipJob?.cancel()
                        flipJob = coroutineScope.launch { pageCurlState.next() }
                    }
                    true
                }
                else -> {
                    currentOnToggleControls()
                    true
                }
            }
        }
    )

    // rememberPageCurlConfig uses rememberSaveable internally, so the initial
    // backPageColor is only applied once.  Force-sync whenever bgColor changes.
    pageCurlConfig.backPageColor = bgColor

    // ─── Book Spread 3D PageCurl Config ───
    val bookSpreadCurlConfig = rememberPageCurlConfig(
        isBookSpread = true,
        backPageColor = bgColor,
        backPageContentAlpha = 0.25f,
        shadowAlpha = 0.35f,
        dragInteraction = PageCurlConfig.GestureDragInteraction(
            pointerBehavior = PageCurlConfig.DragInteraction.PointerBehavior.PageEdge
        ),
        onCustomTap = { size, position ->
            if (consumeSelectionTapIfNeeded()) return@rememberPageCurlConfig true
            if (currentSettingsVisible) return@rememberPageCurlConfig true
            val chromeInset = size.height * CHROME_INSET_RATIO
            if (currentControlsVisible) {
                if (position.y < chromeInset || position.y > size.height - chromeInset) {
                    return@rememberPageCurlConfig true
                }
                currentOnToggleControls()
                return@rememberPageCurlConfig true
            }
            val tapZone = size.width * TAP_ZONE_RATIO
            val spineCenter = size.width / 2f
            when {
                position.x < tapZone -> {
                    val now = System.currentTimeMillis()
                    if (now - lastFlipTime.longValue < FLIP_COOLDOWN_MS) return@rememberPageCurlConfig true
                    lastFlipTime.longValue = now
                    if (bookSpreadPageCurlState.current > 0) {
                        flipJob?.cancel()
                        flipJob = coroutineScope.launch { bookSpreadPageCurlState.prev() }
                    }
                    true
                }
                position.x > spineCenter -> {
                    // Tap anywhere on the right page (past spine) → forward
                    val now = System.currentTimeMillis()
                    if (now - lastFlipTime.longValue < FLIP_COOLDOWN_MS) return@rememberPageCurlConfig true
                    lastFlipTime.longValue = now
                    if (bookSpreadPageCurlState.current < totalSlots - 1) {
                        flipJob?.cancel()
                        flipJob = coroutineScope.launch { bookSpreadPageCurlState.next() }
                    }
                    true
                }
                else -> {
                    currentOnToggleControls()
                    true
                }
            }
        }
    )
    bookSpreadCurlConfig.backPageColor = bgColor

    // 章节切换时重置页码
    LaunchedEffect(currentChapter, pageAnimation) {
        val isRealChapterChange = prevChapterKey != currentChapter
        // 立即阻断边界检测，防止切换期间级联跳章
        chapterJumpTriggered = true
        // 取消残留的翻页动画协程
        flipJob?.cancel()
        flipJob = null
        // 清空 block 注册表，防止初始渲染时错误页面的 block 残留
        blockLayoutRegistry?.clear()
        val isGoingBack = startAtLastPageRef[0]
        val targetSlot = if (isGoingBack) {
            startAtLastPageRef[0] = false
            leadingVirtual + pairedPages.lastIndex.coerceAtLeast(0)
        } else {
            leadingVirtual
        }
        when {
            isRealChapterChange && pageAnimation == "Slide" -> {
                // Slide 模式已经通过 HorizontalPager 的虚拟相邻章节页完成了跨章滑动。
                // 章节状态切换后只需要把 pager 立即归位到新章节目标页；
                // 如果这里再做整章 translationX 动画，会出现“滑入新章 -> 弹回 -> 空白页再滑一次”。
                chapterAlpha.snapTo(1f)
                pagerState.scrollToPage(targetSlot)
                prevChapterKey = currentChapter
            }
            isRealChapterChange && pageAnimation == "Realistic" -> {
                if (isBookSpread) {
                    bookSpreadPageCurlState.snapTo(targetSlot)
                } else {
                    pageCurlState.snapTo(targetSlot)
                }
                prevChapterKey = currentChapter
            }
            isRealChapterChange && pageAnimation == "Cover" -> {
                pagerState.scrollToPage(targetSlot)
                prevChapterKey = currentChapter
            }
            isRealChapterChange -> {
                chapterAlpha.snapTo(1f)
                pagerState.scrollToPage(targetSlot)
                prevChapterKey = currentChapter
            }
            else -> {
                chapterAlpha.snapTo(1f)
                if (isBookSpread) {
                    bookSpreadPageCurlState.snapTo(targetSlot)
                } else if (pageAnimation == "Realistic") {
                    pageCurlState.snapTo(targetSlot)
                } else if (pagerState.currentPage != targetSlot) {
                    pagerState.scrollToPage(targetSlot)
                }
                prevChapterKey = currentChapter
            }
        }
        // 定位完成后解锁边界检测
        chapterJumpTriggered = false
    }

    // 翻页到边界时，自动跨章节
    LaunchedEffect(currentChapter, pageAnimation, hasPrevChapter, hasNextChapter, totalSlots, isBookSpread) {
        if (isBookSpread) {
            snapshotFlow { bookSpreadPageCurlState.current }
                .collect { currentSpread ->
                    if (!chapterJumpTriggered) {
                        if (hasPrevChapter && currentSpread <= 0) {
                            chapterJumpTriggered = true
                            currentOnPrevChapter()
                        } else if (hasNextChapter && currentSpread >= totalSlots - 1) {
                            chapterJumpTriggered = true
                            currentOnNextChapter()
                        }
                    }
                }
        } else if (pageAnimation == "Realistic") {
            snapshotFlow { pageCurlState.current }
                .collect { currentSlot ->
                    if (!chapterJumpTriggered) {
                        if (hasPrevChapter && currentSlot <= 0) {
                            chapterJumpTriggered = true
                            currentOnPrevChapter()
                        } else if (hasNextChapter && currentSlot >= totalSlots - 1) {
                            chapterJumpTriggered = true
                            currentOnNextChapter()
                        }
                    }
                }
        } else {
            snapshotFlow {
                pagerState.currentPage to pagerState.isScrollInProgress
            }.collect { (currentSlot, isScrolling) ->
                if (!isScrolling && !chapterJumpTriggered) {
                    if (hasPrevChapter && currentSlot <= 0) {
                        chapterJumpTriggered = true
                        currentOnPrevChapter()
                    } else if (hasNextChapter && currentSlot >= totalSlots - 1) {
                        chapterJumpTriggered = true
                        currentOnNextChapter()
                    }
                }
            }
        }
    }

    val currentOnSelectionDragStart by rememberUpdatedState(onSelectionDragStart)
    val currentOnSelectionDrag by rememberUpdatedState(onSelectionDrag)
    val currentOnSelectionDragEnd by rememberUpdatedState(onSelectionDragEnd)
    val currentOnSelectionDragCancel by rememberUpdatedState(onSelectionDragCancel)

    Column(
        modifier = Modifier
            .fillMaxSize()
            .graphicsLayer {
                alpha = chapterAlpha.value
            }
            .pointerInput(blockLayoutRegistry, isBookSpread, pageAnimation) {
                if (!isBookSpread && pageAnimation != "Realistic") return@pointerInput
                detectDragGesturesAfterLongPress(
                    onDragStart = {
                        markSelectionGesture()
                        currentOnSelectionDragStart(it)
                    },
                    onDrag = { change, _ ->
                        markSelectionGesture()
                        currentOnSelectionDrag(change.position)
                        change.consume()
                    },
                    onDragEnd = {
                        markSelectionGesture()
                        currentOnSelectionDragEnd()
                    },
                    onDragCancel = {
                        markSelectionGesture()
                        currentOnSelectionDragCancel()
                    }
                )
            }
            .pointerInput(pageAnimation, controlsVisible, settingsVisible) {
                if (pageAnimation == "Realistic") {
                    return@pointerInput
                }
                detectTapGestures(
                    onLongPress = { markSelectionGesture() },
                ) { offset ->
                    if (consumeSelectionTapIfNeeded()) {
                        return@detectTapGestures
                    }

                    if (settingsVisible) {
                        return@detectTapGestures
                    }

                    val chromeInset = with(density) { 96.dp.toPx() }
                    if (controlsVisible) {
                        // 控制栏显示时，忽略顶部/底部区域，避免点击工具栏穿透触发翻页
                        if (offset.y < chromeInset || offset.y > size.height - chromeInset) {
                            return@detectTapGestures
                        }
                    }

                    val screenWidth = size.width
                    val tapZone = if (controlsVisible) screenWidth * 0.24f else screenWidth * TAP_ZONE_RATIO
                    val now = System.currentTimeMillis()
                    when {
                        offset.x < tapZone -> {
                            if (now - lastFlipTime.longValue < FLIP_COOLDOWN_MS) return@detectTapGestures
                            lastFlipTime.longValue = now
                            // 左侧点击 — 上一页
                            flipJob?.cancel()
                            flipJob = coroutineScope.launch {
                                if (pagerState.currentPage > 0) {
                                    if (pageAnimation == "None") {
                                        pagerState.scrollToPage(pagerState.currentPage - 1)
                                    } else {
                                        pagerState.animateScrollToPage(pagerState.currentPage - 1)
                                    }
                                } else {
                                    currentOnPrevChapter()
                                }
                            }
                        }
                        offset.x > screenWidth - tapZone -> {
                            if (now - lastFlipTime.longValue < FLIP_COOLDOWN_MS) return@detectTapGestures
                            lastFlipTime.longValue = now
                            // 右侧点击 — 下一页
                            flipJob?.cancel()
                            flipJob = coroutineScope.launch {
                                if (pagerState.currentPage < pagerState.pageCount - 1) {
                                    if (pageAnimation == "None") {
                                        pagerState.scrollToPage(pagerState.currentPage + 1)
                                    } else {
                                        pagerState.animateScrollToPage(pagerState.currentPage + 1)
                                    }
                                } else {
                                    currentOnNextChapter()
                                }
                            }
                        }
                        else -> {
                            // 中间点击 — 切换控制栏；控制栏显示时，确保不抢左右翻页
                            currentOnToggleControls()
                        }
                    }
                }
            }
    ) {
        if (isBookSpread) {
            // ─── 双列书脊翻页模式（平板专用）───
            PageCurl(
                count = totalSlots,
                state = bookSpreadPageCurlState,
                config = bookSpreadCurlConfig,
                modifier = Modifier
                    .weight(1f)
                    .background(bgColor),
            ) { spreadIndex ->
                val actualSpreadIndex = spreadIndex - leadingVirtual
                val isLeadingVirtual = leadingVirtual > 0 && actualSpreadIndex < 0
                val isTrailingVirtual = trailingVirtual > 0 && actualSpreadIndex >= pairedPages.size

                val slotColumns: List<List<ContentBlock>>
                val slotTitle: String
                val slotShowTitle: Boolean
                val slotPageLabel: String
                
                val isTransitioning = prevChapterKey != currentChapter
                val isGoingBack = currentChapter < prevChapterKey

                fun getPageLabelLocal(columns: List<List<ContentBlock>>, startIdx: Int, totalP: Int): String {
                    if (columns.isEmpty()) return ""
                    if (columns.size == 1) return I18n.tf2("reader.page_info", "${startIdx + 1}", "$totalP")
                    return I18n.tf2("reader.page_info", "${startIdx + 1}-${startIdx + 2}", "$totalP")
                }

                if (isTransitioning) {
                    if (isGoingBack) {
                        slotColumns = pairedPages.lastOrNull() ?: emptyList()
                        slotTitle = chapter.title
                        slotShowTitle = pairedPages.size == 1 && showChapterTitle
                        val startIdx = if (pages.isEmpty()) 0 else if (pages.size % 2 == 0) pages.size - 2 else pages.size - 1
                        slotPageLabel = getPageLabelLocal(slotColumns, startIdx, pages.size)
                    } else {
                        slotColumns = pairedPages.firstOrNull() ?: emptyList()
                        slotTitle = chapter.title
                        slotShowTitle = showChapterTitle
                        val startIdx = 0
                        slotPageLabel = getPageLabelLocal(slotColumns, startIdx, pages.size)
                    }
                } else if (isLeadingVirtual) {
                    val pCh = allChapters.getOrNull(currentChapter - 1)
                    val pPg = paginationCache[currentChapter - 1] ?: emptyList()
                    val pPaired = if (isTwoColumn) pPg.chunked(2) else pPg.map { listOf(it) }
                    slotColumns = pPaired.lastOrNull() ?: emptyList()
                    slotTitle = pCh?.title ?: ""
                    slotShowTitle = pCh != null && pPaired.size == 1 && shouldRenderChapterTitle(pCh)
                    val startIdx = if (pPg.isEmpty()) 0 else if (isTwoColumn) {
                        if (pPg.size % 2 == 0) pPg.size - 2 else pPg.size - 1
                    } else pPg.size - 1
                    slotPageLabel = getPageLabelLocal(slotColumns, startIdx, pPg.size)
                } else if (isTrailingVirtual) {
                    val nCh = allChapters.getOrNull(currentChapter + 1)
                    val nPg = paginationCache[currentChapter + 1] ?: emptyList()
                    val nPaired = if (isTwoColumn) nPg.chunked(2) else nPg.map { listOf(it) }
                    slotColumns = nPaired.firstOrNull() ?: emptyList()
                    slotTitle = nCh?.title ?: ""
                    slotShowTitle = nCh != null && shouldRenderChapterTitle(nCh)
                    val startIdx = 0
                    slotPageLabel = getPageLabelLocal(slotColumns, startIdx, nPg.size)
                } else {
                    slotColumns = pairedPages.getOrNull(actualSpreadIndex) ?: emptyList()
                    slotTitle = chapter.title
                    slotShowTitle = showChapterTitle && actualSpreadIndex == 0
                    val startIdx = if (isTwoColumn) actualSpreadIndex * 2 else actualSpreadIndex
                    slotPageLabel = getPageLabelLocal(slotColumns, startIdx, pages.size)
                }

                PageRenderLayer(
                    slotShowTitle = slotShowTitle,
                    slotTitle = slotTitle,
                    slotColumns = slotColumns,
                    contentWidthPx = contentWidthPx,
                    fontSize = fontSize,
                    spToPx = spToPx,
                    fontFamily = fontFamily,
                    textColor = textColor,
                    linkColor = linkColor,
                    bgColor = bgColor,
                    hPaddingDp = hPaddingDp,
                    topPaddingDp = topPaddingDp,
                    bottomPaddingDp = bottomPaddingDp,
                    slotPageLabel = slotPageLabel,
                    immersiveStatusText = immersiveStatusText,
                    immersiveBatteryPercent = immersiveBatteryPercent,
                    onLinkClick = onLinkClick,
                    onTextTapped = onTextTapped,
                    isTwoColumn = isTwoColumn,
                    lineSpacing = lineSpacing,
                    paraSpacing = paraSpacing,
                    textIndentChars = textIndent,
                    titleFontScale = titleFontScale,
                    chapter = chapter,
                    textSelection = textSelection,
                    onSelectionChange = onSelectionChange,
                    blockLayoutRegistry = if (spreadIndex == bookSpreadPageCurlState.current) blockLayoutRegistry else null,
                    highlights = highlights,
                    cscBlockCorrections = cscBlockCorrections,
                    cscMode = cscMode,
                    ttsCurrentBlock = ttsCurrentBlock,
                    onCscCorrectionClick = onCscCorrectionClick
                )
            }
        } else if (pageAnimation == "Realistic") {
            PageCurl(
                count = totalSlots,
                state = pageCurlState,
                config = pageCurlConfig,
                modifier = Modifier
                    .weight(1f)
                    .background(bgColor)
            ) { pageIndex ->
                val actualPageIndex = pageIndex - leadingVirtual
                val isLeadingVirtual = leadingVirtual > 0 && actualPageIndex < 0
                val isTrailingVirtual = trailingVirtual > 0 && actualPageIndex >= pairedPages.size

                // 虚拟槽显示相邻章节内容，消除跨章空白页
                val slotColumns: List<List<ContentBlock>>
                val slotTitle: String
                val slotShowTitle: Boolean
                val slotPageLabel: String
                
                val isTransitioning = prevChapterKey != currentChapter
                val isGoingBack = currentChapter < prevChapterKey

                fun getPageLabelLocal(columns: List<List<ContentBlock>>, startIdx: Int, totalP: Int): String {
                    if (columns.isEmpty()) return ""
                    if (columns.size == 1) return I18n.tf2("reader.page_info", "${startIdx + 1}", "$totalP")
                    return I18n.tf2("reader.page_info", "${startIdx + 1}-${startIdx + 2}", "$totalP")
                }

                if (isTransitioning) {
                    if (isGoingBack) {
                        slotColumns = pairedPages.lastOrNull() ?: emptyList()
                        slotTitle = chapter.title
                        slotShowTitle = pairedPages.size == 1 && showChapterTitle
                        val startIdx = if (pages.isEmpty()) 0 else if (pages.size % 2 == 0) pages.size - 2 else pages.size - 1
                        slotPageLabel = getPageLabelLocal(slotColumns, startIdx, pages.size)
                    } else {
                        slotColumns = pairedPages.firstOrNull() ?: emptyList()
                        slotTitle = chapter.title
                        slotShowTitle = showChapterTitle
                        val startIdx = 0
                        slotPageLabel = getPageLabelLocal(slotColumns, startIdx, pages.size)
                    }
                } else if (isLeadingVirtual) {
                    val pCh = allChapters.getOrNull(currentChapter - 1)
                    val pPg = paginationCache[currentChapter - 1] ?: emptyList()
                    val pPaired = if (isTwoColumn) pPg.chunked(2) else pPg.map { listOf(it) }
                    slotColumns = pPaired.lastOrNull() ?: emptyList()
                    slotTitle = pCh?.title ?: ""
                    slotShowTitle = pCh != null && pPaired.size == 1 && shouldRenderChapterTitle(pCh)
                    val startIdx = if (pPg.isEmpty()) 0 else if (isTwoColumn) {
                        if (pPg.size % 2 == 0) pPg.size - 2 else pPg.size - 1
                    } else pPg.size - 1
                    slotPageLabel = getPageLabelLocal(slotColumns, startIdx, pPg.size)
                } else if (isTrailingVirtual) {
                    val nCh = allChapters.getOrNull(currentChapter + 1)
                    val nPg = paginationCache[currentChapter + 1] ?: emptyList()
                    val nPaired = if (isTwoColumn) nPg.chunked(2) else nPg.map { listOf(it) }
                    slotColumns = nPaired.firstOrNull() ?: emptyList()
                    slotTitle = nCh?.title ?: ""
                    slotShowTitle = nCh != null && shouldRenderChapterTitle(nCh)
                    val startIdx = 0
                    slotPageLabel = getPageLabelLocal(slotColumns, startIdx, nPg.size)
                } else {
                    slotColumns = pairedPages.getOrNull(actualPageIndex) ?: emptyList()
                    slotTitle = chapter.title
                    slotShowTitle = showChapterTitle && actualPageIndex == 0
                    val startIdx = if (isTwoColumn) actualPageIndex * 2 else actualPageIndex
                    slotPageLabel = getPageLabelLocal(slotColumns, startIdx, pages.size)
                }

                PageRenderLayer(
                    slotShowTitle = slotShowTitle,
                    slotTitle = slotTitle,
                    slotColumns = slotColumns,
                    contentWidthPx = contentWidthPx,
                    fontSize = fontSize,
                    spToPx = spToPx,
                    fontFamily = fontFamily,
                    textColor = textColor,
                    linkColor = linkColor,
                    bgColor = bgColor,
                    hPaddingDp = hPaddingDp,
                    topPaddingDp = topPaddingDp,
                    bottomPaddingDp = bottomPaddingDp,
                    slotPageLabel = slotPageLabel,
                    immersiveStatusText = immersiveStatusText,
                    immersiveBatteryPercent = immersiveBatteryPercent,
                    onLinkClick = onLinkClick,
                    onTextTapped = onTextTapped,
                    lineSpacing = lineSpacing,
                    paraSpacing = paraSpacing,
                    textIndentChars = textIndent,
                    titleFontScale = titleFontScale,
                    chapter = chapter,
                    textSelection = textSelection,
                    onSelectionChange = onSelectionChange,
                    blockLayoutRegistry = if (pageIndex == pageCurlState.current) blockLayoutRegistry else null,
                    highlights = highlights,
                    cscBlockCorrections = cscBlockCorrections,
                    cscMode = cscMode,
                    ttsCurrentBlock = ttsCurrentBlock,
                    onCscCorrectionClick = onCscCorrectionClick
                )
            }
        } else {
            HorizontalPager(
                state = pagerState,
                modifier = Modifier
                    .weight(1f)
                    .background(bgColor)
            ) { pageIndex ->
                val actualPageIndex = pageIndex - leadingVirtual
                val isLeadingVirtual = leadingVirtual > 0 && actualPageIndex < 0
                val isTrailingVirtual = trailingVirtual > 0 && actualPageIndex >= pairedPages.size

                // 虚拟槽显示相邻章节内容
                val slotColumns: List<List<ContentBlock>>
                val slotTitle: String
                val slotShowTitle: Boolean
                val slotPageLabel: String
                
                val isTransitioning = prevChapterKey != currentChapter
                val isGoingBack = currentChapter < prevChapterKey

                fun getPageLabelLocal(columns: List<List<ContentBlock>>, startIdx: Int, totalP: Int): String {
                    if (columns.isEmpty()) return ""
                    if (columns.size == 1) return I18n.tf2("reader.page_info", "${startIdx + 1}", "$totalP")
                    return I18n.tf2("reader.page_info", "${startIdx + 1}-${startIdx + 2}", "$totalP")
                }

                if (isTransitioning) {
                    if (isGoingBack) {
                        slotColumns = pairedPages.lastOrNull() ?: emptyList()
                        slotTitle = chapter.title
                        slotShowTitle = pairedPages.size == 1 && showChapterTitle
                        val startIdx = if (pages.isEmpty()) 0 else if (pages.size % 2 == 0) pages.size - 2 else pages.size - 1
                        slotPageLabel = getPageLabelLocal(slotColumns, startIdx, pages.size)
                    } else {
                        slotColumns = pairedPages.firstOrNull() ?: emptyList()
                        slotTitle = chapter.title
                        slotShowTitle = showChapterTitle
                        val startIdx = 0
                        slotPageLabel = getPageLabelLocal(slotColumns, startIdx, pages.size)
                    }
                } else if (isLeadingVirtual) {
                    val pCh = allChapters.getOrNull(currentChapter - 1)
                    val pPg = paginationCache[currentChapter - 1] ?: emptyList()
                    val pPaired = if (isTwoColumn) pPg.chunked(2) else pPg.map { listOf(it) }
                    slotColumns = pPaired.lastOrNull() ?: emptyList()
                    slotTitle = pCh?.title ?: ""
                    slotShowTitle = pCh != null && pPaired.size == 1 && shouldRenderChapterTitle(pCh)
                    val startIdx = if (pPg.isEmpty()) 0 else if (isTwoColumn) {
                        if (pPg.size % 2 == 0) pPg.size - 2 else pPg.size - 1
                    } else pPg.size - 1
                    slotPageLabel = getPageLabelLocal(slotColumns, startIdx, pPg.size)
                } else if (isTrailingVirtual) {
                    val nCh = allChapters.getOrNull(currentChapter + 1)
                    val nPg = paginationCache[currentChapter + 1] ?: emptyList()
                    val nPaired = if (isTwoColumn) nPg.chunked(2) else nPg.map { listOf(it) }
                    slotColumns = nPaired.firstOrNull() ?: emptyList()
                    slotTitle = nCh?.title ?: ""
                    slotShowTitle = nCh != null && shouldRenderChapterTitle(nCh)
                    val startIdx = 0
                    slotPageLabel = getPageLabelLocal(slotColumns, startIdx, nPg.size)
                } else {
                    slotColumns = pairedPages.getOrNull(actualPageIndex) ?: emptyList()
                    slotTitle = chapter.title
                    slotShowTitle = showChapterTitle && actualPageIndex == 0
                    val startIdx = if (isTwoColumn) actualPageIndex * 2 else actualPageIndex
                    slotPageLabel = getPageLabelLocal(slotColumns, startIdx, pages.size)
                }

                // signedOffset: 正 = 在当前页左侧（旧页）；负 = 在当前页右侧（新页）
                val signedOffset = (pagerState.currentPage - pageIndex) + pagerState.currentPageOffsetFraction
                val absPageOffset = signedOffset.absoluteValue.coerceIn(0f, 1f)
                // 覆盖模式：新页在旧页上方
                val isCoverNewPage = pageAnimation == "Cover" && signedOffset <= 0f

                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .background(bgColor)
                        .pointerInput(blockLayoutRegistry) {
                            detectDragGesturesAfterLongPress(
                                onDragStart = {
                                    markSelectionGesture()
                                    currentOnSelectionDragStart(it)
                                },
                                onDrag = { change, _ ->
                                    markSelectionGesture()
                                    currentOnSelectionDrag(change.position)
                                    change.consume()
                                },
                                onDragEnd = {
                                    markSelectionGesture()
                                    currentOnSelectionDragEnd()
                                },
                                onDragCancel = {
                                    markSelectionGesture()
                                    currentOnSelectionDragCancel()
                                }
                            )
                        }
                        .then(if (isCoverNewPage) Modifier.zIndex(1f) else Modifier)
                        .graphicsLayer {
                            when (pageAnimation) {
                                "Slide" -> {
                                    alpha = 1f - absPageOffset * 0.10f
                                }
                                "Cover" -> {
                                    if (signedOffset > 0f) {
                                        translationX = signedOffset * size.width
                                    }
                                }
                                else -> {
                                    alpha = 1f
                                }
                            }
                        }
                        .then(
                            if (pageAnimation == "Cover" && signedOffset < 0f) {
                                Modifier.drawWithContent {
                                    drawContent()
                                    val shadowW = 20.dp.toPx()
                                    drawRect(
                                        brush = Brush.horizontalGradient(
                                            colors = listOf(Color.Transparent, Color.Black.copy(alpha = 0.28f)),
                                            startX = -shadowW,
                                            endX = 0f
                                        ),
                                        topLeft = Offset(-shadowW, 0f),
                                        size = Size(shadowW, size.height)
                                    )
                                }
                            } else Modifier
                        )
                ) {
                    PageRenderLayer(
                        slotShowTitle = slotShowTitle,
                        slotTitle = slotTitle,
                        slotColumns = slotColumns,
                        contentWidthPx = contentWidthPx,
                        fontSize = fontSize,
                        spToPx = spToPx,
                        fontFamily = fontFamily,
                        textColor = textColor,
                        linkColor = linkColor,
                        bgColor = bgColor,
                        hPaddingDp = hPaddingDp,
                        topPaddingDp = topPaddingDp,
                        bottomPaddingDp = bottomPaddingDp,
                        slotPageLabel = slotPageLabel,
                        immersiveStatusText = immersiveStatusText,
                        immersiveBatteryPercent = immersiveBatteryPercent,
                        onLinkClick = onLinkClick,
                        onTextTapped = onTextTapped,
                        lineSpacing = lineSpacing,
                        paraSpacing = paraSpacing,
                        textIndentChars = textIndent,
                        titleFontScale = titleFontScale,
                    chapter = chapter,
                    textSelection = textSelection,
                    onSelectionChange = onSelectionChange,
                    blockLayoutRegistry = if (pageIndex == pagerState.currentPage) blockLayoutRegistry else null,
                    highlights = highlights,
                    cscBlockCorrections = cscBlockCorrections,
                    cscMode = cscMode,
                    ttsCurrentBlock = ttsCurrentBlock,
                    onCscCorrectionClick = onCscCorrectionClick
                    )
                }
            }
        }
    }
}


@Composable
private fun PageRenderLayer(
    slotShowTitle: Boolean,
    slotTitle: String,
    slotColumns: List<List<ContentBlock>>,
    contentWidthPx: Float,
    fontSize: Float,
    spToPx: Float,
    fontFamily: FontFamily,
    textColor: Color,
    linkColor: Color,
    bgColor: Color,
    hPaddingDp: androidx.compose.ui.unit.Dp,
    topPaddingDp: androidx.compose.ui.unit.Dp,
    bottomPaddingDp: androidx.compose.ui.unit.Dp,
    slotPageLabel: String,
    immersiveStatusText: String? = null,
    immersiveBatteryPercent: Int? = null,
    onLinkClick: (String) -> Unit,
    onTextTapped: () -> Unit,
    isTwoColumn: Boolean = false,
    lineSpacing: Float = 1.5f,
    paraSpacing: Float = 0.5f,
    textIndentChars: Int = 2,
    titleFontScale: Float = 1.5f,
    chapter: Chapter? = null,
    textSelection: TextSelectionState? = null,
    onSelectionChange: (TextSelectionState?) -> Unit = {},
    blockLayoutRegistry: MutableMap<Int, BlockLayoutInfo>? = null,
    highlights: List<HighlightDto> = emptyList(),
    cscBlockCorrections: Map<Int, List<CscBlockCorrection>> = emptyMap(),
    cscMode: String = "none",
    ttsCurrentBlock: Int = -1,
    onCscCorrectionClick: (CscBlockCorrection, Offset) -> Unit = { _, _ -> }
) {
    androidx.compose.foundation.layout.Box(
        modifier = Modifier
            .fillMaxSize()
            .background(bgColor)
            .graphicsLayer { clip = true }
    ) {
        androidx.compose.foundation.layout.Row(
            modifier = Modifier
                .fillMaxSize()
                .padding(top = topPaddingDp, bottom = bottomPaddingDp)
        ) {
            slotColumns.forEachIndexed { index, colBlock ->
                val colShowTitle = if (index == 0) slotShowTitle else false
                androidx.compose.foundation.layout.Column(
                    modifier = Modifier
                        .weight(1f)
                        .fillMaxHeight()
                        .padding(
                            start = if (index == 0) hPaddingDp else hPaddingDp / 2f,
                            end = if (index == slotColumns.lastIndex) hPaddingDp else hPaddingDp / 2f
                        )
                ) {
                    if (colShowTitle) {
                        androidx.compose.material3.Text(
                            text = breakTitleIntoLines(slotTitle, contentWidthPx, fontSize * titleFontScale, spToPx),
                            style = androidx.compose.ui.text.TextStyle(
                                fontSize = (fontSize * titleFontScale).sp,
                                lineHeight = (fontSize * titleFontScale * 1.45f).sp,
                                fontWeight = androidx.compose.ui.text.font.FontWeight.Bold,
                                fontFamily = fontFamily,
                                color = textColor,
                                textAlign = androidx.compose.ui.text.style.TextAlign.Center
                            ),
                            modifier = Modifier
                                .fillMaxWidth()
                                .wrapContentHeight()
                                .padding(top = topPaddingDp * 0.5f, bottom = topPaddingDp * 2.0f)
                        )
                    }
                    colBlock.forEach { block ->
                        val blockIndex = chapter?.blocks?.indexOf(block)
                        ContentBlockView(
                            blockIndex = if(blockIndex != null && blockIndex >= 0) blockIndex else null,
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
                            textIndentChars = textIndentChars,
                            titleFontScale = titleFontScale,
                            textSelection = textSelection,
                            onSelectionChange = onSelectionChange,
                            blockLayoutRegistry = blockLayoutRegistry,
                            highlights = highlights,
                            cscBlockCorrections = if (blockIndex != null && blockIndex >= 0) cscBlockCorrections[blockIndex] ?: emptyList() else emptyList(),
                            cscMode = cscMode,
                            ttsCurrentBlock = ttsCurrentBlock,
                            onCscCorrectionClick = onCscCorrectionClick
                        )
                    }
                }
            }
            if (isTwoColumn && slotColumns.size == 1) {
                androidx.compose.foundation.layout.Spacer(modifier = Modifier.weight(1f))
            }
        }

        if (slotPageLabel.isNotEmpty()) {
            androidx.compose.material3.Text(
                text = slotPageLabel,
                fontSize = 12.sp,
                color = textColor.copy(alpha = 0.38f),
                modifier = Modifier
                    .align(androidx.compose.ui.Alignment.BottomEnd)
                    .padding(end = 16.dp, bottom = 8.dp)
            )
        }

        if (!immersiveStatusText.isNullOrBlank()) {
            ImmersiveStatusBadge(
                text = immersiveStatusText,
                batteryPercent = immersiveBatteryPercent,
                textColor = textColor,
                modifier = Modifier
                    .align(androidx.compose.ui.Alignment.BottomStart)
                    .padding(start = 16.dp, bottom = 8.dp)
            )
        }
    }
}


