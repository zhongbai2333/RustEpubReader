package com.zhongbai233.epub.reader.ui.reader
import android.util.Log


import android.app.Activity
import android.graphics.BitmapFactory
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.geometry.Rect
import androidx.compose.animation.*
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.*
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.ui.input.nestedscroll.nestedScroll
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
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.platform.LocalViewConfiguration
import androidx.compose.ui.text.TextLayoutResult
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
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalTextToolbar
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
import androidx.compose.ui.unit.IntOffset
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
    onToggleToc: () -> Unit,
    onToggleSearch: () -> Unit,
    isChapterBookmarked: Boolean = false,
    onToggleBookmark: () -> Unit = {},
    onShowAnnotations: () -> Unit = {},
    highlights: List<HighlightDto> = emptyList(),
    notes: List<NoteDto> = emptyList(),
    onAddHighlight: (Int, Int, Int, Int, Int, String) -> Unit = { _, _, _, _, _, _ -> },
    onSaveNote: (String, String) -> Unit = { _, _ -> },
    // 排版
    lineSpacing: Float = 1.5f,
    paraSpacing: Float = 0.5f,
    textIndent: Int = 2,
    onLineSpacingChange: (Float) -> Unit = {},
    onParaSpacingChange: (Float) -> Unit = {},
    onTextIndentChange: (Int) -> Unit = {},
    // API
    translateApiUrl: String = "",
    translateApiKey: String = "",
    dictionaryApiUrl: String = "",
    dictionaryApiKey: String = "",
    onTranslateApiUrlChange: (String) -> Unit = {},
    onTranslateApiKeyChange: (String) -> Unit = {},
    onDictionaryApiUrlChange: (String) -> Unit = {},
    onDictionaryApiKeyChange: (String) -> Unit = {},
    // TTS
    ttsVoiceName: String = "zh-CN-XiaoxiaoNeural",
    ttsRate: Int = 0,
    ttsVolume: Int = 0,
    onTtsVoiceNameChange: (String) -> Unit = {},
    onTtsRateChange: (Int) -> Unit = {},
    onTtsVolumeChange: (Int) -> Unit = {},
    // TTS playback
    showTtsBar: Boolean = false,
    ttsPlaying: Boolean = false,
    ttsPaused: Boolean = false,
    ttsStatus: String = "",
    ttsCurrentBlock: Int = -1,
    onTtsPlay: () -> Unit = {},
    onTtsPause: () -> Unit = {},
    onTtsResume: () -> Unit = {},
    onTtsStop: () -> Unit = {},
    onTtsClose: () -> Unit = {},
    // CSC
    cscMode: String = "none",
    cscThreshold: String = "standard",
    onCscModeChange: (String) -> Unit = {},
    onCscThresholdChange: (String) -> Unit = {},
    cscModelReady: Boolean = false,
    cscModelLoading: Boolean = false,
    cscCorrections: List<com.zhongbai233.epub.reader.csc.CorrectionInfo> = emptyList(),
    onDownloadCscModel: () -> Unit = {},
    // 段评
    reviewChapterIndices: Set<Int> = emptySet(),
    showReviewPanel: Boolean = false,
    reviewPanelChapter: Int? = null,
    onOpenReviewPanel: (Int, String?) -> Unit = { _, _ -> },
    onCloseReviewPanel: () -> Unit = {}
) {
    var textSelection by remember { mutableStateOf<TextSelectionState?>(null) }
    var selectionAnchorRange by remember { mutableStateOf<TextSelectionState?>(null) }
    var draggingHandle by remember { mutableStateOf<Int?>(null) }
    var rootLayoutCoords by remember { mutableStateOf<LayoutCoordinates?>(null) }
    val blockLayoutRegistry = remember { mutableMapOf<Int, BlockLayoutInfo>() }
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
                            // Intercept review chapters (段评) — show overlay instead of navigating
                            if (reviewChapterIndices.contains(target)) {
                                val anchor = link.substringAfter('#', "")
                                onOpenReviewPanel(target, anchor.takeIf { it.isNotBlank() })
                            } else {
                                onChapterChange(target)
                            }
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

    // ─── 自定义选区工具栏状态 ───
    var selectionMenuVisible by remember { mutableStateOf(false) }
    var selectionRect by remember { mutableStateOf(androidx.compose.ui.geometry.Rect.Zero) }
    var selectionCopyCallback by remember { mutableStateOf<(() -> Unit)?>(null) }
    var activeSelectionAction by remember { mutableStateOf<SelectionAction?>(null) }
    var currentSelectedText by remember { mutableStateOf("") }
    val clipboardManager = LocalClipboardManager.current

    val customTextToolbar = remember {
        CustomTextToolbar(
            onShowMenu = { rect, onCopy ->
                selectionRect = rect
                selectionCopyCallback = onCopy
                selectionMenuVisible = true
            },
            onHideMenu = {
                selectionMenuVisible = false
                selectionCopyCallback = null
            }
        )
    }

    val focusManager = androidx.compose.ui.platform.LocalFocusManager.current
    val handleTextTapped: () -> Unit = {
        focusManager.clearFocus(true)
        if (textSelection != null) {
            textSelection = null
            selectionAnchorRange = null
            draggingHandle = null
            selectionMenuVisible = false
        } else {
            selectionMenuVisible = false
            if (!showSettingsSheet) {
                showControls = !showControls
            }
        }
    }

    val density = LocalDensity.current
    val selectionHandleWidth = 28.dp
    val selectionHandleHeight = 34.dp
    val selectionHandleWidthPx = with(density) { selectionHandleWidth.toPx() }
    val selectionHandleHeightPx = with(density) { selectionHandleHeight.toPx() }
    val selectionHandleVisualRadiusPx = with(density) { 8.dp.toPx() }
    val selectionHandleColor = if (isDarkMode) Color(0xFF64B5F6) else Color(0xFF1976D2)

    fun compareSelectionPositions(
        blockA: Int,
        charA: Int,
        blockB: Int,
        charB: Int
    ): Int = when {
        blockA != blockB -> blockA.compareTo(blockB)
        else -> charA.compareTo(charB)
    }

    fun findNearestSelectionTarget(rawOffset: Offset): Pair<Int, Int>? {
        val root = rootLayoutCoords ?: return null
        var minBlock = -1
        var minOffset = -1
        var minDistance = Float.MAX_VALUE

        for ((idx, info) in blockLayoutRegistry.entries.toList()) {
            if (!info.coordinates.isAttached) continue
            if (info.text.isBlank()) continue
            try {
                val childOffset = info.coordinates.localPositionOf(root, rawOffset)
                val clampedX = childOffset.x.coerceIn(0f, info.coordinates.size.width.toFloat())
                val clampedY = childOffset.y.coerceIn(0f, info.coordinates.size.height.toFloat())
                val dx = childOffset.x - clampedX
                val dy = childOffset.y - clampedY
                val dist = dx * dx + dy * dy

                if (dist < minDistance) {
                    minDistance = dist
                    minBlock = idx
                    minOffset = info.layoutResult.getOffsetForPosition(Offset(clampedX, clampedY))
                }
            } catch (_: Exception) {
            }
        }

        return if (minBlock != -1 && minDistance <= 500000f) {
            minBlock to minOffset
        } else {
            null
        }
    }

    fun expandSelectionToWord(blockIndex: Int, charOffset: Int): TextSelectionState {
        var actualBlock = blockIndex
        var actualOffset = charOffset
        val info = blockLayoutRegistry[actualBlock]

        // If the block has blank text, find the nearest non-blank block
        if (info == null || info.text.isBlank()) {
            val candidates = blockLayoutRegistry.entries
                .filter { it.value.text.isNotBlank() }
                .sortedBy { kotlin.math.abs(it.key - blockIndex) }
            val nearest = candidates.firstOrNull()
            if (nearest != null) {
                actualBlock = nearest.key
                actualOffset = if (nearest.key < blockIndex) nearest.value.text.length else 0
            } else {
                return TextSelectionState(blockIndex, 0, blockIndex, 0)
            }
        }

        val actualInfo = blockLayoutRegistry[actualBlock]
        var wordStart = actualOffset
        var wordEnd = actualOffset

        if (actualInfo != null) {
            val len = actualInfo.text.length
            val safeOffset = actualOffset.coerceIn(0, if (len > 0) len - 1 else 0)
            try {
                val wordBoundary = actualInfo.layoutResult.getWordBoundary(safeOffset)
                wordStart = wordBoundary.start
                wordEnd = wordBoundary.end
            } catch (_: Exception) {
            }
            if (wordStart >= wordEnd) {
                wordStart = safeOffset
                wordEnd = (safeOffset + 1).coerceAtMost(len)
            }
        }

        return TextSelectionState(actualBlock, wordStart, actualBlock, wordEnd)
    }

    fun getSelectedText(sel: TextSelectionState): String {
        val sb = StringBuilder()
        for (blockIdx in sel.startBlock..sel.endBlock) {
            val info = blockLayoutRegistry[blockIdx] ?: continue
            val text = info.text
            val start = if (blockIdx == sel.startBlock) sel.startChar.coerceIn(0, text.length) else 0
            val end = if (blockIdx == sel.endBlock) sel.endChar.coerceIn(0, text.length) else text.length
            if (start < end) {
                if (sb.isNotEmpty()) sb.append("\n")
                sb.append(text.substring(start, end))
            }
        }
        return sb.toString()
    }

    fun updateSelectionMenuFromCurrentSelection() {
        val sel = textSelection
        if (sel == null) {
            selectionRect = androidx.compose.ui.geometry.Rect.Zero
            selectionMenuVisible = false
            return
        }

        var topY = Float.MAX_VALUE
        var rightX = 0f
        for ((idx, info) in blockLayoutRegistry.entries.toList()) {
            if (idx !in sel.startBlock..sel.endBlock || !info.coordinates.isAttached) continue
            try {
                val bounds = info.coordinates.boundsInWindow()
                if (bounds.top < topY) topY = bounds.top
                if (bounds.right > rightX) rightX = bounds.right
            } catch (_: Exception) {
            }
        }

        if (topY != Float.MAX_VALUE) {
            selectionRect = androidx.compose.ui.geometry.Rect(rightX - 300f, topY - 150f, rightX, topY)
        } else {
            selectionRect = androidx.compose.ui.geometry.Rect.Zero
        }
        selectionMenuVisible = true
    }

    fun getSelectionHandlePositions(): Pair<Offset, Offset>? {
        val sel = textSelection ?: return null
        val root = rootLayoutCoords ?: return null
        val startInfo = blockLayoutRegistry[sel.startBlock] ?: return null
        val endInfo = blockLayoutRegistry[sel.endBlock] ?: return null
        if (!startInfo.coordinates.isAttached || !endInfo.coordinates.isAttached) return null
        if (startInfo.text.isEmpty() || endInfo.text.isEmpty()) return null

        return try {
            val startIndex = sel.startChar.coerceIn(0, (startInfo.text.length - 1).coerceAtLeast(0))
            val endIndex = (sel.endChar - 1).coerceIn(0, (endInfo.text.length - 1).coerceAtLeast(0))
            val startBox = startInfo.layoutResult.getBoundingBox(startIndex)
            val endBox = endInfo.layoutResult.getBoundingBox(endIndex)
            val startHandle = root.localPositionOf(startInfo.coordinates, Offset(startBox.left, startBox.bottom))
            val endHandle = root.localPositionOf(endInfo.coordinates, Offset(endBox.right, endBox.top))
            startHandle to endHandle
        } catch (_: Exception) {
            null
        }
    }

    fun updateSelectionFromHandle(handle: Int, rawOffset: Offset) {
        val sel = textSelection ?: return
        val target = findNearestSelectionTarget(rawOffset) ?: return
        var (tBlock, tChar) = target

        // Fix: endChar=0 at block boundary → snap to end of previous block
        if (handle == 1 && tChar == 0 && tBlock > sel.startBlock) {
            val prevInfo = blockLayoutRegistry[tBlock - 1]
            if (prevInfo != null && prevInfo.text.isNotEmpty()) {
                tBlock -= 1
                tChar = prevInfo.text.length
            } else {
                tChar = 1.coerceAtMost(blockLayoutRegistry[tBlock]?.text?.length ?: 1)
            }
        }
        if (handle == 0 && tChar == 0 && tBlock < sel.endBlock) {
            tChar = 0 // startChar=0 is fine, no fix needed
        }

        val nextSelection = when (handle) {
            0 -> {
                if (compareSelectionPositions(tBlock, tChar, sel.endBlock, sel.endChar) < 0) {
                    TextSelectionState(tBlock, tChar, sel.endBlock, sel.endChar)
                } else {
                    null
                }
            }

            1 -> {
                if (compareSelectionPositions(tBlock, tChar, sel.startBlock, sel.startChar) > 0) {
                    TextSelectionState(sel.startBlock, sel.startChar, tBlock, tChar)
                } else {
                    null
                }
            }

            else -> null
        }

        if (nextSelection != null) {
            textSelection = nextSelection
        }
    }

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

    // ─── 选中拖拽手势回调（提取为 lambda，供各模式复用）───
    val selectionOnDragStart: (Offset) -> Unit = { offset ->
        android.util.Log.d("SelDebug", "onDragStart")
        android.util.Log.d("SelectionGestures", "onDragStart triggered! offset=$offset. Registry size: " + blockLayoutRegistry.size)
        draggingHandle = null
        val target = findNearestSelectionTarget(offset)

        if (target != null) {
            val (minBlock, minOffset) = target
            android.util.Log.d("SelectionDebug", "onDragStart: matched minBlock=$minBlock")
            val wordSelection = expandSelectionToWord(minBlock, minOffset)
            selectionAnchorRange = wordSelection
            textSelection = wordSelection
            selectionMenuVisible = false
        } else {
            selectionAnchorRange = null
            textSelection = null
        }
    }

    val selectionOnDrag: (Offset) -> Unit = { position ->
        val selection = textSelection
        if (selection != null) {
            android.util.Log.d("SelectionDebug", "onDrag!")
            val target = findNearestSelectionTarget(position)

            if (target != null) {
                var (minBlock, minOffset) = target
                val anchorRange = selectionAnchorRange

                if (anchorRange != null) {
                    val dragAfterAnchor = minBlock > anchorRange.endBlock ||
                        (minBlock == anchorRange.endBlock && minOffset >= anchorRange.endChar)
                    val dragBeforeAnchor = minBlock < anchorRange.startBlock ||
                        (minBlock == anchorRange.startBlock && minOffset <= anchorRange.startChar)

                    // Fix: endChar=0 at block boundary → snap to end of previous block
                    if (dragAfterAnchor && minOffset == 0 && minBlock > anchorRange.startBlock) {
                        val prevInfo = blockLayoutRegistry[minBlock - 1]
                        if (prevInfo != null && prevInfo.text.isNotEmpty()) {
                            minBlock -= 1
                            minOffset = prevInfo.text.length
                        }
                    }

                    textSelection = when {
                        dragAfterAnchor -> TextSelectionState(anchorRange.startBlock, anchorRange.startChar, minBlock, minOffset)
                        dragBeforeAnchor -> TextSelectionState(minBlock, minOffset, anchorRange.endBlock, anchorRange.endChar)
                        else -> anchorRange
                    }
                }
            }
        }
    }

    val selectionOnDragEnd: () -> Unit = {
        android.util.Log.d("SelDebug", "onDragEnd")
        selectionAnchorRange = null
        updateSelectionMenuFromCurrentSelection()
    }

    val selectionOnDragCancel: () -> Unit = {
        textSelection = null
        selectionAnchorRange = null
        draggingHandle = null
        selectionMenuVisible = false
    }

    CompositionLocalProvider(LocalTextToolbar provides customTextToolbar) {
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(bgColor)
            .onGloballyPositioned { rootLayoutCoords = it }
            .pointerInput(textSelection) {
                detectTapGestures {
                    if (textSelection != null) {
                        textSelection = null
                        selectionAnchorRange = null
                        draggingHandle = null
                        selectionMenuVisible = false
                    }
                }
            }
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

        // 书签下拉指示器状态
        var bookmarkPullOffset by remember { mutableFloatStateOf(0f) }
        val bookmarkThreshold = with(LocalDensity.current) { 100.dp.toPx() }
        var bookmarkSnackText by remember { mutableStateOf<String?>(null) }

        // Snackbar 显示
        LaunchedEffect(bookmarkSnackText) {
            if (bookmarkSnackText != null) {
                delay(1200)
                bookmarkSnackText = null
            }
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
            // 滚动模式: 点击任意处切换控制栏 + 下拉书签
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
                    .pointerInput(blockLayoutRegistry) {
                        detectDragGesturesAfterLongPress(
                            onDragStart = selectionOnDragStart,
                            onDrag = { change, _ ->
                                selectionOnDrag(change.position)
                                change.consume()
                            },
                            onDragEnd = selectionOnDragEnd,
                            onDragCancel = selectionOnDragCancel
                        )
                    }
            ) {
                ScrollModeContent(
                    chapter = chapter,
                    fontSize = fontSize,
                    textColor = textColor,
                    linkColor = linkColor,
                    bgColor = bgColor,
                    fontFamily = fontFamily,
                    onLinkClick = onLinkClick,
                    onTextTapped = handleTextTapped,
                    onOverscrollDown = {
                        onToggleBookmark()
                        bookmarkSnackText = if (!isChapterBookmarked) I18n.t("annotations.bookmark_added")
                        else I18n.t("annotations.bookmark_removed")
                    },
                    lineSpacing = lineSpacing,
                    paraSpacing = paraSpacing,
                    textIndent = textIndent,
                    textSelection = textSelection,
                    onSelectionChange = { textSelection = it },
                    blockLayoutRegistry = blockLayoutRegistry,
                    highlights = highlights
                )

                // 书签下拉指示文字
                AnimatedVisibility(
                    visible = bookmarkSnackText != null,
                    enter = fadeIn() + slideInVertically { -it },
                    exit = fadeOut() + slideOutVertically { -it },
                    modifier = Modifier
                        .align(Alignment.TopCenter)
                        .padding(top = 80.dp)
                        .zIndex(10f)
                ) {
                    Surface(
                        color = MaterialTheme.colorScheme.inverseSurface,
                        shape = RoundedCornerShape(20.dp),
                        modifier = Modifier.padding(8.dp)
                    ) {
                        Text(
                            bookmarkSnackText ?: "",
                            color = MaterialTheme.colorScheme.inverseOnSurface,
                            modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                            fontSize = 14.sp
                        )
                    }
                }
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
                onLinkClick = onLinkClick,
                onTextTapped = handleTextTapped,
                lineSpacing = lineSpacing,
                paraSpacing = paraSpacing,
                textIndent = textIndent,
                textSelection = textSelection,
                onSelectionChange = { textSelection = it },
                blockLayoutRegistry = blockLayoutRegistry,
                onSelectionDragStart = selectionOnDragStart,
                onSelectionDrag = selectionOnDrag,
                onSelectionDragEnd = selectionOnDragEnd,
                onSelectionDragCancel = selectionOnDragCancel,
                highlights = highlights
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
                isBookmarked = isChapterBookmarked,
                onNavigateBack = onNavigateBack,
                onGoBackChapter = onGoBackChapter,
                onToggleSearch = onToggleSearch,
                onToggleBookmark = onToggleBookmark
            )
        }

        // TTS 控制栏
        AnimatedVisibility(
            visible = showTtsBar,
            enter = slideInVertically { -it } + fadeIn(),
            exit = slideOutVertically { -it } + fadeOut(),
            modifier = Modifier.align(Alignment.TopCenter).padding(top = if (showControls) 56.dp else 0.dp)
        ) {
            TtsControlBar(
                playing = ttsPlaying,
                paused = ttsPaused,
                status = ttsStatus,
                currentBlockIndex = ttsCurrentBlock,
                onPlay = onTtsPlay,
                onPause = onTtsPause,
                onResume = onTtsResume,
                onStop = onTtsStop,
                onClose = onTtsClose
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
                onToggleDarkMode = onToggleDarkMode,
                onToggleToc = onToggleToc,
                onShowAnnotations = onShowAnnotations,
                onToggleTts = onTtsPlay,
                onOpenSettings = {
                    showControls = true
                    showSettingsSheet = true
                }
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
                onClearBackgroundImage = onClearBackgroundImage,
                lineSpacing = lineSpacing,
                paraSpacing = paraSpacing,
                textIndent = textIndent,
                onLineSpacingChange = onLineSpacingChange,
                onParaSpacingChange = onParaSpacingChange,
                onTextIndentChange = onTextIndentChange,
                translateApiUrl = translateApiUrl,
                translateApiKey = translateApiKey,
                dictionaryApiUrl = dictionaryApiUrl,
                dictionaryApiKey = dictionaryApiKey,
                onTranslateApiUrlChange = onTranslateApiUrlChange,
                onTranslateApiKeyChange = onTranslateApiKeyChange,
                onDictionaryApiUrlChange = onDictionaryApiUrlChange,
                onDictionaryApiKeyChange = onDictionaryApiKeyChange,
                ttsVoiceName = ttsVoiceName,
                ttsRate = ttsRate,
                ttsVolume = ttsVolume,
                onTtsVoiceNameChange = onTtsVoiceNameChange,
                onTtsRateChange = onTtsRateChange,
                onTtsVolumeChange = onTtsVolumeChange,
                cscMode = cscMode,
                cscThreshold = cscThreshold,
                onCscModeChange = onCscModeChange,
                onCscThresholdChange = onCscThresholdChange,
                cscModelReady = cscModelReady,
                cscModelLoading = cscModelLoading,
                onDownloadCscModel = onDownloadCscModel
            )
        }

        val handlePositions = getSelectionHandlePositions()
        if (textSelection != null && handlePositions != null) {
            val (startHandlePosition, endHandlePosition) = handlePositions
            val startHandleTopLeft = Offset(
                x = startHandlePosition.x - selectionHandleWidthPx / 2f,
                y = startHandlePosition.y - selectionHandleHeightPx
            )
            val endHandleTopLeft = Offset(
                x = endHandlePosition.x - selectionHandleWidthPx / 2f,
                y = endHandlePosition.y
            )
            val currentStartHandleTopLeft = rememberUpdatedState(startHandleTopLeft)
            val currentEndHandleTopLeft = rememberUpdatedState(endHandleTopLeft)
            val startHandleTint = if (draggingHandle == 0) selectionHandleColor else selectionHandleColor.copy(alpha = 0.88f)
            val endHandleTint = if (draggingHandle == 1) selectionHandleColor else selectionHandleColor.copy(alpha = 0.88f)

            Box(
                modifier = Modifier
                    .offset { IntOffset(startHandleTopLeft.x.toInt(), startHandleTopLeft.y.toInt()) }
                    .size(width = selectionHandleWidth, height = selectionHandleHeight)
                    .zIndex(15f)
                    .pointerInput(Unit) {
                        var dragRootOffset = Offset.Zero
                        detectDragGestures(
                            onDragStart = { downOffset ->
                                draggingHandle = 0
                                selectionMenuVisible = false
                                dragRootOffset = currentStartHandleTopLeft.value + downOffset
                            },
                            onDrag = { change, dragAmount ->
                                dragRootOffset += dragAmount
                                updateSelectionFromHandle(0, dragRootOffset)
                                change.consume()
                            },
                            onDragEnd = {
                                draggingHandle = null
                                updateSelectionMenuFromCurrentSelection()
                            },
                            onDragCancel = {
                                draggingHandle = null
                                updateSelectionMenuFromCurrentSelection()
                            }
                        )
                    }
            ) {
                Canvas(modifier = Modifier.fillMaxSize()) {
                    val centerX = size.width / 2f
                    val circleCenter = Offset(centerX, selectionHandleVisualRadiusPx)
                    drawLine(
                        color = startHandleTint,
                        start = circleCenter,
                        end = Offset(centerX, size.height),
                        strokeWidth = selectionHandleVisualRadiusPx * 0.9f
                    )
                    drawCircle(
                        color = startHandleTint,
                        radius = selectionHandleVisualRadiusPx,
                        center = circleCenter
                    )
                }
            }

            Box(
                modifier = Modifier
                    .offset { IntOffset(endHandleTopLeft.x.toInt(), endHandleTopLeft.y.toInt()) }
                    .size(width = selectionHandleWidth, height = selectionHandleHeight)
                    .zIndex(15f)
                    .pointerInput(Unit) {
                        var dragRootOffset = Offset.Zero
                        detectDragGestures(
                            onDragStart = { downOffset ->
                                draggingHandle = 1
                                selectionMenuVisible = false
                                dragRootOffset = currentEndHandleTopLeft.value + downOffset
                            },
                            onDrag = { change, dragAmount ->
                                dragRootOffset += dragAmount
                                updateSelectionFromHandle(1, dragRootOffset)
                                change.consume()
                            },
                            onDragEnd = {
                                draggingHandle = null
                                updateSelectionMenuFromCurrentSelection()
                            },
                            onDragCancel = {
                                draggingHandle = null
                                updateSelectionMenuFromCurrentSelection()
                            }
                        )
                    }
            ) {
                Canvas(modifier = Modifier.fillMaxSize()) {
                    val centerX = size.width / 2f
                    val circleCenter = Offset(centerX, size.height - selectionHandleVisualRadiusPx)
                    drawLine(
                        color = endHandleTint,
                        start = Offset(centerX, 0f),
                        end = circleCenter,
                        strokeWidth = selectionHandleVisualRadiusPx * 0.9f
                    )
                    drawCircle(
                        color = endHandleTint,
                        radius = selectionHandleVisualRadiusPx,
                        center = circleCenter
                    )
                }
            }
        }

        // ─── 自定义选区悬浮菜单 ───
        SelectionFloatingMenu(
            visible = selectionMenuVisible,
            selectionRect = selectionRect,
            isDarkMode = isDarkMode,
            onAction = { action, color ->
                selectionMenuVisible = false
                
                if (action == SelectionAction.HIGHLIGHT || action == SelectionAction.NOTE) {
                    selectionMenuVisible = false
                    val sel = textSelection
                    if (action == SelectionAction.HIGHLIGHT && sel != null) {
                        onAddHighlight(currentChapter, sel.startBlock, sel.startChar, sel.endBlock, sel.endChar, color ?: "Yellow")
                        textSelection = null
                    } else if (action == SelectionAction.NOTE) {
                        // 收集选区文本用于笔记
                        val selText = if (sel != null) getSelectedText(sel) else ""
                        currentSelectedText = selText
                        activeSelectionAction = SelectionAction.NOTE
                    }
                    return@SelectionFloatingMenu
                }

                // ONLY trigger native copy fallback when dictionary/translate/correct/copy explicitly need text
                selectionCopyCallback?.invoke()
                val textFromClipboard = clipboardManager.getText()?.text ?: ""
                currentSelectedText = textFromClipboard
                selectionMenuVisible = false
                
                when (action) {
                    SelectionAction.COPY -> {
                        // Already copied to clipboard
                    }
                    SelectionAction.HIGHLIGHT, SelectionAction.NOTE -> {
                        // Handled above
                    }
                    SelectionAction.DICTIONARY -> {
                        activeSelectionAction = SelectionAction.DICTIONARY
                    }
                    SelectionAction.TRANSLATE -> {
                        activeSelectionAction = SelectionAction.TRANSLATE
                    }
                    SelectionAction.CORRECT -> {
                        activeSelectionAction = SelectionAction.CORRECT
                    }
                }
            },
            onDismiss = {
                selectionMenuVisible = false
            }
        )

        // ─── 选区操作弹窗 ───
        when (activeSelectionAction) {
            SelectionAction.TRANSLATE -> {
                TranslateDialog(
                    selectedText = currentSelectedText,
                    translateApiUrl = translateApiUrl,
                    translateApiKey = translateApiKey,
                    onDismiss = { activeSelectionAction = null }
                )
            }
            SelectionAction.DICTIONARY -> {
                DictionaryDialog(
                    selectedText = currentSelectedText,
                    dictionaryApiUrl = dictionaryApiUrl,
                    dictionaryApiKey = dictionaryApiKey,
                    onDismiss = { activeSelectionAction = null }
                )
            }
            SelectionAction.NOTE -> {
                NoteDialog(
                    selectedText = currentSelectedText,
                    onSaveNote = { noteContent ->
                        val sel = textSelection
                        if (sel != null) {
                            onAddHighlight(currentChapter, sel.startBlock, sel.startChar, sel.endBlock, sel.endChar, "Yellow")
                            val hlId = "hl-${System.currentTimeMillis()}-${sel.startBlock}-${sel.startChar}"
                            onSaveNote(hlId, noteContent)
                            textSelection = null
                        }
                        activeSelectionAction = null
                    },
                    onDismiss = { activeSelectionAction = null }
                )
            }
            SelectionAction.CORRECT -> {
                CorrectionDialog(
                    selectedText = currentSelectedText,
                    onDismiss = { activeSelectionAction = null }
                )
            }
            else -> {}
        }
    }
    } // CompositionLocalProvider
}

