package com.zhongbai233.epub.reader.ui.reader

import android.app.Activity
import android.graphics.BitmapFactory
import androidx.compose.animation.*
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.*
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.ArrowForward
import androidx.compose.material.icons.automirrored.filled.MenuBook
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.zIndex
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.*
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import com.zhongbai233.epub.reader.util.FontItem
import java.io.File
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextIndent
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.style.LineHeightStyle
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import java.util.Collections
import java.util.LinkedHashMap
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.zhongbai233.epub.reader.model.*
import com.zhongbai233.epub.reader.i18n.I18n
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import coil.compose.AsyncImage
import eu.wewox.pagecurl.ExperimentalPageCurlApi
import eu.wewox.pagecurl.config.PageCurlConfig
import eu.wewox.pagecurl.config.rememberPageCurlConfig
import eu.wewox.pagecurl.page.PageCurl
import eu.wewox.pagecurl.page.rememberPageCurlState
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import java.net.URI
import kotlin.math.absoluteValue
import kotlin.math.ceil

// ─── 页面交互常量 ───

/** 上下留白区域占屏幕高度的比例，点击此区域时不触发翻页 */
private const val CHROME_INSET_RATIO = 0.12f
/** 左右点击翻页区域占屏幕宽度的比例（1/3） */
private const val TAP_ZONE_RATIO = 1f / 3f
/** 连续翻页最小间隔（毫秒） */
private const val FLIP_COOLDOWN_MS = 300L
/** 分页缓存最大章节数 */
private const val PAGINATION_CACHE_MAX_SIZE = 10

// ─── CJK 标点禁则 ───

private val NO_BREAK_BEFORE = charArrayOf(
    '，', '。', '！', '？', '；', '：', '、', '"', '\'', '）', '》',
    '」', '』', '】', '〉', '〕', '〗', '〙', '〛', '，', '．',
    '！', '？', '）', '：', '；', '"', '\'', '」', '〉', '》',
    '】', '〗', '〙', '〛', '.', ',', '!', '?', ';', ':', ')',
    ']', '}', '…', '‥', '％', '‰', '℃', 'ー', '〜', '～'
)

private val NO_BREAK_AFTER = charArrayOf(
    '"', '\'', '（', '《', '「', '『', '【', '〈', '〔', '〖',
    '〘', '〚', '（', '"', '\'', '「', '〈', '《', '【', '〖',
    '〘', '〚', '(', '[', '{'
)

