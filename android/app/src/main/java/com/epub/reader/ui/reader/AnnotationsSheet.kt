package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.*
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.model.*
import com.zhongbai233.epub.reader.ui.theme.ReaderGeometry

/** 标注面板 — 显示书签、高亮、笔记 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AnnotationsSheet(
    bookmarks: List<BookmarkDto>,
    highlights: List<HighlightDto>,
    notes: List<NoteDto>,
    corrections: List<CorrectionDto>,
    chapters: List<String>,
    onNavigateToChapter: (Int) -> Unit,
    onRemoveBookmark: (Int) -> Unit,
    onRemoveHighlight: (String) -> Unit,
    onEditNote: (String, String) -> Unit,
    onDismiss: () -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    var selectedTab by remember { mutableIntStateOf(0) }
    val tabs =
        listOf(
            I18n.t("annotations.bookmarks"),
            I18n.t("annotations.highlights"),
            I18n.t("annotations.corrections"),
        )

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        dragHandle = { BottomSheetDefaults.DragHandle() },
    ) {
        Column(
            modifier = Modifier.fillMaxWidth().fillMaxHeight(0.75f).padding(horizontal = 16.dp)
        ) {
            Text(
                I18n.t("annotations.title"),
                fontSize = 18.sp,
                fontWeight = FontWeight.Bold,
                modifier = Modifier.padding(bottom = 12.dp),
            )

            TabRow(selectedTabIndex = selectedTab) {
                tabs.forEachIndexed { index, title ->
                    Tab(
                        selected = selectedTab == index,
                        onClick = { selectedTab = index },
                        text = { Text(title, fontSize = 13.sp) },
                    )
                }
            }

            Spacer(Modifier.height(8.dp))

            when (selectedTab) {
                0 ->
                    BookmarksTab(
                        bookmarks = bookmarks,
                        chapters = chapters,
                        onNavigate = onNavigateToChapter,
                        onRemove = onRemoveBookmark,
                    )
                1 ->
                    HighlightsTab(
                        highlights = highlights,
                        notes = notes,
                        chapters = chapters,
                        onNavigate = onNavigateToChapter,
                        onRemove = onRemoveHighlight,
                        onEditNote = onEditNote,
                    )
                2 ->
                    CorrectionsTab(
                        corrections = corrections,
                        chapters = chapters,
                        onNavigate = onNavigateToChapter,
                    )
            }
        }
    }
}

@Composable
private fun BookmarksTab(
    bookmarks: List<BookmarkDto>,
    chapters: List<String>,
    onNavigate: (Int) -> Unit,
    onRemove: (Int) -> Unit,
) {
    if (bookmarks.isEmpty()) {
        EmptyHint(I18n.t("annotations.no_bookmarks"))
        return
    }

    LazyColumn {
        items(bookmarks.sortedBy { it.chapter }) { bm ->
            val chTitle = chapters.getOrElse(bm.chapter) { "Chapter ${bm.chapter + 1}" }
            Row(
                modifier =
                    Modifier.fillMaxWidth()
                        .clickable { onNavigate(bm.chapter) }
                        .padding(vertical = 10.dp, horizontal = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(
                    Icons.Default.Bookmark,
                    contentDescription = null,
                    tint = Color(0xFFFF9800),
                    modifier = Modifier.size(20.dp),
                )
                Spacer(Modifier.width(12.dp))
                Column(Modifier.weight(1f)) {
                    Text(
                        chTitle,
                        fontSize = 14.sp,
                        fontWeight = FontWeight.Medium,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Text(
                        "${I18n.t("annotations.chapter")} ${bm.chapter + 1}",
                        fontSize = 12.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                IconButton(onClick = { onRemove(bm.chapter) }) {
                    Icon(
                        Icons.Default.Close,
                        contentDescription = I18n.t("annotations.remove"),
                        modifier = Modifier.size(18.dp),
                    )
                }
            }
            HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f))
        }
    }
}

@Composable
private fun HighlightsTab(
    highlights: List<HighlightDto>,
    notes: List<NoteDto>,
    chapters: List<String>,
    onNavigate: (Int) -> Unit,
    onRemove: (String) -> Unit,
    onEditNote: (String, String) -> Unit,
) {
    if (highlights.isEmpty()) {
        EmptyHint(I18n.t("annotations.no_highlights"))
        return
    }

    var editingNoteId by remember { mutableStateOf<String?>(null) }
    var editingNoteText by remember { mutableStateOf("") }

    LazyColumn {
        items(highlights.sortedBy { it.chapter * 10000 + it.startBlock }) { hl ->
            val chTitle = chapters.getOrElse(hl.chapter) { "Chapter ${hl.chapter + 1}" }
            val note = notes.firstOrNull { it.highlightId == hl.id }
            val hlColor = highlightDtoColor(hl.color)

            Column(
                modifier =
                    Modifier.fillMaxWidth()
                        .clickable { onNavigate(hl.chapter) }
                        .padding(vertical = 8.dp, horizontal = 4.dp)
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.size(12.dp).clip(CircleShape).background(hlColor))
                    Spacer(Modifier.width(8.dp))
                    Text(
                        chTitle,
                        fontSize = 13.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.weight(1f),
                    )
                    IconButton(
                        onClick = {
                            editingNoteId = hl.id
                            editingNoteText = note?.content ?: ""
                        },
                        modifier = Modifier.size(32.dp),
                    ) {
                        Icon(
                            if (note != null) Icons.Default.Edit
                            else Icons.AutoMirrored.Filled.NoteAdd,
                            contentDescription = I18n.t("annotations.note"),
                            modifier = Modifier.size(16.dp),
                        )
                    }
                    IconButton(
                        onClick = { onRemove(hl.id) },
                        modifier = Modifier.size(32.dp),
                    ) {
                        Icon(
                            Icons.Default.Close,
                            contentDescription = I18n.t("annotations.remove"),
                            modifier = Modifier.size(16.dp),
                        )
                    }
                }

                if (note != null) {
                    Text(
                        note.content,
                        fontSize = 12.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.padding(start = 20.dp, top = 2.dp),
                    )
                }

                // 笔记编辑弹窗
                if (editingNoteId == hl.id) {
                    AlertDialog(
                        onDismissRequest = { editingNoteId = null },
                        title = { Text(I18n.t("annotations.edit_note")) },
                        text = {
                            OutlinedTextField(
                                value = editingNoteText,
                                onValueChange = { editingNoteText = it },
                                modifier = Modifier.fillMaxWidth(),
                                minLines = 3,
                                maxLines = 6,
                                shape = ReaderGeometry.Shape,
                            )
                        },
                        confirmButton = {
                            TextButton(
                                onClick = {
                                    onEditNote(hl.id, editingNoteText)
                                    editingNoteId = null
                                },
                                shape = ReaderGeometry.Shape,
                                contentPadding = ReaderGeometry.ButtonPadding,
                            ) {
                                Text(I18n.t("common.save"))
                            }
                        },
                        dismissButton = {
                            TextButton(
                                onClick = { editingNoteId = null },
                                shape = ReaderGeometry.Shape,
                                contentPadding = ReaderGeometry.ButtonPadding,
                            ) {
                                Text(I18n.t("common.cancel"))
                            }
                        },
                    )
                }
            }
            HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f))
        }
    }
}

@Composable
private fun CorrectionsTab(
    corrections: List<CorrectionDto>,
    chapters: List<String>,
    onNavigate: (Int) -> Unit,
) {
    if (corrections.isEmpty()) {
        EmptyHint(I18n.t("annotations.no_corrections"))
        return
    }

    LazyColumn {
        items(corrections.sortedBy { it.chapter * 10000 + it.blockIdx }) { cor ->
            val chTitle = chapters.getOrElse(cor.chapter) { "Chapter ${cor.chapter + 1}" }
            Row(
                modifier =
                    Modifier.fillMaxWidth()
                        .clickable { onNavigate(cor.chapter) }
                        .padding(vertical = 10.dp, horizontal = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(
                    Icons.Default.Spellcheck,
                    contentDescription = null,
                    tint =
                        when (cor.status) {
                            "Accepted" -> Color(0xFF4CAF50)
                            "Rejected" -> Color(0xFFF44336)
                            else -> Color(0xFFFF9800)
                        },
                    modifier = Modifier.size(20.dp),
                )
                Spacer(Modifier.width(12.dp))
                Column(Modifier.weight(1f)) {
                    Row {
                        Text(
                            cor.original,
                            fontSize = 14.sp,
                            color = Color(0xFFF44336),
                            fontWeight = FontWeight.Medium,
                        )
                        Text(" → ", fontSize = 14.sp)
                        Text(
                            cor.corrected,
                            fontSize = 14.sp,
                            color = Color(0xFF4CAF50),
                            fontWeight = FontWeight.Medium,
                        )
                    }
                    Text(
                        "$chTitle · ${cor.status}",
                        fontSize = 12.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f))
        }
    }
}

@Composable
private fun EmptyHint(text: String) {
    Box(
        modifier = Modifier.fillMaxWidth().padding(top = 48.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, color = MaterialTheme.colorScheme.onSurfaceVariant, fontSize = 14.sp)
    }
}

private fun highlightDtoColor(name: String): Color =
    when (name) {
        "Yellow" -> Color(0xFFFFF176)
        "Green" -> Color(0xFFA5D6A7)
        "Blue" -> Color(0xFF90CAF9)
        "Pink" -> Color(0xFFF48FB1)
        else -> Color(0xFFFFF176)
    }