// ─── 搜索对话框 ───

@Composable
fun SearchDialog(
    visible: Boolean,
    query: String,
    results: List<com.zhongbai233.epub.reader.model.SearchResult>,
    onQueryChange: (String) -> Unit,
    onSearch: (String) -> Unit,
    onResultClick: (Int) -> Unit,
    onDismiss: () -> Unit
) {
    if (!visible) return
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(I18n.t("search.title")) },
        text = {
            Column(modifier = Modifier.fillMaxWidth()) {
                OutlinedTextField(
                    value = query,
                    onValueChange = onQueryChange,
                    label = { Text(I18n.t("search.placeholder")) },
                    singleLine = true,
                    trailingIcon = {
                        IconButton(onClick = { onSearch(query) }) {
                            Icon(Icons.Default.Search, contentDescription = null)
                        }
                    },
                    modifier = Modifier.fillMaxWidth()
                )
                Spacer(modifier = Modifier.height(8.dp))
                if (results.isEmpty() && query.isNotBlank()) {
                    Text(
                        I18n.t("search.no_results"),
                        fontSize = 13.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
                LazyColumn(modifier = Modifier.heightIn(max = 350.dp)) {
                    items(results.size) { idx ->
                        val r = results[idx]
                        Surface(
                            onClick = { onResultClick(r.chapterIndex) },
                            modifier = Modifier.fillMaxWidth()
                        ) {
                            Column(modifier = Modifier.padding(vertical = 6.dp)) {
                                Text(
                                    r.chapterTitle,
                                    fontWeight = FontWeight.Bold,
                                    fontSize = 13.sp,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis
                                )
                                Text(
                                    r.context,
                                    fontSize = 12.sp,
                                    maxLines = 2,
                                    overflow = TextOverflow.Ellipsis,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant
                                )
                            }
                        }
                    }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = onDismiss) {
                Text(I18n.t("dialog.close"))
            }
        }
    )
}

/** 从文件路径创建 Compose FontFamily；失败时降级到 SansSerif */
private fun fontFamilyFromFile(path: String): FontFamily = try {
    FontFamily(Font(File(path)))
} catch (_: Exception) {
    FontFamily.SansSerif
}