/**
 * 阅读器界面 — 对应PC版 render_reader()
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ReaderScreen(
    book: EpubBook,
    currentChapter: Int,
    fontSize: Float,
    isDarkMode: Boolean,
    scrollMode: Boolean,
    bgColorIndex: Int,
    customBgColorArgb: Int,
    fontColorIndex: Int,
    customFontColorArgb: Int,
    fontFamilyName: String,
    pageAnimation: String,
    bgImageUri: String?,
    bgImageAlpha: Float,
    language: String,
    systemFonts: List<FontItem> = emptyList(),
    showToc: Boolean,
    onNavigateBack: () -> Unit,
    onChapterChange: (Int) -> Unit,
    previousChapter: Int?,
    onGoBackChapter: () -> Unit,
    onFontSizeChange: (Float) -> Unit,
    onToggleDarkMode: () -> Unit,
    onToggleScrollMode: () -> Unit,
    onUpdateScrollMode: (Boolean) -> Unit,
    onUpdateDarkMode: (Boolean) -> Unit,
    onUpdateBgColor: (Int) -> Unit,
    onUpdateCustomBgColor: (Int) -> Unit,
    onUpdateFontColor: (Int) -> Unit,
    onUpdateCustomFontColor: (Int) -> Unit,
    onUpdateFontFamily: (String) -> Unit,
    onUpdatePageAnimation: (String) -> Unit,
    onUpdateBgImageAlpha: (Float) -> Unit,
    onUpdateLanguage: (String) -> Unit,
    onOpenBackgroundPicker: () -> Unit,
    onClearBackgroundImage: () -> Unit,
    onToggleToc: () -> Unit
) {
    val chapter = book.chapters.getOrNull(currentChapter)
    val uriHandler = LocalUriHandler.current

    val onLinkClick: (String) -> Unit = { raw ->
        val link = raw.trim()
        if (link.isBlank()) {
            // no-op
        } else {
            val lowered = link.lowercase()
            val isExternal = lowered.startsWith("http://") ||
                lowered.startsWith("https://") ||
                lowered.startsWith("mailto:") ||
                lowered.startsWith("tel:")

            when {
                isExternal -> {
                    runCatching { uriHandler.openUri(link) }
                }

                link.startsWith("#") -> {
                    // 章节内锚点暂不支持精确滚动，先避免误跳外链
                }

                else -> {
                    val normalizedPath = normalizeInternalHref(link)
                    if (normalizedPath.isBlank()) {
                        runCatching { uriHandler.openUri(link) }
                    } else {
                        val target = book.chapters.indexOfFirst { ch ->
                            val src = ch.sourceHref ?: return@indexOfFirst false
                            val srcNorm = normalizeInternalHref(src)
                            srcNorm == normalizedPath ||
                                srcNorm.endsWith("/$normalizedPath") ||
                                normalizedPath.endsWith("/$srcNorm")
                        }

                        if (target >= 0) {
                            onChapterChange(target)
                        } else {
                            runCatching { uriHandler.openUri(link) }
                        }
                    }
                }
            }
        }
    }
    // 控制栏显示/隐藏
    var showControls by remember { mutableStateOf(false) }
    var showSettingsSheet by rememberSaveable { mutableStateOf(false) }
    val startAtLastPageRef = remember { booleanArrayOf(false) }

    // 读取 I18n.version 以确保语言切换时触发重组
    @Suppress("UNUSED_VARIABLE")
    val langVersion = I18n.version

    val bgPalette = remember(langVersion) {
        listOf(
            I18n.t("color.warm_white") to Color(0xFFF5F0E8),
            I18n.t("color.light_gray") to Color(0xFFF1F3F5),
            I18n.t("color.bean_green") to Color(0xFFE8F0E8),
            I18n.t("color.deep_night") to Color(0xFF1A1A1A),
            I18n.t("color.graphite") to Color(0xFF24262B)
        )
    }
    val fontPalette = remember(langVersion) {
        listOf(
            I18n.t("color.auto") to Color.Unspecified,
            I18n.t("color.ink_black") to Color(0xFF1A1A1A),
            I18n.t("color.dark_gray") to Color(0xFF2D2D2D),
            I18n.t("color.light_gray") to Color(0xFFE6E6E6),
            I18n.t("color.cream") to Color(0xFFF1EAD8)
        )
    }

    val customBgColor = Color(customBgColorArgb)
    val customFontColor = Color(customFontColorArgb)
    val selectedBg = when {
        bgColorIndex in bgPalette.indices -> bgPalette[bgColorIndex].second
        bgColorIndex == bgPalette.size -> customBgColor
        else -> if (isDarkMode) Color(0xFF1A1A1A) else Color(0xFFF5F0E8)
    }
    val autoText = if (selectedBg.luminance() < 0.45f) Color(0xFFE8E8E8) else Color(0xFF1A1A1A)
    val selectedFont = when {
        fontColorIndex in fontPalette.indices -> fontPalette[fontColorIndex].second
        fontColorIndex == fontPalette.size -> customFontColor
        else -> Color.Unspecified
    }

    val textColor = if (selectedFont == Color.Unspecified) autoText else selectedFont
    val bgColor = selectedBg
    val linkColor = if (textColor.luminance() < 0.45f) Color(0xFF78B4FF) else Color(0xFF3366CC)

    val fontFamily: FontFamily = remember(fontFamilyName, systemFonts) {
        when (fontFamilyName) {
            "Serif" -> FontFamily.Serif
            "Monospace" -> FontFamily.Monospace
            else -> {
                val item = systemFonts.find { it.displayName == fontFamilyName }
                if (item != null) fontFamilyFromFile(item.path) else FontFamily.SansSerif
            }
        }
    }

    // 沉浸式模式: 隐藏/显示系统栏
    val view = LocalView.current
    LaunchedEffect(showControls) {
        val window = (view.context as? Activity)?.window ?: return@LaunchedEffect
        val controller = WindowCompat.getInsetsController(window, view)
        if (!showControls) {
            controller.hide(WindowInsetsCompat.Type.systemBars())
            controller.systemBarsBehavior =
                WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        } else {
            controller.show(WindowInsetsCompat.Type.systemBars())
        }
    }

    LaunchedEffect(showSettingsSheet) {
        if (showSettingsSheet) {
            showControls = true
        }
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(bgColor)
    ) {
        if (!bgImageUri.isNullOrBlank()) {
            AsyncImage(
                model = bgImageUri,
                contentDescription = I18n.t("reader.bg_image_desc"),
                modifier = Modifier.fillMaxSize(),
                contentScale = ContentScale.Crop,
                alpha = bgImageAlpha
            )
        }

        // 内容层 — 全屏
        if (chapter == null) {
            Box(
                modifier = Modifier.fillMaxSize(),
                contentAlignment = Alignment.Center
            ) {
                Text(I18n.t("reader.no_content"), color = textColor)
            }
        } else if (scrollMode) {
            // 滚动模式: 点击任意处切换控制栏
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .pointerInput(Unit) {
                        detectTapGestures {
                            if (!showSettingsSheet) {
                                showControls = !showControls
                            }
                        }
                    }
            ) {
                ScrollModeContent(
                    chapter = chapter,
                    fontSize = fontSize,
                    textColor = textColor,
                    linkColor = linkColor,
                    bgColor = bgColor,
                    fontFamily = fontFamily,
                    onLinkClick = onLinkClick
                )
            }
        } else {
            // 翻页模式: 左右点击翻页, 中间点击切换控制栏
            PageModeContent(
                chapter = chapter,
                currentChapter = currentChapter,
                totalChapters = book.chapters.size,
                allChapters = book.chapters,
                fontSize = fontSize,
                textColor = textColor,
                linkColor = linkColor,
                bgColor = bgColor,
                fontFamily = fontFamily,
                pageAnimation = pageAnimation,
                controlsVisible = showControls,
                settingsVisible = showSettingsSheet,
                startAtLastPageRef = startAtLastPageRef,
                onPrevChapter = {
                    if (currentChapter > 0) {
                        startAtLastPageRef[0] = true
                        onChapterChange(currentChapter - 1)
                    }
                },
                onNextChapter = {
                    if (currentChapter < book.chapters.size - 1) onChapterChange(currentChapter + 1)
                },
                onToggleControls = {
                    if (!showSettingsSheet) {
                        showControls = !showControls
                    }
                },
                onLinkClick = onLinkClick
            )
        }

        // 顶部控制栏 — 覆盖层 + 动画
        AnimatedVisibility(
            visible = showControls,
            enter = slideInVertically { -it } + fadeIn(),
            exit = slideOutVertically { -it } + fadeOut(),
            modifier = Modifier.align(Alignment.TopCenter)
        ) {
            ReaderTopBar(
                title = book.title,
                chapterTitle = chapter?.title,
                currentChapter = currentChapter,
                totalChapters = book.chapters.size,
                isDarkMode = isDarkMode,
                previousChapter = previousChapter,
                onNavigateBack = onNavigateBack,
                onGoBackChapter = onGoBackChapter,
                onToggleToc = onToggleToc,
                onOpenSettings = {
                    showControls = true
                    showSettingsSheet = true
                },
                onPrevChapter = {
                    if (currentChapter > 0) {
                        startAtLastPageRef[0] = true
                        onChapterChange(currentChapter - 1)
                    }
                },
                onNextChapter = {
                    if (currentChapter < book.chapters.size - 1) onChapterChange(currentChapter + 1)
                }
            )
        }

        // 底部控制栏 — 覆盖层 + 动画
        AnimatedVisibility(
            visible = showControls,
            enter = slideInVertically { it } + fadeIn(),
            exit = slideOutVertically { it } + fadeOut(),
            modifier = Modifier.align(Alignment.BottomCenter)
        ) {
            ReaderBottomBar(
                fontSize = fontSize,
                scrollMode = scrollMode,
                isDarkMode = isDarkMode,
                onFontSizeChange = onFontSizeChange,
                onToggleScrollMode = onToggleScrollMode,
                onToggleDarkMode = onToggleDarkMode
            )
        }

        if (showSettingsSheet) {
            ReaderSettingsSheet(
                fontSize = fontSize,
                scrollMode = scrollMode,
                isDarkMode = isDarkMode,
                bgColorIndex = bgColorIndex,
                customBgColor = customBgColor,
                fontColorIndex = fontColorIndex,
                customFontColor = customFontColor,
                fontFamilyName = fontFamilyName,
                pageAnimation = pageAnimation,
                bgImageEnabled = !bgImageUri.isNullOrBlank(),
                bgImageAlpha = bgImageAlpha,
                language = language,
                systemFonts = systemFonts,
                onDismiss = { showSettingsSheet = false },
                onFontSizeChange = onFontSizeChange,
                onScrollModeChange = onUpdateScrollMode,
                onDarkModeChange = onUpdateDarkMode,
                onBgColorChange = onUpdateBgColor,
                onCustomBgColorChange = { onUpdateCustomBgColor(it.toArgb()) },
                onFontColorChange = onUpdateFontColor,
                onCustomFontColorChange = { onUpdateCustomFontColor(it.toArgb()) },
                onFontFamilyChange = onUpdateFontFamily,
                onPageAnimationChange = onUpdatePageAnimation,
                onBgImageAlphaChange = onUpdateBgImageAlpha,
                onLanguageChange = onUpdateLanguage,
                onPickBackgroundImage = onOpenBackgroundPicker,
                onClearBackgroundImage = onClearBackgroundImage
            )
        }
    }
}

// ─── 滚动模式 ───

@Composable
private fun ScrollModeContent(
    chapter: Chapter,
    fontSize: Float,
    textColor: Color,
    linkColor: Color,
    bgColor: Color,
    fontFamily: FontFamily,
    onLinkClick: (String) -> Unit
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

    LazyColumn(
        state = listState,
        modifier = Modifier
            .fillMaxSize()
            .background(bgColor),
        contentPadding = PaddingValues(start = hPaddingDp, end = hPaddingDp, top = topPaddingDp, bottom = bottomPaddingDp)
    ) {
        if (showChapterTitle) {
            // 章节标题
            item {
                Text(
                    text = breakTitleIntoLines(chapter.title, scrollContentWidthPx, fontSize * 1.5f, scrollSpToPx),
                    style = TextStyle(
                        fontSize = (fontSize * 1.5f).sp,
                        lineHeight = (fontSize * 2.2f).sp,
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
        itemsIndexed(chapter.blocks) { _, block ->
            ContentBlockView(
                block = block,
                fontSize = fontSize,
                textColor = textColor,
                linkColor = linkColor,
                fontFamily = fontFamily,
                onLinkClick = onLinkClick
            )
        }

        // 底部留白
        item { Spacer(Modifier.height(64.dp)) }
    }
}

// ─── 翻页模式 ───

@Composable
@OptIn(ExperimentalPageCurlApi::class)
private fun PageModeContent(
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
    startAtLastPageRef: BooleanArray = booleanArrayOf(false),
    onPrevChapter: () -> Unit,
    onNextChapter: () -> Unit,
    onToggleControls: () -> Unit,
    onLinkClick: (String) -> Unit
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
    val layoutTag = "$fontSize-${availableHeightDp.value}-${contentWidthDp.value}"
    val prevLayoutTag = remember { mutableStateOf(layoutTag) }
    if (prevLayoutTag.value != layoutTag) {
        prevLayoutTag.value = layoutTag
        paginationCache.clear()
    }

    // 将内容分页（优先从缓存取，避免主线程重复计算）
    val pages = remember(currentChapter, fontSize, availableHeightDp, contentWidthDp, showChapterTitle) {
        paginationCache.getOrPut(currentChapter) {
            paginateContent(chapter, fontSize, availableHeightDp, contentWidthDp, density, showChapterTitle, titleVPaddingDp)
        }
    }
    
    val pairedPages = remember(pages, isTwoColumn) {
        if (isTwoColumn) pages.chunked(2) else pages.map { listOf(it) }
    }

    // 预加载相邻章节，消除跨章翻页时的白屏闪烁
    LaunchedEffect(currentChapter, fontSize, availableHeightDp, contentWidthDp) {
        withContext(Dispatchers.Default) {
            for (adjIdx in listOf(currentChapter - 1, currentChapter + 1)) {
                val adjChapter = allChapters.getOrNull(adjIdx) ?: continue
                paginationCache.getOrPut(adjIdx) {
                    val adjShowTitle = shouldRenderChapterTitle(adjChapter)
                    paginateContent(adjChapter, fontSize, availableHeightDp, contentWidthDp, density, adjShowTitle, titleVPaddingDp)
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
    val chapterSlideProgress = remember { Animatable(0f) }
    // 跟踪当前翻页动画 Job，跨章时取消残留协程防止快速翻页导致状态错乱
    var flipJob by remember { mutableStateOf<Job?>(null) }
    // 强制限制连续翻页间隔，防止过快卡死
    val lastFlipTime = remember { mutableLongStateOf(0L) }


    // 保持回调引用最新 (用于 pointerInput 内部)
    val currentOnPrevChapter by rememberUpdatedState(onPrevChapter)
    val currentOnNextChapter by rememberUpdatedState(onNextChapter)
    val currentOnToggleControls by rememberUpdatedState(onToggleControls)

    val currentHasPrevChapter by rememberUpdatedState(hasPrevChapter)
    val currentHasNextChapter by rememberUpdatedState(hasNextChapter)
    val currentLeadingVirtual by rememberUpdatedState(leadingVirtual)
    val currentTrailingVirtual by rememberUpdatedState(trailingVirtual)
    val currentPairedPages by rememberUpdatedState(pairedPages)
    val currentSettingsVisible by rememberUpdatedState(settingsVisible)
    val currentControlsVisible by rememberUpdatedState(controlsVisible)

    val pageCurlConfig = rememberPageCurlConfig(
        backPageColor = bgColor,
        dragInteraction = PageCurlConfig.GestureDragInteraction(
            pointerBehavior = PageCurlConfig.DragInteraction.PointerBehavior.PageEdge
        ),
        onCustomTap = { size, position ->
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
        val isGoingBack = startAtLastPageRef[0]
        val targetSlot = if (isGoingBack) {
            startAtLastPageRef[0] = false
            leadingVirtual + pairedPages.lastIndex.coerceAtLeast(0)
        } else {
            leadingVirtual
        }
        when {
            isRealChapterChange && pageAnimation == "Slide" -> {
                chapterSlideProgress.snapTo(if (isGoingBack) -1f else 1f)
                chapterAlpha.snapTo(1f)
                pagerState.scrollToPage(targetSlot)
                prevChapterKey = currentChapter
                chapterSlideProgress.animateTo(0f, animationSpec = tween(durationMillis = 400))
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

    Column(
        modifier = Modifier
            .fillMaxSize()
            .graphicsLayer {
                alpha = chapterAlpha.value
                translationX = chapterSlideProgress.value * size.width
            }
            .pointerInput(pageAnimation, controlsVisible, settingsVisible) {
                if (pageAnimation == "Realistic") {
                    return@pointerInput
                }
                detectTapGestures { offset ->
                    if (settingsVisible) {
                        return@detectTapGestures
                    }

                    val chromeInset = with(density) { 96.dp.toPx() }
                    if (controlsVisible) {
                        // 控制栏显示时，忽略顶部/底部区域，避免点击工具栏穿透触发翻页
                        if (offset.y < chromeInset || offset.y > size.height - chromeInset) {
                            return@detectTapGestures
                        }
                        // 中部区域点击仅切换控制栏
                        currentOnToggleControls()
                        return@detectTapGestures
                    }

                    val screenWidth = size.width
                    val tapZone = screenWidth * TAP_ZONE_RATIO
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
                            // 中间点击 — 切换控制栏
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
                    onLinkClick = onLinkClick,
                    isTwoColumn = isTwoColumn
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
                    onLinkClick = onLinkClick
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
                        onLinkClick = onLinkClick
                    )
                }
            }
        }
    }
}

// ─── 内容块渲染 ───

@Composable
fun ContentBlockView(
    block: ContentBlock,
    fontSize: Float,
    textColor: Color,
    linkColor: Color,
    fontFamily: FontFamily,
    onLinkClick: (String) -> Unit
) {
    when (block) {
        is ContentBlock.Heading -> {
            val scale = when (block.level) {
                1 -> 2.0f
                2 -> 1.6f
                3 -> 1.3f
                else -> 1.2f
            }
            val annotated = buildSpanAnnotatedString(
                spans = block.spans,
                fontSize = fontSize * scale,
                textColor = textColor,
                linkColor = linkColor,
                fontFamily = fontFamily,
                overrideWeight = FontWeight.Bold,
                onLinkClick = onLinkClick
            )
            val headingStyle = TextStyle(
                fontFamily = fontFamily,
                lineHeight = (fontSize * scale * 1.5f).sp,
                platformStyle = PlatformTextStyle(includeFontPadding = false),
                lineHeightStyle = LineHeightStyle(
                    alignment = LineHeightStyle.Alignment.Center,
                    trim = LineHeightStyle.Trim.None
                )
            )

            Text(
                text = annotated,
                style = headingStyle,
                modifier = Modifier.padding(top = (fontSize * 1.2f).dp, bottom = (fontSize * 1.8f).dp)
            )
        }

        is ContentBlock.Paragraph -> {
            val annotated = buildSpanAnnotatedString(
                spans = block.spans,
                fontSize = fontSize,
                textColor = textColor,
                linkColor = linkColor,
                fontFamily = fontFamily,
                overrideWeight = null,
                onLinkClick = onLinkClick
            )
            val baseStyle = TextStyle(
                fontFamily = fontFamily,
                textIndent = TextIndent(firstLine = (fontSize * 2).sp),
                lineHeight = (fontSize * 1.5f).sp,
                platformStyle = PlatformTextStyle(includeFontPadding = false),
                lineHeightStyle = LineHeightStyle(
                    alignment = LineHeightStyle.Alignment.Center,
                    trim = LineHeightStyle.Trim.None
                )
            )
            
            Text(
                text = annotated,
                modifier = Modifier.padding(vertical = (fontSize * 0.5f).dp),
                style = baseStyle
            )
        }

        is ContentBlock.Image -> {
            val bitmap by produceState<android.graphics.Bitmap?>(initialValue = null, block.data) {
                value = withContext(Dispatchers.IO) {
                    val bytes = android.util.Base64.decode(block.data, android.util.Base64.DEFAULT)
                    BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
                }
            }
            val bmp = bitmap
            if (bmp != null && bmp.width > 0 && bmp.height > 0) {
                val ratio = bmp.width.toFloat() / bmp.height.toFloat()
                Image(
                    bitmap = bmp.asImageBitmap(),
                    contentDescription = block.alt,
                    modifier = Modifier
                        .fillMaxWidth()
                        .aspectRatio(ratio)
                        .padding(vertical = (fontSize * 0.35f).dp),
                    contentScale = ContentScale.Fit
                )
            } else {
                Text(
                    text = block.alt ?: I18n.t("reader.image_load_failed"),
                    color = textColor.copy(alpha = 0.65f),
                    fontSize = (fontSize * 0.9f).sp,
                    modifier = Modifier.padding(vertical = (fontSize * 0.35f).dp)
                )
            }
        }

        is ContentBlock.Separator -> {
            HorizontalDivider(
                modifier = Modifier.padding(vertical = (fontSize * 0.5f).dp),
                color = textColor.copy(alpha = 0.2f)
            )
        }

        is ContentBlock.BlankLine -> {
            Spacer(Modifier.height((fontSize * 0.5f).dp))
        }
    }
}

/**
 * 构建 AnnotatedString，支持粗体、斜体、链接等内联样式
 */
