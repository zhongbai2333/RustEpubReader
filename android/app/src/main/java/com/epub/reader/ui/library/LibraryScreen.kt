package com.zhongbai233.epub.reader.ui.library

import android.graphics.BitmapFactory
import androidx.compose.animation.core.tween
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.grid.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.MenuBook
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Check
import androidx.compose.ui.platform.LocalContext
import androidx.compose.material.icons.filled.MoreHoriz
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.BorderStroke
import androidx.compose.material.icons.filled.Add
import androidx.compose.material3.*
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.zIndex
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.model.BookEntry
import com.zhongbai233.epub.reader.ui.theme.ReaderGeometry
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LibraryScreen(
    books: List<BookEntry>,
    active: Boolean = true,
    query: String = "",
    searchActive: Boolean = false,
    onQueryChange: (String) -> Unit = {},
    onSearchActiveChange: (Boolean) -> Unit = {},
    coverCache: Map<String, ByteArray?>,
    onOpenFilePicker: () -> Unit,
    onOpenBook: (String, Int) -> Unit,
    onRemoveBook: (String) -> Unit,
    onExportBooks: (List<String>) -> Unit = {},
    onRenameBook: (String, String) -> Unit = { _, _ -> },
    exportingBooks: Boolean = false,
    shareModel: BookShareModel? = null,
    githubToken: String? = null,
    githubAuthorized: Boolean = false,
    onGithubLogin: () -> Unit = {},
    onReceiveBooks: () -> Unit = {},
    onRequestCover: (String) -> Unit = {},
    onRefreshLibrary: suspend () -> Unit = {},
    onOpenSettings: () -> Unit = {},
    onOpenSync: () -> Unit = {},
    onOpenStatistics: () -> Unit = {},
    syncing: Boolean = false,
    syncMeter: org.json.JSONObject? = null,
    archivedBooks: List<org.json.JSONObject> = emptyList(),
    onArchiveAction: (String,String) -> Unit = { _,_ -> },
    onCoverBounds: (String, Rect) -> Unit = { _, _ -> },
    shelfOrder: StableShelfOrder = remember { StableShelfOrder() },
    shelfRevision: Int = 0,
    promotedUri: String? = null,
    reduceShelfMotion: Boolean = false,
) {
    val langVersion = I18n.version
    var managing by rememberSaveable { mutableStateOf(false) }
    var selected by rememberSaveable { mutableStateOf(listOf<String>()) }
    var confirmDelete by remember { mutableStateOf(false) }
    var shareTargets by remember { mutableStateOf<List<String>>(emptyList()) }
    var renaming by remember { mutableStateOf<BookEntry?>(null) }
    var renameTitle by remember { mutableStateOf("") }
    LaunchedEffect(books.map { it.uri }) { selected = selected.filter { uri -> books.any { it.uri == uri } } }
    BackHandler(enabled = managing) { managing = false; selected = emptyList() }
    val focusManager = LocalFocusManager.current
    val keyboard = LocalSoftwareKeyboardController.current
    var showArchive by remember { mutableStateOf(false) }
    var showMore by remember { mutableStateOf(false) }
    var showAdd by remember { mutableStateOf(false) }
    var showSortDisplay by rememberSaveable { mutableStateOf(false) }
    val preferences = LocalContext.current.getSharedPreferences("library-display", android.content.Context.MODE_PRIVATE)
    var sortTitle by rememberSaveable { mutableStateOf(preferences.getBoolean("sort-title", false)) }
    var listMode by rememberSaveable { mutableStateOf(preferences.getBoolean("list", false)) }
    val chooseSort: (Boolean) -> Unit = { value -> sortTitle = value; shelfOrder.reset(); preferences.edit().putBoolean("sort-title", value).apply() }
    val chooseDisplay: (Boolean) -> Unit = { value -> listMode = value; preferences.edit().putBoolean("list", value).apply() }
    var refreshing by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val shelfBooks = books.toList()
    val ranked = remember(shelfBooks, sortTitle) { if(sortTitle) shelfBooks.sortedBy {it.title.lowercase()} else shelfBooks.sortedByDescending {it.lastOpened} }
    val ranks = remember(ranked, sortTitle, shelfRevision) {
        shelfOrder.resolve(ranked.map {it.uri},sortTitle,shelfRevision).withIndex().associate {it.value to it.index}
    }
    val searchTerm = query.trim()
    val filtered = remember(ranked, ranks, searchTerm) { ranked.sortedBy {ranks[it.uri] ?: Int.MAX_VALUE}.filter {it.title.contains(searchTerm,ignoreCase=true)} }
    val recent = remember(shelfBooks) { shelfBooks.maxByOrNull { it.lastOpened }?.takeIf { it.lastOpened > 0 } }
    val libraryHeader: @Composable () -> Unit = {
        Row(Modifier.fillMaxWidth().heightIn(min = 56.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.weight(1f)) {
                Column(Modifier.fillMaxWidth()) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text(
                            I18n.t("library.title"),
                            style = MaterialTheme.typography.headlineSmall,
                            fontWeight = FontWeight.Bold,
                        )
                        if (syncing) {
                            Spacer(Modifier.width(4.dp))
                            CircularProgressIndicator(
                                modifier = Modifier.size(14.dp).semantics { contentDescription = I18n.t("cloud.syncing") },
                                strokeWidth = 1.5.dp,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                            Text(I18n.t("cloud.syncing_indicator"), maxLines = 1, softWrap = false, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                    }
                    if (syncing) SyncMeterContent(syncMeter,compact=true)
                    Text(
                        I18n.count("ui.library_count", books.size),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                val outline = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.55f))
                val buttonColors = IconButtonDefaults.outlinedIconButtonColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow, contentColor = MaterialTheme.colorScheme.primary)
                Box {
                    OutlinedIconButton(onClick = { showAdd = true; showMore = false }, border = outline, colors = buttonColors, modifier = Modifier.size(48.dp)) {
                        Icon(Icons.Default.Add, I18n.t("library.add_books"), Modifier.size(22.dp))
                    }
                    DropdownMenu(showAdd, { showAdd = false }) {
                        DropdownMenuItem(text = { Text(I18n.t("ui.import")) }, onClick = { showAdd = false; onOpenFilePicker() })
                        DropdownMenuItem(text = { Text(I18n.t("transfer.receive")) }, onClick = { showAdd = false; onReceiveBooks() })
                    }
                }
                Spacer(Modifier.width(8.dp))
                OutlinedButton(onClick = { managing = !managing; if (!managing) selected = emptyList() }, enabled = books.isNotEmpty(), shape = CircleShape, border = outline,
                    colors = ButtonDefaults.outlinedButtonColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
                    contentPadding = PaddingValues(horizontal = 12.dp), modifier = Modifier.heightIn(min = 40.dp)) { Text(I18n.t(if (managing) "manage.done" else "manage.books"), maxLines = 1) }
                Spacer(Modifier.width(8.dp))
                Box {
                    OutlinedIconButton(onClick = { showMore = true; showAdd = false }, border = outline, colors = buttonColors, modifier = Modifier.size(48.dp)) {
                        Icon(Icons.Default.MoreHoriz, I18n.t("ui.more"), Modifier.size(22.dp))
                    }
                    DropdownMenu(showMore, { showMore = false }) {
                        DropdownMenuItem(text = { Text(I18n.t("library.sort_display")) }, onClick = { showMore = false; showSortDisplay = true })
                        DropdownMenuItem(text = { Text(I18n.t("archive.title") + " (${archivedBooks.size})") }, onClick = { showMore = false; showArchive = true })
                        DropdownMenuItem(text = { Text(I18n.t("stats.title")) }, onClick = { showMore = false; onOpenStatistics() })
                        DropdownMenuItem(text = { Text(I18n.t("settings.title")) }, onClick = { showMore = false; onOpenSettings() })
                    }
                }
            }
        }
    }
    Scaffold(
        containerColor = MaterialTheme.colorScheme.background,
        bottomBar = { HomeNavigation(false, {}, onOpenSync,
            onSearch = { onSearchActiveChange(true) }, searching = searchActive, query = query,
            onQueryChange = onQueryChange, interactive = active, onSearchClose = { onSearchActiveChange(false); onQueryChange("") }) },
    ) { padding ->
        val layoutDirection = LocalLayoutDirection.current
        PullToRefreshBox(
            isRefreshing = refreshing,
            onRefresh = {
                scope.launch {
                    refreshing = true
                    try {
                        shelfOrder.reset()
                        // Commit the refreshing state before even a fast cached refresh completes.
                        withFrameNanos { }
                        onRefreshLibrary()
                    } finally {
                        refreshing = false
                    }
                }
            },
            // The bottom controls float over the shelf; only the scrollable
            // content tail reserves their height so the final book stays reachable.
            modifier = Modifier.fillMaxSize().padding(
                start = padding.calculateStartPadding(layoutDirection),
                top = padding.calculateTopPadding(),
                end = padding.calculateEndPadding(layoutDirection),
            ),
        ) {
            LazyVerticalGrid(
                columns = if (listMode) GridCells.Fixed(1) else GridCells.Adaptive(144.dp),
                contentPadding = PaddingValues(
                    start = ReaderGeometry.PageInset,
                    top = ReaderGeometry.PageInset,
                    end = ReaderGeometry.PageInset,
                    bottom = ReaderGeometry.PageInset + padding.calculateBottomPadding(),
                ),
                horizontalArrangement = Arrangement.spacedBy(ReaderGeometry.BlockGap),
                verticalArrangement = Arrangement.spacedBy(if (listMode) 8.dp else ReaderGeometry.SectionGap),
            ) {
                item(span = { GridItemSpan(maxLineSpan) }) { libraryHeader() }
                if (!managing && query.isEmpty() && recent != null)
                    item(span = { GridItemSpan(maxLineSpan) }) {
                        Card(
                            colors =
                                CardDefaults.cardColors(
                                    containerColor = MaterialTheme.colorScheme.surface
                                ),
                            modifier = Modifier.fillMaxWidth(),
                        ) {
                            Row(
                                Modifier.fillMaxWidth().padding(ReaderGeometry.CardInset),
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement =
                                    Arrangement.spacedBy(ReaderGeometry.BlockGap),
                            ) {
                                Column(
                                    Modifier.weight(1f),
                                    verticalArrangement = Arrangement.spacedBy(ReaderGeometry.Gap),
                                ) {
                                    Text(
                                        I18n.t("ui.recent"),
                                        style = MaterialTheme.typography.labelMedium,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                                    )
                                    Text(
                                        recent.title,
                                        style = MaterialTheme.typography.titleMedium,
                                        maxLines = 2,
                                        overflow = TextOverflow.Ellipsis,
                                    )
                                    Text(
                                        recent.lastChapterTitle
                                            ?: I18n.tf1("ui.chapter", "${recent.lastChapter+1}"),
                                        style = MaterialTheme.typography.bodySmall,
                                    )
                                }
                                Button(
                                    onClick = { focusManager.clearFocus(); keyboard?.hide(); onCoverBounds(recent.uri, Rect.Zero); onOpenBook(recent.uri, recent.lastChapter) },
                                    contentPadding = ReaderGeometry.ButtonPadding,
                                    shape = ReaderGeometry.Shape,
                                ) {
                                    Text(I18n.t("ui.continue"))
                                }
                            }
                        }
                    }
                item(span = { GridItemSpan(maxLineSpan) }) {
                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                        Text(
                            I18n.t("ui.all_books"),
                            style = MaterialTheme.typography.titleMedium,
                            modifier = Modifier.weight(1f),
                        )

                    }
                }
                if (filtered.isEmpty())
                    item(span = { GridItemSpan(maxLineSpan) }) {
                        Column(
                            Modifier.fillMaxWidth().padding(vertical = 40.dp),
                            horizontalAlignment = Alignment.CenterHorizontally,
                            verticalArrangement = Arrangement.spacedBy(ReaderGeometry.BlockGap),
                        ) {
                            Icon(
                                Icons.AutoMirrored.Filled.MenuBook,
                                null,
                                modifier = Modifier.size(40.dp),
                                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                            Text(
                                I18n.t(if (books.isEmpty() && query.isBlank()) "library.empty" else "ui.no_results"),
                                style = MaterialTheme.typography.titleMedium,
                            )
                            if (books.isEmpty() && query.isBlank()) {
                                Text(I18n.t("ui.empty_hint"))
                                Button(
                                    onClick = onOpenFilePicker,
                                    shape = CircleShape,
                                    contentPadding = ReaderGeometry.ButtonPadding,
                                ) {
                                    Text(I18n.t("ui.import"))
                                }
                            } else
                                TextButton(
                                    onClick = { onQueryChange("") },
                                    shape = ReaderGeometry.Shape,
                                    contentPadding = ReaderGeometry.ButtonPadding,
                                ) {
                                    Text(I18n.t("ui.clear_search"))
                                }
                        }
                    }
                if (managing) item(span = { GridItemSpan(maxLineSpan) }) {
                    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(I18n.tf1("manage.count", "${selected.size}"), style = MaterialTheme.typography.titleSmall)
                        Row {
                            TextButton(onClick = { selected = (selected + filtered.map { it.uri }).distinct() }) { Text(I18n.t("manage.all")) }
                            TextButton(onClick = { selected = emptyList() }) { Text(I18n.t("manage.clear")) }
                        }
                        Row {
                            TextButton(onClick = { shareTargets = selected.toList() }, enabled = selected.isNotEmpty()) { Text(I18n.t("manage.share")) }
                            TextButton(onClick = { confirmDelete = true }, enabled = selected.isNotEmpty(), ) { Text(I18n.t("archive.action")) }
                        }
                    }
                }
                items(filtered, key = { it.uri }) { entry ->
                    DisposableEffect(entry.uri) { onDispose { onCoverBounds(entry.uri, Rect.Zero) } }
                    LaunchedEffect(entry.uri) {
                        if (!coverCache.containsKey(entry.uri)) onRequestCover(entry.uri)
                    }
                    BookCard(
                        entry,
                        coverCache[entry.uri],
                        { if (managing) selected = if (entry.uri in selected) selected - entry.uri else selected + entry.uri else { focusManager.clearFocus(); keyboard?.hide(); onOpenBook(entry.uri, entry.lastChapter) } },
                        { onRemoveBook(entry.uri) },
                        managing, entry.uri in selected,
                        listMode = listMode,
                        onShare = { shareTargets = listOf(entry.uri) },
                        onRename = { renaming = entry; renameTitle = entry.title },
                        onBounds = { onCoverBounds(entry.uri, it) },
                        modifier = Modifier.zIndex(if(entry.uri == promotedUri) 1f else 0f).animateItem(fadeInSpec = null, fadeOutSpec = null,
                            placementSpec = if (reduceShelfMotion) null else tween(380)),
                    )
                }
            }
        }
    }
    if (showArchive) ArchiveBooksSheet(archivedBooks,onArchiveAction) { showArchive=false }
    if (confirmDelete) AlertDialog(onDismissRequest = { confirmDelete = false }, title = { Text(I18n.t("archive.confirm")) }, text = { Text(I18n.t("archive.note")) }, confirmButton = {
        TextButton(onClick = { val deleting = selected; confirmDelete = false; selected = emptyList(); deleting.forEach(onRemoveBook) }) { Text(I18n.t("archive.action")) }
    }, dismissButton = { TextButton(onClick = { confirmDelete = false }) { Text(I18n.t("settings.cancel")) } })
    if (shareTargets.isNotEmpty()) {
        val shareBooks = shareTargets.mapNotNull { uri -> books.find { it.uri == uri } }
        BookShareSendSheet(shareModel, shareBooks, coverCache, githubToken, githubAuthorized, onGithubLogin, exportingBooks,
            onExport = { onExportBooks(shareBooks.map { it.uri }) }, onDismiss = { if (!exportingBooks) shareTargets = emptyList() })
    }
    renaming?.let { entry ->
        AlertDialog(onDismissRequest = { renaming = null }, title = { Text(I18n.t("book.rename")) }, text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(I18n.t("book.rename_description"))
                OutlinedTextField(value = renameTitle, onValueChange = { renameTitle = it }, singleLine = true,
                    label = { Text(I18n.t("book.rename_hint")) })
            }
        }, confirmButton = {
            TextButton(onClick = { onRenameBook(entry.uri, renameTitle.trim()); renaming = null }, enabled = renameTitle.trim().isNotEmpty()) { Text(I18n.t("error.ok")) }
        }, dismissButton = { TextButton(onClick = { renaming = null }) { Text(I18n.t("settings.cancel")) } })
    }
    if (showSortDisplay) LibrarySortDisplaySheet(
        sortTitle = sortTitle, listMode = listMode,
        onSort = chooseSort, onDisplay = chooseDisplay,
        onDismiss = { showSortDisplay = false },
    )
}

