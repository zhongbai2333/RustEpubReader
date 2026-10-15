package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.tween
import androidx.compose.animation.core.rememberTransition
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.FormatListBulleted
import androidx.compose.material.icons.automirrored.filled.VolumeUp
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.semantics.isTraversalGroup
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import com.zhongbai233.epub.reader.i18n.I18n

@Composable
@OptIn(ExperimentalLayoutApi::class)
internal fun ReaderTopBar(
    title: String,
    chapterTitle: String?,
    currentChapter: Int,
    totalChapters: Int,
    isBookmarked: Boolean,
    onNavigateBack: () -> Unit,
    onToggleBookmark: () -> Unit,
) {
    Row(Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.statusBarsIgnoringVisibility).padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        ReaderCircleButton(Icons.Default.Close, I18n.t("nav.back_to_library"), action = onNavigateBack)
        Surface(Modifier.weight(1f), shape = RoundedCornerShape(16.dp), color = MaterialTheme.colorScheme.surface.copy(alpha = 0.88f)) {
            Column(Modifier.padding(horizontal = 12.dp, vertical = 7.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                Text(title, style = MaterialTheme.typography.labelLarge, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if(chapterTitle != null) Text("${currentChapter + 1}/$totalChapters · $chapterTitle", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        ReaderCircleButton(if(isBookmarked) Icons.Default.Bookmark else Icons.Default.BookmarkBorder,
            I18n.t(if(isBookmarked) "reader.bookmark_chapter_remove" else "reader.bookmark_chapter_add"), active = isBookmarked, action = onToggleBookmark)
    }
}

@Composable
private fun ReaderCircleButton(icon: ImageVector, label: String, active: Boolean = false, action: () -> Unit) {
    Surface(modifier = Modifier.size(48.dp), shape = CircleShape, color = MaterialTheme.colorScheme.surface.copy(alpha = 0.88f), shadowElevation = 2.dp) {
        IconButton(onClick = action, modifier = Modifier.fillMaxSize()) {
            Icon(icon, contentDescription = label, tint = if(active) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface, modifier = Modifier.size(22.dp))
        }
    }
}

@Composable
@OptIn(ExperimentalLayoutApi::class)
internal fun ReaderBottomBar(
    menuOpen: Boolean,
    bottomInset: Dp = 0.dp,
    onMenuOpenChange: (Boolean) -> Unit,
    progressLabel: String,
    showProgress: Boolean = true,
    readerNight: Boolean,
    onToggleDayNight: () -> Unit,
    onToggleToc: () -> Unit,
    onShowAnnotations: () -> Unit,
    onToggleTts: () -> Unit,
    onOpenCorrection: () -> Unit,
    onToggleSearch: () -> Unit,
    onShowStatistics: () -> Unit,
    onOpenSettings: () -> Unit,
) {
    var more by rememberSaveable { mutableStateOf(false) }
    val screen = LocalConfiguration.current
    val menuWidth = (screen.screenWidthDp - 32).coerceAtMost(300).coerceAtLeast(120).dp
    val menuHeight = (screen.screenHeightDp - 48).coerceAtLeast(200).dp
    Row(Modifier.fillMaxWidth().padding(bottom = bottomInset).windowInsetsPadding(WindowInsets.navigationBarsIgnoringVisibility).padding(16.dp), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
        if (showProgress) Surface(shape = RoundedCornerShape(12.dp), color = MaterialTheme.colorScheme.surface.copy(alpha = 0.88f)) {
            Text(progressLabel, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp))
        } else { Spacer(Modifier.width(1.dp)) }
        Box {
            ReaderCircleButton(Icons.Default.MoreHoriz, I18n.t("reader.menu"), action = { more=false; onMenuOpenChange(!menuOpen) })
            ReaderQuickMenu(expanded = menuOpen, onDismissRequest = { onMenuOpenChange(false) },
                modifier = Modifier.width(menuWidth).heightIn(max = menuHeight)) {
                fun choose(action: () -> Unit) { onMenuOpenChange(false); action() }
                if(more) {
                    ReaderMenuRow(Icons.AutoMirrored.Filled.ArrowBack,I18n.t("settings.back")) {more=false}
                    HorizontalDivider(Modifier.padding(horizontal=16.dp,vertical=4.dp))
                    ReaderMenuRow(Icons.Default.Search, I18n.t("reader.search_book")) {choose(onToggleSearch)}
                    ReaderMenuRow(Icons.Default.BarChart, I18n.t("toolbar.stats")) {choose(onShowStatistics)}
                    ReaderMenuRow(Icons.Default.Spellcheck, I18n.t("reader.correction")) {choose(onOpenCorrection)}
                } else {
                    Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        ReaderMenuTile(Modifier.weight(1f), Icons.AutoMirrored.Filled.FormatListBulleted, I18n.t("nav.toc")) { choose(onToggleToc) }
                        ReaderMenuTile(Modifier.weight(1f), Icons.AutoMirrored.Filled.VolumeUp, I18n.t("toolbar.tts")) { choose(onToggleTts) }
                    }
                    HorizontalDivider(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.6f))
                    ReaderMenuRow(Icons.Default.BookmarkBorder, I18n.t("reader.bookmarks_notes")) { choose(onShowAnnotations) }
                    ReaderMenuRow(if(readerNight) Icons.Default.LightMode else Icons.Default.DarkMode, I18n.t(if(readerNight) "toolbar.light_mode" else "toolbar.dark_mode")) { choose(onToggleDayNight) }
                    ReaderMenuRow(Icons.Default.TextFields, I18n.t("settings.appearance_title")) { choose(onOpenSettings) }
                    ReaderMenuRow(Icons.Default.MoreHoriz, I18n.t("reader.more_actions")) { more=true }
                }
            }
        }
    }
}

/** Keep the popup mounted through exit, so a rapid reopen reverses from its
 * current scale/opacity instead of flashing or restarting the transition. */
@Composable
private fun ReaderQuickMenu(
    expanded: Boolean,
    onDismissRequest: () -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    val visibility = remember { MutableTransitionState(false) }
    visibility.targetState = expanded
    val transition = rememberTransition(visibility, label = "reader menu")
    val opacity by transition.animateFloat(
        transitionSpec = { tween(if (targetState) 180 else 130) }, label = "opacity",
    ) { if (it) 1f else 0f }
    val scale by transition.animateFloat(
        transitionSpec = { tween(if (targetState) 240 else 160, easing = FastOutSlowInEasing) },
        label = "scale",
    ) { if (it) 1f else 0.92f }
    val density = LocalDensity.current
    val direction = LocalLayoutDirection.current
    val menuTitle = I18n.t("reader.menu")
    val position = remember(density) {
        // The outer padding gives the surface's shadow room inside the popup.
        val shadow = with(density) { 12.dp.roundToPx() }
        val gap = with(density) { 8.dp.roundToPx() }
        object : PopupPositionProvider {
            override fun calculatePosition(
                anchorBounds: IntRect, windowSize: IntSize,
                layoutDirection: LayoutDirection, popupContentSize: IntSize,
            ): IntOffset {
                val x = if (layoutDirection == LayoutDirection.Ltr)
                    anchorBounds.right + shadow - popupContentSize.width
                else anchorBounds.left - shadow
                val y = anchorBounds.top - gap + shadow - popupContentSize.height
                return IntOffset(
                    x.coerceIn(0, (windowSize.width - popupContentSize.width).coerceAtLeast(0)),
                    y.coerceIn(0, (windowSize.height - popupContentSize.height).coerceAtLeast(0)),
                )
            }
        }
    }
    if (visibility.currentState || visibility.targetState || !visibility.isIdle) {
        Popup(
            popupPositionProvider = position,
            onDismissRequest = onDismissRequest,
            properties = PopupProperties(focusable = expanded),
        ) {
            Box(Modifier.padding(12.dp)) {
                Surface(
                    modifier.semantics {
                        paneTitle = menuTitle
                        isTraversalGroup = true
                    }.graphicsLayer {
                        alpha = opacity
                        scaleX = scale
                        scaleY = scale
                        transformOrigin = TransformOrigin(if (direction == LayoutDirection.Ltr) 1f else 0f, 1f)
                    },
                    shape = RoundedCornerShape(24.dp),
                    color = MaterialTheme.colorScheme.surface.copy(alpha = 0.95f),
                    tonalElevation = 0.dp,
                    shadowElevation = 12.dp,
                ) {
                    Column(
                        Modifier.verticalScroll(rememberScrollState()).padding(vertical = 8.dp),
                        content = content,
                    )
                }
            }
        }
    }
}

@Composable
private fun ReaderMenuTile(modifier: Modifier, icon: ImageVector, label: String, action: () -> Unit) {
    TextButton(onClick = action, modifier = modifier.heightIn(min = 76.dp), shape = RoundedCornerShape(12.dp), colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.onSurface)) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Icon(icon, null, Modifier.size(26.dp))
            Text(label, style = MaterialTheme.typography.bodyMedium, maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
    }
}

@Composable
private fun ReaderMenuRow(icon: ImageVector, label: String, action: () -> Unit) {
    DropdownMenuItem(text = { Text(label, style = MaterialTheme.typography.bodyLarge, maxLines = 2, overflow = TextOverflow.Ellipsis) },
        leadingIcon = { Icon(icon, null, Modifier.size(22.dp)) }, onClick = action, contentPadding = PaddingValues(horizontal = 20.dp, vertical = 8.dp))
}