private fun buildSpanAnnotatedString(
    spans: List<TextSpan>,
    fontSize: Float,
    textColor: Color,
    linkColor: Color,
    fontFamily: FontFamily,
    overrideWeight: FontWeight?,
    onLinkClick: ((String) -> Unit)? = null
): AnnotatedString {
    return buildAnnotatedString {
        for (span in spans) {
            val weight = when (span.style) {
                InlineStyle.Bold, InlineStyle.BoldItalic -> FontWeight.Bold
                else -> overrideWeight ?: FontWeight.Normal
            }
            val fontStyle = when (span.style) {
                InlineStyle.Italic, InlineStyle.BoldItalic -> FontStyle.Italic
                else -> FontStyle.Normal
            }
            val color = if (span.linkUrl != null) linkColor else textColor

            val start = length
            withStyle(
                SpanStyle(
                    fontSize = fontSize.sp,
                    fontFamily = fontFamily,
                    fontWeight = weight,
                    fontStyle = fontStyle,
                    color = color
                )
            ) {
                append(span.text)
            }
            val end = length
            val url = span.linkUrl
            if (url != null && end > start) {
                addLink(
                    LinkAnnotation.Clickable(
                        tag = url,
                        linkInteractionListener = if (onLinkClick != null) LinkInteractionListener { onLinkClick(url) } else null
                    ),
                    start,
                    end
                )
            }
        }
    }
}

