/**
 * ReaderBars.kt — 阅读器顶部/底部工具栏
 *
 * 从 ReaderScreen.kt 中拆分出来的 Composable，包含:
 * - ReaderTopBar（顶部导航栏）
 * - ReaderBottomBar（底部控制栏：字号、暗色模式、目录等）
 */
package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.MenuBook
import androidx.compose.material.icons.filled.Bookmark
import androidx.compose.material.icons.filled.BookmarkBorder
import androidx.compose.material.icons.filled.DarkMode
import androidx.compose.material.icons.filled.EditNote
import androidx.compose.material.icons.automirrored.filled.FormatListBulleted
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.LightMode
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.SwapVert
import androidx.compose.material.icons.filled.VolumeUp
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.zhongbai233.epub.reader.i18n.I18n

// ─── 顶部工具栏 ───

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun ReaderTopBar(
    title: String,
    chapterTitle: String?,
    currentChapter: Int,
    totalChapters: Int,
    isDarkMode: Boolean,
    previousChapter: Int?,
    isBookmarked: Boolean = false,
    onNavigateBack: () -> Unit,
    onGoBackChapter: () -> Unit,
    onToggleSearch: () -> Unit,
    onToggleBookmark: () -> Unit = {}
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
            IconButton(onClick = onToggleSearch) {
                Icon(Icons.Default.Search, I18n.t("search.title"))
            }
            IconButton(onClick = onToggleBookmark) {
                Icon(
                    if (isBookmarked) Icons.Default.Bookmark else Icons.Default.BookmarkBorder,
                    contentDescription = I18n.t("annotations.add_bookmark"),
                    tint = if (isBookmarked) Color(0xFFFF9800) else LocalContentColor.current
                )
            }
        },
        colors = TopAppBarDefaults.topAppBarColors(
            containerColor = MaterialTheme.colorScheme.surface.copy(alpha = 0.95f)
        )
    )
}

// ─── 底部控制栏 ───

@Composable
internal fun ReaderBottomBar(
    fontSize: Float,
    scrollMode: Boolean,
    isDarkMode: Boolean,
    onFontSizeChange: (Float) -> Unit,
    onToggleScrollMode: () -> Unit,
    onToggleDarkMode: () -> Unit,
    onToggleToc: () -> Unit,
    onShowAnnotations: () -> Unit,
    onToggleTts: () -> Unit,
    onOpenSettings: () -> Unit
) {
    Surface(
        color = MaterialTheme.colorScheme.surface.copy(alpha = 0.95f),
        modifier = Modifier
            .fillMaxWidth()
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .navigationBarsPadding()
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
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
                IconButton(onClick = { onToggleDarkMode(); onOpenSettings() }) {
                    if (isDarkMode) {
                        Icon(Icons.Default.DarkMode, I18n.t("nav.dark_mode"))
                    } else {
                        Icon(Icons.Default.LightMode, I18n.t("nav.light_mode"))
                    }
                }
            }

            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp)
                    .padding(bottom = 10.dp),
                horizontalArrangement = Arrangement.SpaceEvenly,
                verticalAlignment = Alignment.CenterVertically
            ) {
                IconButton(onClick = onToggleToc) {
                    Icon(Icons.AutoMirrored.Filled.FormatListBulleted, I18n.t("nav.toc"))
                }
                IconButton(onClick = onShowAnnotations) {
                    Icon(Icons.Default.EditNote, I18n.t("annotations.title"))
                }
                @Suppress("DEPRECATION")
                IconButton(onClick = onToggleTts) {
                    Icon(Icons.Default.VolumeUp, I18n.t("toolbar.tts"))
                }
                IconButton(onClick = onOpenSettings) {
                    Icon(Icons.Default.Settings, I18n.t("nav.reading_settings"))
                }
            }
        }
    }
}
