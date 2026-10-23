package com.zhongbai233.epub.reader.ui.library

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.ui.reader.rememberReaderSheetDismiss
import com.zhongbai233.epub.reader.ui.theme.ReaderGeometry
import com.zhongbai233.epub.reader.viewmodel.TxtChapterPreview

private enum class TxtReviewPage { Review, Source, Rules, Excerpt }

/** One scroll container per page. Optional tools never compete with the heading list. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TxtImportDialog(
    show: Boolean, title: String, author: String, customRegex: String, useHeuristic: Boolean,
    previews: List<TxtChapterPreview>, converting: Boolean, analyzing: Boolean,
    headingCount: Int, chapterCount: Int, excluded: Set<Int>, sourceQuery: String,
    sourceHits: List<TxtChapterPreview>, onHeading: (Int, Boolean) -> Unit,
    onQuery: (String) -> Unit, onSearch: () -> Unit, onAdd: (Int) -> Unit,
    error: String?, onTitleChange: (String) -> Unit, onAuthorChange: (String) -> Unit,
    onRegexChange: (String) -> Unit, onHeuristicChange: (Boolean) -> Unit,
    onRefreshPreview: () -> Unit, onConvert: () -> Unit, onDismiss: () -> Unit,
) {
    if (!show) return
    val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = true,
        confirmValueChange = { !converting || it != SheetValue.Hidden })
    val close = rememberReaderSheetDismiss(sheet, onDismiss)
    var page by rememberSaveable { mutableStateOf(TxtReviewPage.Review) }
    var excerpt by remember { mutableStateOf<TxtChapterPreview?>(null) }
    val reviewScroll = rememberLazyListState()
    val sourceScroll = rememberLazyListState()
    val rulesScroll = rememberLazyListState()
    val excerptScroll = rememberLazyListState()
    val focus = LocalFocusManager.current
    val wide = LocalConfiguration.current.screenWidthDp >= 600
    fun navigate(to: TxtReviewPage) { focus.clearFocus(); page = to }
    val back = { if (!converting) { if (page == TxtReviewPage.Review) close() else navigate(TxtReviewPage.Review) } }
    ModalBottomSheet(onDismissRequest = { if (!converting) onDismiss() }, sheetState = sheet,
        sheetMaxWidth = 720.dp, containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
        contentWindowInsets = { WindowInsets.safeDrawing.only(WindowInsetsSides.Bottom) }) {
        BackHandler(enabled = page != TxtReviewPage.Review || converting, onBack = back)
        Column(Modifier.fillMaxWidth().fillMaxHeight(0.94f).imePadding()) {
            Row(Modifier.fillMaxWidth().padding(start = 12.dp, end = 16.dp, bottom = 8.dp),
                verticalAlignment = Alignment.CenterVertically) {
                if (page != TxtReviewPage.Review) IconButton(onClick = back, enabled = !converting) {
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, I18n.t("settings.back"))
                }
                Text(I18n.t(when (page) {
                    TxtReviewPage.Review -> "txt.import_title"
                    TxtReviewPage.Source -> "txt.add_heading"
                    TxtReviewPage.Rules -> "txt.advanced"
                    TxtReviewPage.Excerpt -> "txt.excerpt"
                }), style = MaterialTheme.typography.titleLarge,
                    modifier = Modifier.weight(1f).padding(start = if (page == TxtReviewPage.Review) 8.dp else 0.dp))
                if (page == TxtReviewPage.Review) TextButton(onClick = close, enabled = !converting) { Text(I18n.t("common.cancel")) }
            }
            if (analyzing) LinearProgressIndicator(Modifier.fillMaxWidth())
            LazyColumn(Modifier.weight(1f).fillMaxWidth(),
                state = when (page) { TxtReviewPage.Review -> reviewScroll; TxtReviewPage.Source -> sourceScroll; TxtReviewPage.Rules -> rulesScroll; TxtReviewPage.Excerpt -> excerptScroll },
                contentPadding = PaddingValues(horizontal = 20.dp, vertical = 12.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp)) {
                if (error != null) item {
                    Surface(shape = ReaderGeometry.Shape, color = MaterialTheme.colorScheme.errorContainer) {
                        Column(Modifier.fillMaxWidth().padding(16.dp)) {
                            Text(error, color = MaterialTheme.colorScheme.onErrorContainer)
                            TextButton(onClick = onRefreshPreview, enabled = !analyzing && !converting) { Text(I18n.t("txt.retry")) }
                        }
                    }
                }
                when (page) {
                    TxtReviewPage.Review -> {
                        item { TxtSectionLabel(I18n.t("txt.book_info")) }
                        item {
                            Surface(shape = ReaderGeometry.Shape, color = MaterialTheme.colorScheme.surface) {
                                val fieldColors = TextFieldDefaults.colors(unfocusedContainerColor = MaterialTheme.colorScheme.surface, focusedContainerColor = MaterialTheme.colorScheme.surface)
                                if (wide) Row(Modifier.padding(horizontal = 16.dp, vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(20.dp)) {
                                    TextField(title, onTitleChange, label = { Text(I18n.t("txt.book_title")) }, enabled = !converting,
                                        singleLine = true, modifier = Modifier.weight(1f), colors = fieldColors)
                                    TextField(author, onAuthorChange, label = { Text(I18n.t("txt.author_optional")) }, enabled = !converting,
                                        singleLine = true, modifier = Modifier.weight(1f), colors = fieldColors)
                                } else Column(Modifier.padding(horizontal = 16.dp, vertical = 4.dp)) {
                                    TextField(title, onTitleChange, label = { Text(I18n.t("txt.book_title")) },
                                        enabled = !converting, singleLine = true, modifier = Modifier.fillMaxWidth(), colors = fieldColors)
                                    TextField(author, onAuthorChange, label = { Text(I18n.t("txt.author_optional")) },
                                        enabled = !converting, singleLine = true, modifier = Modifier.fillMaxWidth(), colors = fieldColors)
                                }
                            }
                        }
                        item { Text(I18n.t("txt.source_preserved"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                        item {
                            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                                TextButton(enabled = !converting, onClick = { navigate(TxtReviewPage.Source); if (sourceHits.isEmpty()) onSearch() }) { Text(I18n.t("txt.add_heading")) }
                                TextButton(enabled = !converting, onClick = { navigate(TxtReviewPage.Rules) }) { Text(I18n.t("txt.advanced")) }
                            }
                        }
                        item {
                            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                TxtSectionLabel(I18n.t("txt.preview_title"))
                                Text(I18n.count("txt.detected_count", headingCount), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                Text(I18n.t("txt.review_hint"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                        }
                        if (chapterCount > 500) item { Text(I18n.count("txt.preview_limit", 500), style = MaterialTheme.typography.bodySmall) }
                        if (headingCount == 0 && !analyzing && error == null) item { Text(I18n.t("txt.fallback_hint"), style = MaterialTheme.typography.bodySmall) }
                        items(previews, key = { it.lineStart }) { chapter ->
                            Surface(shape = ReaderGeometry.Shape, color = MaterialTheme.colorScheme.surface) {
                                Row(Modifier.fillMaxWidth().heightIn(min = 56.dp), verticalAlignment = Alignment.CenterVertically) {
                                    if (chapter.isHeading || chapter.lineStart in excluded) Checkbox(
                                        checked = chapter.lineStart !in excluded, onCheckedChange = { onHeading(chapter.lineStart, it) }, enabled = !converting && !analyzing)
                                    else Spacer(Modifier.width(16.dp))
                                    Row(Modifier.weight(1f).clickable(enabled = !converting) { excerpt = chapter; navigate(TxtReviewPage.Excerpt) }.padding(vertical = 14.dp).padding(end = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                                        Text(chapter.title, modifier = Modifier.weight(1f), maxLines = 2, overflow = TextOverflow.Ellipsis,
                                            color = if (chapter.lineStart in excluded) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface)
                                        Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, I18n.t("txt.excerpt"), tint = MaterialTheme.colorScheme.onSurfaceVariant)
                                    }
                                }
                            }
                        }
                    }
                    TxtReviewPage.Rules -> {
                        item {
                            Surface(shape = ReaderGeometry.Shape, color = MaterialTheme.colorScheme.surface) {
                                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                                    Row(verticalAlignment = Alignment.CenterVertically) {
                                        Text(I18n.t("txt.mode_heuristic"), Modifier.weight(1f))
                                        Switch(checked = useHeuristic, onCheckedChange = onHeuristicChange, enabled = !converting)
                                    }
                                    Text(I18n.t("txt.heuristic_hint"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                    OutlinedTextField(customRegex, onRegexChange, label = { Text(I18n.t("txt.mode_regex")) },
                                        placeholder = { Text(I18n.t("txt.rule_placeholder")) }, textStyle = MaterialTheme.typography.bodyMedium.copy(fontFamily = FontFamily.Monospace),
                                        enabled = !converting, modifier = Modifier.fillMaxWidth(), shape = ReaderGeometry.Shape, minLines = 2, maxLines = 5)
                                    OutlinedButton(onClick = onRefreshPreview, enabled = !converting && !analyzing) { Text(I18n.t("txt.retry")) }
                                }
                            }
                        }
                    }
                    TxtReviewPage.Source -> {
                        item { Text(I18n.t("txt.add_hint"), style = MaterialTheme.typography.bodyMedium) }
                        item {
                            OutlinedTextField(sourceQuery, onQuery, label = { Text(I18n.t("txt.search_source")) },
                                singleLine = true, modifier = Modifier.fillMaxWidth(), shape = ReaderGeometry.Shape)
                            TextButton(onClick = onSearch, enabled = !converting && !analyzing) { Text(I18n.t("txt.preview")) }
                        }
                        if (sourceHits.isEmpty() && !analyzing) item { Text(I18n.t("txt.no_matches")) }
                        items(sourceHits, key = { it.lineStart }) { row ->
                            Surface(shape = ReaderGeometry.Shape, color = MaterialTheme.colorScheme.surface,
                                modifier = Modifier.clickable(enabled = !converting && !analyzing) { onAdd(row.lineStart); navigate(TxtReviewPage.Review) }) {
                                Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                    Text(row.title, style = MaterialTheme.typography.titleSmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
                                    Text(row.excerpt, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 3, overflow = TextOverflow.Ellipsis)
                                }
                            }
                        }
                        if (sourceHits.size >= 80) item { Text(I18n.t("txt.search_limit"), style = MaterialTheme.typography.bodySmall) }
                    }
                    TxtReviewPage.Excerpt -> excerpt?.let { row ->
                        item { Text(row.title, style = MaterialTheme.typography.titleMedium) }
                        item {
                            Surface(shape = ReaderGeometry.Shape, color = MaterialTheme.colorScheme.surface) {
                                SelectionContainer { Text(row.excerpt, modifier = Modifier.fillMaxWidth().padding(20.dp), style = MaterialTheme.typography.bodyLarge) }
                            }
                        }
                    }
                }
            }
            if (page == TxtReviewPage.Review) {
                HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f))
                Button(onClick = { focus.clearFocus(); onConvert() },
                    enabled = !converting && !analyzing && error == null && previews.isNotEmpty(),
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 12.dp).heightIn(min = 48.dp), shape = ReaderGeometry.Shape) {
                    if (converting) { CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp); Spacer(Modifier.width(8.dp)) }
                    Text(I18n.t(if (converting) "txt.converting" else "txt.convert_import"))
                }
            }
        }
    }
}

@Composable
private fun TxtSectionLabel(text: String) { Text(text, style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