private fun normalizeInternalHref(raw: String): String {
    val clean = raw.trim().substringBefore('#').trim()
    if (clean.isBlank()) return ""
    val withoutScheme = runCatching {
        val uri = URI(clean)
        if (uri.scheme != null) {
            (uri.path ?: "").trim('/').removePrefix("./")
        } else {
            clean
        }
    }.getOrDefault(clean)

    return withoutScheme
        .trim()
        .removePrefix("./")
        .trim('/')
}

// ─── 分页逻辑 ───

/**
 * 手动将标题按行宽拆成多行（插入 \n），避免 Compose 自动 wrap 导致 lineHeight 不生效。
 */
private fun breakTitleIntoLines(title: String, contentWidthPx: Float, titleFontSizeSp: Float, spToPx: Float): String {
    // 使用 1.1f 倍宽作为字符估算，防止极端字体时一行的字被挤到下一行
    val charWidth = titleFontSizeSp * spToPx * 1.1f
    val charsPerLine = (contentWidthPx / charWidth).toInt().coerceAtLeast(4)
    if (title.length <= charsPerLine) return title
    val sb = StringBuilder()
    var i = 0
    while (i < title.length) {
        if (i > 0) sb.append('\n')
        val end = (i + charsPerLine).coerceAtMost(title.length)
        sb.append(title, i, end)
        i = end
    }
    return sb.toString()
}

