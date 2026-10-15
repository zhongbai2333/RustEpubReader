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
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.*
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.zIndex
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.asImageBitmap
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
    coverCache: Map<String, ByteArray?>,
    language: String,
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
    onUpdateLanguage: (String) -> Unit = {},
    onRefreshLibrary: () -> Unit = {},
    onOpenAbout: () -> Unit = {},
    onOpenSync: () -> Unit = {},
    onOpenStatistics: () -> Unit = {},
    onToggleTheme: () -> Unit = {},
    syncing: Boolean = false,
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
    var query by rememberSaveable { mutableStateOf("") }
    var sortTitle by rememberSaveable { mutableStateOf(false) }
    var showMore by remember { mutableStateOf(false) }
    var showLanguage by remember { mutableStateOf(false) }
    var refreshing by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val ranked=if(sortTitle) books.sortedBy {it.title.lowercase()} else books.sortedByDescending {it.lastOpened}
    val ranks=shelfOrder.resolve(ranked.map {it.uri},sortTitle,shelfRevision)
        .withIndex().associate {it.value to it.index}
    val filtered=ranked.sortedBy {ranks[it.uri] ?: Int.MAX_VALUE}.filter {it.title.contains(query,ignoreCase=true)}
    val recent = books.maxByOrNull { it.lastOpened }?.takeIf { it.lastOpened > 0 }
    Scaffold(
        containerColor = MaterialTheme.colorScheme.background,
        topBar = {
            TopAppBar(
                title = {
                    Column {
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
                                Text(I18n.t("cloud.syncing_indicator"), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                        }
                        Text(
                            I18n.count("ui.library_count", books.size),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                },
                actions = {
                    TextButton(onClick = { managing = !managing; if (!managing) selected = emptyList() }, enabled = books.isNotEmpty()) { Text(I18n.t(if (managing) "manage.done" else "manage.books")) }
                    Box {
                        IconButton(onClick = { showMore = true }) {
                            Icon(Icons.Default.MoreVert, I18n.t("ui.more"))
                        }
                        DropdownMenu(showMore, { showMore = false }) {
                            DropdownMenuItem(text={Text(I18n.t("transfer.receive"))},onClick={showMore=false;onReceiveBooks()})
                            DropdownMenuItem(text={Text(I18n.t("stats.title"))},onClick={showMore=false;onOpenStatistics()})
                            DropdownMenuItem(
                                text = { Text(I18n.t("ui.language")) },
                                onClick = {
                                    showMore = false
                                    showLanguage = true
                                },
                            )
                            DropdownMenuItem(
                                text = { Text(I18n.t("ui.appearance")) },
                                onClick = {
                                    showMore = false
                                    onToggleTheme()
                                },
                            )
                            DropdownMenuItem(
                                text = { Text(I18n.t("about.title")) },
                                onClick = {
                                    showMore = false
                                    onOpenAbout()
                                },
                            )
                        }
                    }
                },
            )
        },
        bottomBar = { HomeNavigation(false, {}, onOpenSync) },
    ) { padding ->
        PullToRefreshBox(
            isRefreshing = refreshing,
            onRefresh = {
                scope.launch {
                    refreshing = true
                    try {
                        shelfOrder.reset()
                        onRefreshLibrary()
                    } finally {
                        refreshing = false
                    }
                }
            },
            modifier = Modifier.fillMaxSize().padding(padding),
        ) {
            LazyVerticalGrid(
                columns = GridCells.Adaptive(144.dp),
                contentPadding = PaddingValues(ReaderGeometry.PageInset),
                horizontalArrangement = Arrangement.spacedBy(ReaderGeometry.BlockGap),
                verticalArrangement = Arrangement.spacedBy(ReaderGeometry.SectionGap),
            ) {
                item(span = { GridItemSpan(maxLineSpan) }) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(ReaderGeometry.BlockGap),
                    ) {
                        OutlinedTextField(
                            value = query,
                            onValueChange = { query = it },
                            placeholder = { Text(I18n.t("ui.search_books")) },
                            leadingIcon = { Icon(Icons.Default.Search, null) },
                            singleLine = true,
                            modifier = Modifier.weight(1f),
                            shape = ReaderGeometry.Shape,
                        )
                        Button(
                            onClick = onOpenFilePicker,
                            contentPadding = ReaderGeometry.ButtonPadding,
                            modifier = Modifier.height(56.dp),
                            shape = ReaderGeometry.Shape,
                        ) {
                            Text(I18n.t("ui.import"))
                        }
                    }
                }
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
                                    onClick = { onCoverBounds(recent.uri, Rect.Zero); onOpenBook(recent.uri, recent.lastChapter) },
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
                        TextButton(
                            onClick = { sortTitle = !sortTitle },
                            shape = ReaderGeometry.Shape,
                            contentPadding = ReaderGeometry.ButtonPadding,
                        ) {
                            Text(
                                I18n.t(if (sortTitle) "ui.sort_title" else "ui.sort_recent"),
                                style = MaterialTheme.typography.labelMedium,
                            )
                        }
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
                                I18n.t(if (books.isEmpty()) "library.empty" else "ui.no_results"),
                                style = MaterialTheme.typography.titleMedium,
                            )
                            if (books.isEmpty()) {
                                Text(I18n.t("ui.empty_hint"))
                                Button(
                                    onClick = onOpenFilePicker,
                                    shape = ReaderGeometry.Shape,
                                    contentPadding = ReaderGeometry.ButtonPadding,
                                ) {
                                    Text(I18n.t("ui.import"))
                                }
                            } else
                                TextButton(
                                    onClick = { query = "" },
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
                            TextButton(onClick = { confirmDelete = true }, enabled = selected.isNotEmpty(), colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error)) { Text(I18n.t("manage.delete")) }
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
                        { if (managing) selected = if (entry.uri in selected) selected - entry.uri else selected + entry.uri else onOpenBook(entry.uri, entry.lastChapter) },
                        { onRemoveBook(entry.uri) },
                        managing, entry.uri in selected,
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
    if (confirmDelete) AlertDialog(onDismissRequest = { confirmDelete = false }, title = { Text(I18n.t("manage.confirm")) }, text = { Text(I18n.t("manage.delete_note")) }, confirmButton = {
        TextButton(onClick = { val deleting = selected; confirmDelete = false; selected = emptyList(); deleting.forEach(onRemoveBook) }) { Text(I18n.t("manage.delete"), color = MaterialTheme.colorScheme.error) }
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
    if (showLanguage)
        AlertDialog(
            onDismissRequest = { showLanguage = false },
            title = { Text(I18n.t("ui.language")) },
            text = {
                Column {
                    I18n.availableLanguages.forEach { (code, label) ->
                        Row(
                            Modifier.fillMaxWidth().clickable {
                                onUpdateLanguage(code)
                                showLanguage = false
                            },
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            RadioButton(
                                selected = language == code,
                                onClick = {
                                    onUpdateLanguage(code)
                                    showLanguage = false
                                },
                            )
                            Text(label)
                        }
                    }
                }
            },
            confirmButton = {
                TextButton(
                    onClick = { showLanguage = false },
                    shape = ReaderGeometry.Shape,
                    contentPadding = ReaderGeometry.ButtonPadding,
                ) {
                    Text(I18n.t("error.ok"))
                }
            },
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
    onBounds: (Rect) -> Unit = {},
    onShare: () -> Unit = {},
    onRename: () -> Unit = {},
    modifier: Modifier = Modifier,
) {
    var more by remember { mutableStateOf(false) }
    val bitmap by
        produceState<android.graphics.Bitmap?>(null, coverBytes) {
            value =
                withContext(Dispatchers.IO) {
                    coverBytes?.let { BitmapFactory.decodeByteArray(it, 0, it.size) }
                }
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
            Box {
                IconButton(onClick = { more = true }, modifier = Modifier.size(48.dp)) {
                    Icon(Icons.Default.MoreVert, I18n.t("ui.more"), modifier = Modifier.size(20.dp))
                }
                DropdownMenu(more, { more = false }) {
                    DropdownMenuItem(text = { Text(I18n.t("book.share")) }, onClick = { more = false; onShare() })
                    DropdownMenuItem(text = { Text(I18n.t("book.rename")) }, onClick = { more = false; onRename() })
                    HorizontalDivider()
                    DropdownMenuItem(
                        text = { Text(I18n.t("library.delete")) },
                        onClick = {
                            more = false
                            onDelete()
                        },
                    )
                }
            }
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
