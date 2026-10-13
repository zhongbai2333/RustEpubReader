/**
 * ReaderBars.kt — 阅读器顶部/底部工具栏
 *
 * 从 ReaderScreen.kt 中拆分出来的 Composable，包含:
 * - ReaderTopBar（顶部导航栏）
 * - ReaderBottomBar（底部控制栏：字号、暗色模式、目录等）
 */
package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.FormatListBulleted
import androidx.compose.material.icons.automirrored.filled.VolumeUp
import androidx.compose.material.icons.filled.Bookmark
import androidx.compose.material.icons.filled.BookmarkBorder
import androidx.compose.material.icons.filled.EditNote
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
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
import com.zhongbai233.epub.reader.ui.theme.ReaderGeometry

// ─── 顶部工具栏 ───

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun ReaderTopBar(
    title: String,
    chapterTitle: String?,
    currentChapter: Int,
    totalChapters: Int,
    chapterProgress: Float,
    previousChapter: Int?,
    isBookmarked: Boolean = false,
    onNavigateBack: () -> Unit,
    onGoBackChapter: () -> Unit,
    onToggleSearch: () -> Unit,
    onToggleBookmark: () -> Unit = {},
) {
    TopAppBar(
        title = {
            Column {
                Text(
                    title,
                    fontSize = 16.sp,
                    fontWeight = FontWeight.Bold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                if (chapterTitle != null) {
                    Text(
                        "${currentChapter + 1}/$totalChapters  ·  $chapterTitle",
                        fontSize = 12.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                LinearProgressIndicator(
                    progress = { chapterProgress.coerceIn(0f, 1f) },
                    modifier = Modifier.fillMaxWidth().padding(top = 4.dp).height(2.dp),
                    color = MaterialTheme.colorScheme.primary,
                    trackColor = MaterialTheme.colorScheme.surfaceVariant,
                )
            }
        },
        navigationIcon = {
            IconButton(onClick = onNavigateBack) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, I18n.t("nav.back_to_library"))
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
                    tint = if (isBookmarked) Color(0xFFFF9800) else LocalContentColor.current,
                )
            }
        },
        colors =
            TopAppBarDefaults.topAppBarColors(
                containerColor = MaterialTheme.colorScheme.surface.copy(alpha = 0.95f)
            ),
    )
}

// ─── 底部控制栏 ───

@Composable
internal fun ReaderBottomBar(
    onToggleToc: () -> Unit,
    onShowAnnotations: () -> Unit,
    onToggleTts: () -> Unit,
    onOpenSettings: () -> Unit,
) {
    Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 0.dp) {
        Row(
            Modifier.fillMaxWidth().navigationBarsPadding().padding(vertical = 8.dp),
            horizontalArrangement = Arrangement.SpaceEvenly,
        ) {
            ReaderTool(Icons.AutoMirrored.Filled.FormatListBulleted, I18n.t("nav.toc"), onToggleToc)
            ReaderTool(Icons.Default.EditNote, I18n.t("annotations.title"), onShowAnnotations)
            ReaderTool(Icons.AutoMirrored.Filled.VolumeUp, I18n.t("toolbar.tts"), onToggleTts)
            ReaderTool(Icons.Default.Settings, I18n.t("settings.title"), onOpenSettings)
        }
    }
}

@Composable
private fun ReaderTool(
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    label: String,
    action: () -> Unit,
) {
    androidx.compose.material3.TextButton(
        onClick = action,
        contentPadding = ReaderGeometry.ButtonPadding,
        shape = ReaderGeometry.Shape,
    ) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Icon(icon, null, modifier = Modifier.size(22.dp))
            Text(label, style = MaterialTheme.typography.labelSmall, maxLines = 1)
        }
    }
}