private fun paginateContent(
    chapter: Chapter,
    fontSize: Float,
    availableHeight: Dp,
    contentWidth: Dp,
    density: androidx.compose.ui.unit.Density,
    showChapterTitle: Boolean,
    titleVPaddingDp: Dp = 32.dp
): List<List<ContentBlock>> {
    val contentWidthPx = with(density) { contentWidth.toPx() }
    // sp → px 需要乘 fontScale（处理系统字体缩放）
    val spToPx = density.fontScale * density.density
    val lineHeight = fontSize * 1.5f * spToPx
    
    // 采用更精准的容错边距（不再一次性扣除40dp+整行高，那会导致严重底部留白）
    // 给系统布局误差保留 0.5 行高的弹性空间足矣，因为屏幕 padding 本身已避开页码
    val safetyMarginPx = lineHeight * 0.5f
    val maxHeightPx = with(density) { availableHeight.toPx() } - safetyMarginPx
    
    val pages = mutableListOf<List<ContentBlock>>()
    var currentPage = mutableListOf<ContentBlock>()
    var currentHeight = 0f
    var isFirstPage = true

    // 第一页有章节标题占的高度
    if (isFirstPage && showChapterTitle) {
        // 标题实际渲染 lineHeight = (fontSize * 2.2f).sp
        val titleLineHeightPx = fontSize * 2.2f * spToPx
        // 使用 breakTitleIntoLines 保持分行逻辑一致
        val brokenTitle = breakTitleIntoLines(chapter.title, contentWidthPx, fontSize * 1.5f, spToPx)
        val titleLines = brokenTitle.count { it == '\n' } + 1
        // 标题行高 + padding(top = vPadding*0.5 + bottom = vPadding) + 30% 缓冲
        val titlePaddingPx = with(density) { titleVPaddingDp.toPx() } * 1.5f
        currentHeight += (titleLines * titleLineHeightPx + titlePaddingPx) * 1.3f
    }

    for (block in chapter.blocks) {
        val blockHeight = estimateBlockHeight(block, fontSize, lineHeight, contentWidthPx, density)

        if (currentHeight + blockHeight > maxHeightPx && currentPage.isNotEmpty()) {
            pages.add(currentPage.toList())
            currentPage = mutableListOf()
            currentHeight = 0f
            isFirstPage = false
        }

        currentPage.add(block)
        currentHeight += blockHeight
    }

    if (currentPage.isNotEmpty()) {
        pages.add(currentPage.toList())
    }

    return pages.ifEmpty { listOf(emptyList()) }
}