@Composable
private fun BookCard(
    entry: BookEntry,
    coverBytes: ByteArray?,
    onClick: () -> Unit,
    onDelete: () -> Unit,
    managing: Boolean = false,
    selected: Boolean = false,
    listMode: Boolean = false,
    onBounds: (Rect) -> Unit = {},
    onShare: () -> Unit = {},
    onRename: () -> Unit = {},
    modifier: Modifier = Modifier,
) {
    val bitmap by
        produceState<android.graphics.Bitmap?>(null, coverBytes) {
            value =
                withContext(Dispatchers.IO) {
                    coverBytes?.let { BitmapFactory.decodeByteArray(it, 0, it.size) }
                }
        }
    if (listMode) {
        Column(modifier) {
            Row(Modifier.fillMaxWidth().clickable(onClick = onClick).padding(vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Box(Modifier.size(52.dp, 78.dp).onGloballyPositioned { onBounds(it.boundsInRoot()) }
                    .clip(ReaderGeometry.Shape).background(MaterialTheme.colorScheme.surfaceVariant), contentAlignment = Alignment.Center) {
                    bitmap?.let { Image(it.asImageBitmap(), null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop) }
                        ?: Icon(Icons.AutoMirrored.Filled.MenuBook, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text(entry.title, style = MaterialTheme.typography.titleMedium, maxLines = 2, overflow = TextOverflow.Ellipsis)
                    Text(entry.lastChapterTitle ?: I18n.tf1("ui.chapter", "${entry.lastChapter+1}"), style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                if (managing) Checkbox(checked = selected, onCheckedChange = { onClick() })
                else BookActionsMenu(onShare, onRename, onDelete)
            }
            HorizontalDivider(Modifier.padding(start = 64.dp), color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.45f))
        }
        return
    }
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Box(
            Modifier.fillMaxWidth()
                .aspectRatio(2f / 3f)
                .onGloballyPositioned { onBounds(it.boundsInRoot()) }
                .clip(ReaderGeometry.Shape)
                .background(MaterialTheme.colorScheme.surfaceVariant)
                .clickable(onClick = onClick),
            contentAlignment = Alignment.Center,
        ) {
            bitmap?.let {
                Image(
                    it.asImageBitmap(),
                    entry.title,
                    modifier = Modifier.fillMaxSize(),
                    contentScale = ContentScale.Crop,
                )
            }
                ?: Column(
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(ReaderGeometry.BlockGap),
                ) {
                    Icon(
                        Icons.AutoMirrored.Filled.MenuBook,
                        null,
                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Text(I18n.t("ui.cover_missing"), style = MaterialTheme.typography.bodySmall)
                }
        }
        if (managing) Row(Modifier.fillMaxWidth().clickable(onClick = onClick), verticalAlignment = Alignment.CenterVertically) {
            Checkbox(checked = selected, onCheckedChange = { onClick() }); Text(entry.title, maxLines = 2, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.titleSmall)
        }
        else Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.Top) {
            Text(
                entry.title,
                style = MaterialTheme.typography.titleSmall,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f).clickable(onClick = onClick),
            )
            BookActionsMenu(onShare, onRename, onDelete)
        }
        Text(
            entry.lastChapterTitle ?: I18n.tf1("ui.chapter", "${entry.lastChapter+1}"),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun BookActionsMenu(onShare: () -> Unit, onRename: () -> Unit, onDelete: () -> Unit) {
    var more by remember { mutableStateOf(false) }
    Box {
        IconButton(onClick = { more = true }, modifier = Modifier.size(48.dp)) { Icon(Icons.Default.MoreHoriz, I18n.t("ui.more"), Modifier.size(20.dp)) }
        DropdownMenu(more, { more = false }) {
            DropdownMenuItem(text = { Text(I18n.t("book.share")) }, onClick = { more = false; onShare() })
            DropdownMenuItem(text = { Text(I18n.t("book.rename")) }, onClick = { more = false; onRename() })
            HorizontalDivider()
            DropdownMenuItem(text = { Text(I18n.t("archive.action")) }, onClick = { more = false; onDelete() })
        }
    }
}
