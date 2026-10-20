package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.text.selection.SelectionContainer
import kotlinx.coroutines.launch
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
    exporting: Boolean,
    onExport: () -> Unit,
    loadDetails: suspend (String)->org.json.JSONObject,
    bookmarks: List<BookmarkDto>,
    highlights: List<HighlightDto>,
    notes: List<NoteDto>,
    concurrentNotes: Set<String>,
    corrections: List<CorrectionDto>,
    chapters: List<String>,
    onNavigateToChapter: (Int, Int, Int) -> Unit,
    onRemoveBookmark: (Int) -> Unit,
    onRemoveHighlight: (String) -> Unit,
    onEditNote: (String, String, String?, (Boolean)->Unit) -> Unit,
    onDismiss: () -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    var selectedTab by remember { mutableIntStateOf(0) }
    val scope=rememberCoroutineScope()
    var detailsKey by remember {mutableStateOf<String?>(null)}
    var details by remember {mutableStateOf<org.json.JSONObject?>(null)}
    var detailsError by remember {mutableStateOf<String?>(null)}
    var detailsGeneration by remember {mutableLongStateOf(0L)}
    val showDetails:(String)->Unit={key->
        detailsGeneration++;val generation=detailsGeneration
        detailsKey=key;details=null;detailsError=null
        scope.launch {
            try {val result=loadDetails(key);if(generation==detailsGeneration) details=result}
            catch(cancelled:kotlinx.coroutines.CancellationException) {throw cancelled}
            catch(error:Exception) {if(generation==detailsGeneration) detailsError=error.message ?: I18n.t("common.error_unknown")}
        }
    }
    if(detailsKey!=null) {
        AlertDialog(onDismissRequest={detailsGeneration++;detailsKey=null},title={Text(I18n.t("annotations.sync_details"))},
            text={
                Column(Modifier.heightIn(max=420.dp).verticalScroll(rememberScrollState()),verticalArrangement=Arrangement.spacedBy(12.dp)) {
                    if(detailsError!=null) Text(detailsError!!)
                    else if(details==null) CircularProgressIndicator()
                    else {
                        val result=details!!
                        if(result.optBoolean("concurrent")) Text(I18n.t("annotations.concurrent_versions"),fontWeight=FontWeight.SemiBold)
                        val versions=result.getJSONArray("revisions")
                        for(index in 0 until versions.length()) {
                            val version=versions.getJSONObject(index)
                            val source=if(version.optBoolean("local")) I18n.t("annotations.local_source") else if(version.isNull("source")) I18n.t("annotations.legacy_source") else version.getString("source").take(12)
                            val time=version.optLong("edited_at",0L)
                            val date=if(time in 1L..253402300799L) java.text.DateFormat.getDateTimeInstance(java.text.DateFormat.MEDIUM,java.text.DateFormat.SHORT,I18n.locale).format(java.util.Date(time*1000)) else I18n.t("annotations.unknown_time")
                            Text("${I18n.t("annotations.source_device")}: $source")
                            Text("${I18n.t("annotations.edited_at")}: $date")
                            val payload=version.optJSONObject("value")?.optJSONObject("data")
                            if(payload?.has("content")==true) SelectionContainer {Text(payload.getString("content"))}
                            HorizontalDivider()
                        }
                    }
                }
            },confirmButton={TextButton(onClick={detailsGeneration++;detailsKey=null}) {Text(I18n.t("manage.done"))}})
    }
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
            Row(Modifier.fillMaxWidth().padding(bottom=12.dp),verticalAlignment=Alignment.CenterVertically) {
                Text(I18n.t("annotations.title"),fontSize=18.sp,fontWeight=FontWeight.Bold,modifier=Modifier.weight(1f))
                if(exporting) CircularProgressIndicator(Modifier.size(24.dp),strokeWidth=2.dp)
                else IconButton(onClick=onExport,enabled=bookmarks.isNotEmpty()||highlights.isNotEmpty()) {
                    Icon(Icons.AutoMirrored.Filled.OpenInNew,contentDescription=I18n.t("annotations.export_accessibility"))
                }
            }

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
                        onDetails=showDetails,
                    )
                1 ->
                    HighlightsTab(
                        highlights = highlights,
                        notes = notes,
                        concurrentNotes = concurrentNotes,
                        chapters = chapters,
                        onNavigate = onNavigateToChapter,
                        onRemove = onRemoveHighlight,
                        onEditNote = onEditNote,
                        onDetails=showDetails,
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
    onNavigate: (Int, Int, Int) -> Unit,
    onRemove: (Int) -> Unit,
    onDetails:(String)->Unit,
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
                        .clickable { onNavigate(bm.chapter, bm.block, 0) }
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
                        I18n.tf1("annotations.chapter", (bm.chapter + 1).toString()),
                        fontSize = 12.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                IconButton(onClick={onDetails("b:${bm.chapter}:${bm.block}")}) {Icon(Icons.Default.Info,contentDescription=I18n.t("annotations.sync_details"))}
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
    concurrentNotes: Set<String>,
    chapters: List<String>,
    onNavigate: (Int, Int, Int) -> Unit,
    onRemove: (String) -> Unit,
    onDetails:(String)->Unit,
    onEditNote: (String, String, String?, (Boolean)->Unit) -> Unit,
) {
    if (highlights.isEmpty()) {
        EmptyHint(I18n.t("annotations.no_highlights"))
        return
    }

    var editingNoteId by remember { mutableStateOf<String?>(null) }
    var editingNoteText by remember { mutableStateOf("") }
    var originalNote by remember {mutableStateOf<String?>(null)}
    var savingNote by remember {mutableStateOf(false)}

    LazyColumn {
        items(highlights.sortedBy { it.chapter * 10000 + it.startBlock }) { hl ->
            val chTitle = chapters.getOrElse(hl.chapter) { "Chapter ${hl.chapter + 1}" }
            val note = notes.firstOrNull { it.highlightId == hl.id }
            val hlColor = highlightDtoColor(hl.color)

            Column(
                modifier =
                    Modifier.fillMaxWidth()
                        .clickable { onNavigate(hl.chapter, hl.startBlock, hl.startOffset) }
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
                    IconButton(onClick={onDetails(if(note==null) "h:${hl.id}" else "n:${hl.id}")}) {Icon(Icons.Default.Info,contentDescription=I18n.t("annotations.sync_details"))}
                    IconButton(
                        onClick = {
                            editingNoteId = hl.id
                            originalNote=note?.content
                            editingNoteText = note?.content ?: ""
                        },
                        modifier = Modifier.size(48.dp),
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
                        modifier = Modifier.size(48.dp),
                    ) {
                        Icon(
                            Icons.Default.Close,
                            contentDescription = I18n.t("annotations.remove"),
                            modifier = Modifier.size(16.dp),
                        )
                    }
                }

                if (hl.id in concurrentNotes) Text(I18n.t("annotations.concurrent_versions"),fontSize=12.sp,color=MaterialTheme.colorScheme.primary,modifier=Modifier.padding(start=20.dp))
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
                        onDismissRequest = { if(!savingNote) editingNoteId = null },
                        title = { Text(I18n.t("annotations.edit_note")) },
                        text = {
                            OutlinedTextField(
                                enabled=!savingNote,
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
                                enabled=!savingNote,
                                onClick = {
                                    savingNote=true
                                    onEditNote(hl.id,editingNoteText,originalNote) {success->savingNote=false;if(success) editingNoteId=null}
                                },
                                shape = ReaderGeometry.Shape,
                                contentPadding = ReaderGeometry.ButtonPadding,
                            ) {
                                Text(I18n.t(if(hl.id in concurrentNotes) "annotations.save_merge" else "common.save"))
                            }
                        },
                        dismissButton = {
                            TextButton(
                                enabled=!savingNote,
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
    onNavigate: (Int, Int, Int) -> Unit,
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
                        .clickable { onNavigate(cor.chapter, cor.blockIdx, cor.charOffset) }
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