private fun estimateBlockHeight(
    block: ContentBlock,
    fontSize: Float,
    lineHeight: Float,
    contentWidthPx: Float,
    density: androidx.compose.ui.unit.Density
): Float {
    return when (block) {
        is ContentBlock.Heading -> {
            val scale = when (block.level) {
                1 -> 2.0f; 2 -> 1.6f; 3 -> 1.3f; else -> 1.2f
            }
            val spToPx = density.fontScale * density.density
            var cjkCount = 0
            var asciiCount = 0
            block.spans.forEach { span ->
                span.text.forEach { ch ->
                    if (ch.code > 255) cjkCount++ else asciiCount++
                }
            }
            // 准确区分纯英文和中文估算不同字宽，不一拍脑门乘以全行缩减 1.1f 倍
            val estimatedTextWidthPx = (cjkCount * 1.05f + asciiCount * 0.6f) * (fontSize * scale * spToPx)
            val lines = ceil(estimatedTextWidthPx / contentWidthPx).toInt().coerceAtLeast(1)
            // 顶部 1.2 + 底部 1.8 = 3.0 padding (增加标题与正文间距)
            lines * lineHeight * scale + fontSize * 3.0f * density.density
        }
        is ContentBlock.Paragraph -> {
            val spToPx = density.fontScale * density.density
            var cjkCount = 0
            var asciiCount = 0
            block.spans.forEach { span ->
                span.text.forEach { ch ->
                    if (ch.code > 255) cjkCount++ else asciiCount++
                }
            }
            // 中文字体平均严格占宽 1.05em (包括字距)，英文平均大约在 0.55em
            val estimatedTextWidthPx = (cjkCount * 1.05f + asciiCount * 0.55f) * (fontSize * spToPx)
            // 加上默认首行缩进的 2em + 适当的尾行排版容错
            val totalWidthPx = estimatedTextWidthPx + (fontSize * spToPx * 2.5f)
            val lines = ceil(totalWidthPx / contentWidthPx).toInt().coerceAtLeast(1)
            lines * lineHeight + fontSize * 1.0f * density.density
        }
        is ContentBlock.Image -> {
            estimateImageBlockHeight(
                data = block.data,
                contentWidthPx = contentWidthPx,
                density = density,
                fontSize = fontSize
            )
        }
        is ContentBlock.Separator -> lineHeight + fontSize * density.density
        is ContentBlock.BlankLine -> fontSize * 0.5f * density.density
    }
}

private fun estimateImageBlockHeight(
    data: String,
    contentWidthPx: Float,
    density: androidx.compose.ui.unit.Density,
    fontSize: Float
): Float {
    val options = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    val bytes = android.util.Base64.decode(data, android.util.Base64.DEFAULT)
    BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options)
    val w = options.outWidth
    val h = options.outHeight
    if (w <= 0 || h <= 0) {
        return (fontSize * 8f * density.density).coerceAtLeast(96f)
    }

    val ratio = h.toFloat() / w.toFloat()
    val imageHeight = contentWidthPx * ratio
    val verticalPadding = fontSize * 0.7f * density.density
    return imageHeight + verticalPadding
}

private fun shouldRenderChapterTitle(chapter: Chapter): Boolean {
    val first = chapter.blocks.firstOrNull() as? ContentBlock.Heading ?: return true
    val headingText = first.spans.joinToString("") { it.text }.trim().replace(" ", "")
    val chapterText = chapter.title.trim().replace(" ", "")
    if (headingText.isBlank() || chapterText.isBlank()) return true
    return headingText != chapterText
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ReaderSettingsSheet(
    fontSize: Float,
    scrollMode: Boolean,
    isDarkMode: Boolean,
    bgColorIndex: Int,
    customBgColor: Color,
    fontColorIndex: Int,
    customFontColor: Color,
    fontFamilyName: String,
    pageAnimation: String,
    bgImageEnabled: Boolean,
    bgImageAlpha: Float,
    language: String,
    systemFonts: List<FontItem> = emptyList(),
    onDismiss: () -> Unit,
    onFontSizeChange: (Float) -> Unit,
    onScrollModeChange: (Boolean) -> Unit,
    onDarkModeChange: (Boolean) -> Unit,
    onBgColorChange: (Int) -> Unit,
    onCustomBgColorChange: (Color) -> Unit,
    onFontColorChange: (Int) -> Unit,
    onCustomFontColorChange: (Color) -> Unit,
    onFontFamilyChange: (String) -> Unit,
    onPageAnimationChange: (String) -> Unit,
    onBgImageAlphaChange: (Float) -> Unit,
    onLanguageChange: (String) -> Unit,
    onPickBackgroundImage: () -> Unit,
    onClearBackgroundImage: () -> Unit
) {
    val bgOptions = listOf(I18n.t("color.warm_white"), I18n.t("color.light_gray"), I18n.t("color.bean_green"), I18n.t("color.deep_night"), I18n.t("color.graphite"), I18n.t("settings.custom"))
    val fontColorOptions = listOf(I18n.t("color.auto"), I18n.t("color.ink_black"), I18n.t("color.dark_gray"), I18n.t("color.light_gray"), I18n.t("color.cream"), I18n.t("settings.custom"))
    val fontFamilyOptions = listOf("Sans", "Serif", "Monospace")
    val pageAnimationOptions = listOf("Slide", "Cover", "Realistic", "None")
    var fontDropdownExpanded by remember { mutableStateOf(false) }
    var fontSearchQuery by remember { mutableStateOf("") }

    val scrollState = rememberScrollState()
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(scrollState)
                .padding(horizontal = 16.dp, vertical = 8.dp)
        ) {
            Text(I18n.t("settings.title"), style = MaterialTheme.typography.titleLarge)
            Spacer(Modifier.height(12.dp))

            Text(I18n.tf1("settings.font_size", "${fontSize.toInt()}sp"))
            Slider(
                value = fontSize,
                onValueChange = onFontSizeChange,
                valueRange = 12f..40f
            )

            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(I18n.t("settings.reading_mode"), modifier = Modifier.width(72.dp))
                FilterChip(
                    selected = scrollMode,
                    onClick = { onScrollModeChange(true) },
                    label = { Text(I18n.t("settings.scroll")) }
                )
                Spacer(Modifier.width(8.dp))
                FilterChip(
                    selected = !scrollMode,
                    onClick = { onScrollModeChange(false) },
                    label = { Text(I18n.t("settings.paging")) }
                )
            }
            Spacer(Modifier.height(10.dp))

            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(I18n.t("settings.visual"), modifier = Modifier.width(72.dp))
                FilterChip(
                    selected = !isDarkMode,
                    onClick = { onDarkModeChange(false) },
                    label = { Text(I18n.t("toolbar.light_mode")) }
                )
                Spacer(Modifier.width(8.dp))
                FilterChip(
                    selected = isDarkMode,
                    onClick = { onDarkModeChange(true) },
                    label = { Text(I18n.t("toolbar.dark_mode")) }
                )
            }

            Spacer(Modifier.height(12.dp))
            Text(I18n.t("settings.bg_color"))
            Row(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.horizontalScroll(rememberScrollState())
            ) {
                bgOptions.forEachIndexed { index, name ->
                    FilterChip(
                        selected = bgColorIndex == index,
                        onClick = { onBgColorChange(index) },
                        label = { Text(name) }
                    )
                }
            }
            if (bgColorIndex == bgOptions.lastIndex) {
                Spacer(Modifier.height(8.dp))
                ColorEditorRow(
                    label = I18n.t("settings.custom_bg"),
                    color = customBgColor,
                    onColorChange = onCustomBgColorChange
                )
            }

            Spacer(Modifier.height(12.dp))
            Text(I18n.t("settings.font_color"))
            Row(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.horizontalScroll(rememberScrollState())
            ) {
                fontColorOptions.forEachIndexed { index, name ->
                    FilterChip(
                        selected = fontColorIndex == index,
                        onClick = { onFontColorChange(index) },
                        label = { Text(name) }
                    )
                }
            }
            if (fontColorIndex == fontColorOptions.lastIndex) {
                Spacer(Modifier.height(8.dp))
                ColorEditorRow(
                    label = I18n.t("settings.custom_font_color"),
                    color = customFontColor,
                    onColorChange = onCustomFontColorChange
                )
            }

            Spacer(Modifier.height(12.dp))
            Text(I18n.t("settings.font"))
            Box {
                OutlinedButton(
                    onClick = { fontDropdownExpanded = true },
                    modifier = Modifier.fillMaxWidth()
                ) {
                    Text(fontFamilyName, modifier = Modifier.weight(1f))
                    Icon(Icons.Default.ArrowDropDown, contentDescription = null)
                }
                DropdownMenu(
                    expanded = fontDropdownExpanded,
                    onDismissRequest = { fontDropdownExpanded = false; fontSearchQuery = "" },
                    modifier = Modifier.fillMaxWidth(0.9f)
                ) {
                    // 搜索框
                    OutlinedTextField(
                        value = fontSearchQuery,
                        onValueChange = { fontSearchQuery = it },
                        placeholder = { Text(I18n.t("settings.search_font")) },
                        singleLine = true,
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(horizontal = 8.dp, vertical = 4.dp)
                    )
                    val q = fontSearchQuery.lowercase()
                    // 内置字体
                    fontFamilyOptions.filter { q.isEmpty() || it.lowercase().contains(q) }.forEach { fam ->
                        DropdownMenuItem(
                            text = { Text(fam) },
                            onClick = {
                                onFontFamilyChange(fam)
                                fontDropdownExpanded = false
                                fontSearchQuery = ""
                            },
                            leadingIcon = if (fontFamilyName == fam) {
                                { Icon(Icons.Default.Check, contentDescription = null) }
                            } else null
                        )
                    }
                    // 系统字体
                    val filteredSystem = systemFonts.filter { q.isEmpty() || it.displayName.lowercase().contains(q) }
                    if (filteredSystem.isNotEmpty()) {
                        HorizontalDivider()
                        filteredSystem.forEach { item ->
                            DropdownMenuItem(
                                text = { Text(item.displayName) },
                                onClick = {
                                    onFontFamilyChange(item.displayName)
                                    fontDropdownExpanded = false
                                    fontSearchQuery = ""
                                },
                                leadingIcon = if (fontFamilyName == item.displayName) {
                                    { Icon(Icons.Default.Check, contentDescription = null) }
                                } else null
                            )
                        }
                    }
                }
            }

            Spacer(Modifier.height(12.dp))
            Text(I18n.t("settings.page_animation"))
            Row(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.horizontalScroll(rememberScrollState())
            ) {
                pageAnimationOptions.forEach { mode ->
                    FilterChip(
                        selected = pageAnimation == mode,
                        onClick = { onPageAnimationChange(mode) },
                        label = {
                            Text(
                                when (mode) {
                                    "Slide" -> I18n.t("settings.slide")
                                    "Cover" -> I18n.t("settings.cover")
                                    "Realistic" -> I18n.t("settings.realistic")
                                    else -> I18n.t("settings.none")
                                }
                            )
                        }
                    )
                }
            }

            Spacer(Modifier.height(12.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(I18n.t("settings.bg_image"), modifier = Modifier.weight(1f))
                TextButton(onClick = onPickBackgroundImage) { Text(I18n.t("settings.pick_bg_image")) }
                if (bgImageEnabled) {
                    TextButton(onClick = onClearBackgroundImage) { Text(I18n.t("settings.clear_bg_image")) }
                }
            }

            if (bgImageEnabled) {
                Text("${I18n.t("settings.opacity")}: ${(bgImageAlpha * 100).toInt()}%")
                Slider(
                    value = bgImageAlpha,
                    onValueChange = onBgImageAlphaChange,
                    valueRange = 0f..1f
                )
            }

            // ─── 语言选择 ───
            Spacer(Modifier.height(12.dp))
            Text(I18n.t("settings.language"))
            Row(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.horizontalScroll(rememberScrollState())
            ) {
                I18n.availableLanguages.forEach { (code, label) ->
                    val selected = if (I18n.isAuto) code == "auto"
                                   else code == I18n.currentCode
                    FilterChip(
                        selected = selected,
                        onClick = { onLanguageChange(code) },
                        label = { Text(label) }
                    )
                }
            }

            Spacer(Modifier.height(20.dp))
        }
    }
}

/** 从文件路径创建 Compose FontFamily；失败时降级到 SansSerif */
private fun fontFamilyFromFile(path: String): FontFamily = try {
    FontFamily(Font(File(path)))
} catch (_: Exception) {
    FontFamily.SansSerif
}

@Composable
private fun ColorEditorRow(
    label: String,
    color: Color,
    onColorChange: (Color) -> Unit
) {
    fun Color.channelR(): Float = red
    fun Color.channelG(): Float = green
    fun Color.channelB(): Float = blue

    Text(label, style = MaterialTheme.typography.labelMedium)
    Spacer(Modifier.height(6.dp))
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp)
    ) {
        Box(
            modifier = Modifier
                .size(28.dp)
                .background(color, RoundedCornerShape(6.dp))
                .border(1.dp, MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(6.dp))
        )
        Column(modifier = Modifier.weight(1f)) {
            Slider(
                value = color.channelR(),
                onValueChange = { onColorChange(Color(it, color.channelG(), color.channelB(), 1f)) },
                valueRange = 0f..1f
            )
            Slider(
                value = color.channelG(),
                onValueChange = { onColorChange(Color(color.channelR(), it, color.channelB(), 1f)) },
                valueRange = 0f..1f
            )
            Slider(
                value = color.channelB(),
                onValueChange = { onColorChange(Color(color.channelR(), color.channelG(), it, 1f)) },
                valueRange = 0f..1f
            )
        }
    }
}

// ─── 顶部工具栏 ───

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ReaderTopBar(
    title: String,
    chapterTitle: String?,
    currentChapter: Int,
    totalChapters: Int,
    isDarkMode: Boolean,
    previousChapter: Int?,
    onNavigateBack: () -> Unit,
    onGoBackChapter: () -> Unit,
    onToggleToc: () -> Unit,
    onOpenSettings: () -> Unit,
    onPrevChapter: () -> Unit,
    onNextChapter: () -> Unit
) {
    TopAppBar(
        title = {
            Column {
                Text(
                    title,
                    fontSize = 14.sp,
                    fontWeight = FontWeight.Bold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis
                )
                if (chapterTitle != null) {
                    Text(
                        "${currentChapter + 1}/$totalChapters  $chapterTitle",
                        fontSize = 11.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis
                    )
                }
            }
        },
        navigationIcon = {
            IconButton(onClick = onNavigateBack) {
                Icon(Icons.Default.Home, I18n.t("nav.back_to_library"))
            }
        },
        actions = {
            if (previousChapter != null) {
                IconButton(onClick = onGoBackChapter) {
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, I18n.t("reader.go_back_chapter"))
                }
            }
            IconButton(onClick = onToggleToc) {
                Icon(Icons.AutoMirrored.Filled.MenuBook, I18n.t("nav.toc"))
            }
            IconButton(onClick = onOpenSettings) {
                Icon(Icons.Default.Settings, I18n.t("nav.reading_settings"))
            }
            IconButton(onClick = onPrevChapter) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, I18n.t("nav.prev_chapter"))
            }
            IconButton(onClick = onNextChapter) {
                Icon(Icons.AutoMirrored.Filled.ArrowForward, I18n.t("nav.next_chapter"))
            }
        },
        colors = TopAppBarDefaults.topAppBarColors(
            containerColor = MaterialTheme.colorScheme.surface.copy(alpha = 0.95f)
        )
    )
}

// ─── 底部控制栏 ───

@Composable
private fun ReaderBottomBar(
    fontSize: Float,
    scrollMode: Boolean,
    isDarkMode: Boolean,
    onFontSizeChange: (Float) -> Unit,
    onToggleScrollMode: () -> Unit,
    onToggleDarkMode: () -> Unit
) {
    Surface(
        color = MaterialTheme.colorScheme.surface.copy(alpha = 0.95f),
        modifier = Modifier
            .fillMaxWidth()
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .navigationBarsPadding()
                .padding(horizontal = 16.dp, vertical = 10.dp),
            horizontalArrangement = Arrangement.SpaceEvenly,
            verticalAlignment = Alignment.CenterVertically
        ) {
            // 字体缩小
            OutlinedButton(
                onClick = { if (fontSize > 12f) onFontSizeChange(fontSize - 2f) },
                modifier = Modifier.size(40.dp),
                shape = CircleShape,
                contentPadding = PaddingValues(0.dp),
                enabled = fontSize > 12f
            ) {
                Text("A-", fontSize = 13.sp, fontWeight = FontWeight.Bold)
            }

            Text("${fontSize.toInt()}sp", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)

            // 字体放大
            OutlinedButton(
                onClick = { if (fontSize < 40f) onFontSizeChange(fontSize + 2f) },
                modifier = Modifier.size(40.dp),
                shape = CircleShape,
                contentPadding = PaddingValues(0.dp),
                enabled = fontSize < 40f
            ) {
                Text("A+", fontSize = 13.sp, fontWeight = FontWeight.Bold)
            }

            // 分隔
            Box(
                Modifier
                    .width(1.dp)
                    .height(24.dp)
                    .background(MaterialTheme.colorScheme.outlineVariant)
            )

            // 滚动/翻页切换
            IconButton(onClick = onToggleScrollMode) {
                if (scrollMode) {
                    Icon(Icons.Default.SwapVert, I18n.t("nav.scroll_mode"))
                } else {
                    Icon(Icons.AutoMirrored.Filled.MenuBook, I18n.t("nav.page_mode"))
                }
            }

            // 日/夜间模式
            IconButton(onClick = onToggleDarkMode) {
                if (isDarkMode) {
                    Icon(Icons.Default.DarkMode, I18n.t("nav.dark_mode"))
                } else {
                    Icon(Icons.Default.LightMode, I18n.t("nav.light_mode"))
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
    onLinkClick: (String) -> Unit,
    isTwoColumn: Boolean = false
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
                            text = breakTitleIntoLines(slotTitle, contentWidthPx, fontSize * 1.5f, spToPx),
                            style = androidx.compose.ui.text.TextStyle(
                                fontSize = (fontSize * 1.5f).sp,
                                lineHeight = (fontSize * 2.2f).sp,
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
                        ContentBlockView(
                            block = block,
                            fontSize = fontSize,
                            textColor = textColor,
                            linkColor = linkColor,
                            fontFamily = fontFamily,
                            onLinkClick = onLinkClick
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
    }
}

/** Thread-safe LRU cache backed by [LinkedHashMap] with access-order eviction. */
private fun <K, V> lruCache(maxSize: Int): MutableMap<K, V> =
    Collections.synchronizedMap(
        object : LinkedHashMap<K, V>(maxSize, 0.75f, true) {
            override fun removeEldestEntry(eldest: MutableMap.MutableEntry<K, V>?): Boolean =
                size > maxSize
        }
    )
