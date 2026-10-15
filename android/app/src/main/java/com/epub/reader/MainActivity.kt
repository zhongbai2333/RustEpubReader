package com.zhongbai233.epub.reader

import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.*
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.viewmodel.compose.viewModel
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.ui.library.AboutScreen
import com.zhongbai233.epub.reader.ui.library.LibraryScreen
import com.zhongbai233.epub.reader.ui.library.SyncScreen
import com.zhongbai233.epub.reader.ui.library.TxtImportDialog
import com.zhongbai233.epub.reader.ui.reader.AnnotationsSheet
import com.zhongbai233.epub.reader.ui.reader.ContributeDialog
import com.zhongbai233.epub.reader.ui.reader.GithubSyncSettings
import com.zhongbai233.epub.reader.ui.reader.MultiPlatformSyncDialog
import com.zhongbai233.epub.reader.ui.reader.ReaderScreen
import com.zhongbai233.epub.reader.ui.reader.ReviewPanel
import com.zhongbai233.epub.reader.ui.reader.SearchDialog
import com.zhongbai233.epub.reader.ui.reader.TocDrawerContent
import com.zhongbai233.epub.reader.ui.theme.EpubReaderTheme
import com.zhongbai233.epub.reader.viewmodel.ReaderViewModel
import kotlinx.coroutines.launch
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.zIndex
import com.zhongbai233.epub.reader.ui.library.BookPresentation
import com.zhongbai233.epub.reader.ui.library.BookPresentationOverlay
import com.zhongbai233.epub.reader.ui.library.readerPaperColor
import androidx.compose.ui.input.pointer.pointerInput
import com.zhongbai233.epub.reader.ui.reader.ReadingStatsSheet

class MainActivity : ComponentActivity() {
    private var pendingShareLink by mutableStateOf<String?>(null)

    override fun onNewIntent(intent: android.content.Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        pendingShareLink = intent.data?.takeIf { it.scheme == "rustepubreader" }?.toString()
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        pendingShareLink = intent?.data?.takeIf { it.scheme == "rustepubreader" }?.toString()
        enableEdgeToEdge()

        setContent {
            val vm: ReaderViewModel = viewModel()
            val systemDark = androidx.compose.foundation.isSystemInDarkTheme()
            val ambient = com.zhongbai233.epub.reader.ui.reader.automaticReadingAppearance(vm.appearanceLinkage.usesAmbient)
            SideEffect { vm.updateAppearanceSignals(systemDark,ambient.dark,ambient.available) }

            EpubReaderTheme(darkTheme = vm.appearanceLinkage.appDark(systemDark,ambient.dark)) {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background,
                ) {
                    MainContent(vm, pendingShareLink) { pendingShareLink = null }
                }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
private fun MainContent(vm: ReaderViewModel, shareLink: String? = null, consumeShareLink: () -> Unit = {}) {
    // 读取 I18n.version 以确保语言切换时触发重组
    @Suppress("UNUSED_VARIABLE") val langVersion = I18n.version
    val scope = rememberCoroutineScope()
    val context = androidx.compose.ui.platform.LocalContext.current
    val uriHandler = LocalUriHandler.current
    val bookMotion = remember { BookPresentation(scope) }
    val automaticDark = vm.readingDark
    val ambientSensorAvailable = vm.ambientSensorAvailable
    val readerDark = vm.readingDark
    val reducedMotion = !android.animation.ValueAnimator.areAnimatorsEnabled()
    fun closeReader(after: () -> Unit = {}) {
        vm.cancelPendingBookOpen()
        bookMotion.close(reducedMotion, { vm.closeBook() }, after)
    }
    LaunchedEffect(shareLink) {
        if (shareLink != null) {
            val receive = { vm.bookSharing.cancel(); vm.receivingSharedBooks = true; vm.bookSharing.receive(shareLink) }
            if (vm.currentBook != null) closeReader(receive) else receive()
            consumeShareLink()
        }
    }
    LaunchedEffect(vm.isLoading, vm.currentBook, vm.errorMessage) {
        if(bookMotion.active && !bookMotion.closing && !vm.isLoading && vm.currentBook==null && vm.errorMessage!=null) closeReader()
    }

    // SAF 文件选择器：支持一次选择多本 EPUB。单个 TXT 仍走 TXT 导入流程。
    val filePicker =
        rememberLauncherForActivityResult(
            contract = ActivityResultContracts.OpenMultipleDocuments()
        ) { uris: List<Uri> ->
            if (uris.isNotEmpty()) {
                vm.importBooksFromUris(uris)
            }
        }

    val backgroundImagePicker =
        rememberLauncherForActivityResult(contract = ActivityResultContracts.OpenDocument()) {
            uri: Uri? ->
            if (uri != null) {
                runCatching { context.contentResolver.takePersistableUriPermission(uri, android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION) }
                vm.updateReaderBgImage(uri.toString())
            }
        }

    // 目录抽屉状态
    val drawerState = rememberDrawerState(DrawerValue.Closed)

    // 共享对话框状态
    var homeTab by androidx.compose.runtime.saveable.rememberSaveable { mutableStateOf(0) }
    var libraryQuery by androidx.compose.runtime.saveable.rememberSaveable { mutableStateOf("") }
    var librarySearching by androidx.compose.runtime.saveable.rememberSaveable { mutableStateOf(false) }
    LaunchedEffect(homeTab) { if(homeTab != 0) bookMotion.shelfOrder.reset() }
    var showMultiPlatformSyncDialog by remember { mutableStateOf(false) }
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    DisposableEffect(lifecycle, vm) {
        val observer = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_RESUME) {I18n.refreshSystemLanguage();vm.activityTracker.foreground=true;vm.activityTracker.touch()}
            if (event == Lifecycle.Event.ON_PAUSE) {vm.activityTracker.foreground=false;vm.activityTracker.pause()}
            if (event == Lifecycle.Event.ON_START) vm.setGithubSyncForeground(true)
            if (event == Lifecycle.Event.ON_STOP) { vm.setGithubSyncForeground(false); vm.bookSharing.cancel() }
        }
        lifecycle.addObserver(observer)
        vm.setGithubSyncForeground(lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED))
        vm.activityTracker.foreground=lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)
        onDispose {
            lifecycle.removeObserver(observer)
            vm.setGithubSyncForeground(false)
            vm.activityTracker.foreground=false;vm.activityTracker.pause()
        }
    }
    if (showMultiPlatformSyncDialog) {
        MultiPlatformSyncDialog(vm, onDismiss = { showMultiPlatformSyncDialog = false })
    }
    if (vm.receivingSharedBooks) com.zhongbai233.epub.reader.ui.library.BookShareReceiver(vm.bookSharing) { vm.receivingSharedBooks = false }

    // 关于界面状态
    var showAboutScreen by remember { mutableStateOf(false) }
    var showAppearanceLinkage by remember { mutableStateOf(false) }
    if(showAppearanceLinkage) AlertDialog(onDismissRequest={showAppearanceLinkage=false},
        title={Text(I18n.t("appearance.link_title"))},
        text={Column(Modifier.verticalScroll(rememberScrollState())) {
            com.zhongbai233.epub.reader.ui.reader.AppearanceLinkageSettings(vm.appearanceLinkage,vm.systemAppearanceDark,vm.ambientAppearanceDark,vm.ambientSensorAvailable,vm::updateAppearanceLinkage,showTitle=false)
        }},
        confirmButton={TextButton(onClick={showAppearanceLinkage=false}) {Text(I18n.t("settings.done"))}})
    var preparingBookExport by remember { mutableStateOf(false) }

    val currentBook = vm.currentBook
    val readerPaper=androidx.compose.ui.graphics.Color(vm.readingAppearance.style.variant(readerDark).paper)
    val paperCoversBars = if(bookMotion.active) bookMotion.expansion.value >= 0.985f else currentBook != null
    LaunchedEffect(currentBook,vm.isDarkMode,readerPaper,bookMotion.active,paperCoversBars) {
        val controller=androidx.core.view.WindowCompat.getInsetsController((context as android.app.Activity).window,(context as android.app.Activity).window.decorView)
        val light=if(paperCoversBars) readerPaper.luminance()>=0.45f else !vm.isDarkMode
        controller.isAppearanceLightStatusBars=light; controller.isAppearanceLightNavigationBars=light
        if(currentBook==null && !bookMotion.active) controller.show(androidx.core.view.WindowInsetsCompat.Type.systemBars())
    }
    SideEffect { vm.activityTracker.blocked = vm.isLoading || currentBook==null || vm.showStatistics || vm.showSearch || vm.showAnnotationsPanel || vm.errorMessage!=null }
    if(vm.showStatistics) ReadingStatsSheet(currentBook!=null,vm.statisticsCurrentBook,
        load={vm.loadStatistics(it)},onDismiss={vm.showStatistics=false},
        onSyncSettings={vm.showStatistics=false;closeReader { homeTab=1 }})

    BackHandler(
        enabled =
            vm.showStatistics || vm.showTxtImport ||
                homeTab != 0 ||
                vm.showContributeDialog ||
                vm.showAnnotationsPanel ||
                vm.showSearch ||
                drawerState.isOpen ||
                currentBook != null || bookMotion.active ||
                showAboutScreen
    ) {
        when {
            vm.showStatistics -> vm.showStatistics = false
            vm.showTxtImport -> vm.dismissTxtImport()
            homeTab != 0 && currentBook == null -> homeTab = 0
            vm.showContributeDialog -> vm.dismissContributeDialog()
            vm.showAnnotationsPanel -> vm.showAnnotationsPanel = false
            vm.showSearch -> vm.showSearch = false
            drawerState.isOpen -> scope.launch { drawerState.close() }
            currentBook != null || bookMotion.active -> closeReader()
            showAboutScreen -> showAboutScreen = false
        }
    }

    // 错误提示
    vm.errorMessage?.let { msg ->
        AlertDialog(
            onDismissRequest = { vm.dismissError() },
            confirmButton = {
                TextButton(onClick = { vm.dismissError() }) { Text(I18n.t("error.ok")) }
            },
            title = { Text(I18n.t("error.title")) },
            text = { Text(I18n.message(msg)) },
        )
    }

    // 更新提示弹窗
    if (vm.showUpdateDialog) {
        vm.updateInfo?.let { info ->
            AlertDialog(
                onDismissRequest = { vm.dismissUpdateDialog() },
                confirmButton = {
                    FlowRow(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        TextButton(
                            onClick = {
                                vm.dismissUpdateDialog()
                                runCatching { uriHandler.openUri(info.cdnDownloadUrl) }
                            }
                        ) {
                            Text(I18n.t("update.download_cdn"))
                        }
                        TextButton(
                            onClick = {
                                vm.dismissUpdateDialog()
                                runCatching { uriHandler.openUri(info.githubDownloadUrl) }
                            }
                        ) {
                            Text(I18n.t("update.download_github"))
                        }
                        TextButton(onClick = { vm.dismissUpdateDialog() }) {
                            Text(I18n.t("feedback.not_now"))
                        }
                    }
                },
                title = { Text(I18n.t("update.check")) },
                text = { Text(I18n.tf1("update.new_version", info.tagName)) },
            )
        }
    }

    // 主内容 + 加载遮罩叠加
    Box(Modifier.fillMaxSize().pointerInput(vm) {
        awaitPointerEventScope { while(true) {val event=awaitPointerEvent();if(event.changes.any {it.pressed || it.previousPressed}) vm.activityTracker.touch()} }
    }) {
        // 主内容（加载完成后显示）
        if (!vm.isLoading || bookMotion.active) {
            val book = currentBook

            Box(Modifier.fillMaxSize().graphicsLayer { alpha=if(book==null || bookMotion.showShelf) 1f else 0f }.then(if(book!=null) Modifier.clearAndSetSemantics { } else Modifier)) {
                if (showAboutScreen) {
                    // ---- 关于界面 ----
                    AboutScreen(
                        onNavigateBack = { showAboutScreen = false },
                        onExportLogs = { vm.exportFeedbackLogs() },
                    )
                } else {
                    // ---- 书库界面 ----
                    if (homeTab == 1) {
                        SyncScreen(vm, onLibrary = { homeTab = 0 }, onSearch = { homeTab = 0; librarySearching = true })
                    } else {
                        LibraryScreen(
                            active = book == null && !bookMotion.active,
                            query = libraryQuery, searchActive = librarySearching,
                            onQueryChange = { libraryQuery = it }, onSearchActiveChange = { librarySearching = it },
                            shelfOrder = bookMotion.shelfOrder,
                            shelfRevision = bookMotion.shelfRevision,
                            promotedUri = bookMotion.sourceUri,
                            reduceShelfMotion = reducedMotion,
                            syncing = vm.githubSyncRunning,
                            onCoverBounds = { uri, rect -> if(rect.width>0 && rect.height>0) bookMotion.covers[uri]=rect else bookMotion.covers.remove(uri) },
                            onOpenSync = { homeTab = 1 },
                            onOpenStatistics = { vm.openStatistics(false) },
                            onToggleTheme = { showAppearanceLinkage = true },
                            books = vm.books,
                            coverCache = vm.coverCache,
                            language = vm.readerLanguage,
                            shareModel = vm.bookSharing,
                            githubToken = vm.githubToken,
                            githubAuthorized = vm.githubRepoAuthorized,
                            onGithubLogin = { homeTab = 1; vm.startGitHubLogin(privateSync = true, enableAfterLogin = false) },
                            onReceiveBooks = { vm.bookSharing.resetReceiver(); vm.receivingSharedBooks = true },
                            onRenameBook = { uri, title -> vm.renameBook(uri, title) },
                            exportingBooks = preparingBookExport,
                            onExportBooks = { paths ->
                                preparingBookExport = true
                                scope.launch {
                                    try {
                                        val uris = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) {
                                            val dir = java.io.File(context.cacheDir, "exports/${java.util.UUID.randomUUID()}")
                                            val request = org.json.JSONArray(paths).toString()
                                            val result = org.json.JSONObject(RustBridge.prepareBookExports(context.filesDir.absolutePath, request, dir.absolutePath) ?: error("Reader core returned no result"))
                                            check(result.optBoolean("ok")) { result.optString("error") }
                                            val exported = result.getJSONArray("paths")
                                            (0 until exported.length()).map { index ->
                                                val file = java.io.File(exported.getString(index))
                                                androidx.core.content.FileProvider.getUriForFile(context, "${context.packageName}.exports", file)
                                            }
                                        }
                                        val intent = android.content.Intent(if (uris.size == 1) android.content.Intent.ACTION_SEND else android.content.Intent.ACTION_SEND_MULTIPLE).apply {
                                            type = "application/epub+zip"
                                            if (uris.size == 1) putExtra(android.content.Intent.EXTRA_STREAM, uris.first())
                                            else putParcelableArrayListExtra(android.content.Intent.EXTRA_STREAM, ArrayList(uris))
                                            addFlags(android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION)
                                            clipData = android.content.ClipData.newUri(context.contentResolver, "EPUB", uris.first()).apply { uris.drop(1).forEach { addItem(android.content.ClipData.Item(it)) } }
                                        }
                                        context.startActivity(android.content.Intent.createChooser(intent, I18n.t("book.export")))
                                    } catch (error: Exception) { vm.reportExportFailure(error.message ?: I18n.t("common.error_unknown")) }
                                    finally { preparingBookExport = false }
                                }
                            },
                            onRequestCover = { uri -> vm.requestCover(uri) },
                            onOpenFilePicker = {
                                filePicker.launch(
                                    arrayOf("application/epub+zip", "application/epub", "*/*")
                                )
                            },
                            onOpenBook = { uri, chapter ->
                                bookMotion.open(uri,reducedMotion) { vm.openFromPath(uri, chapter) }
                            },
                            onRemoveBook = { uri ->
                                vm.removeBookByUri(uri)
                            },
                            onUpdateLanguage = { code ->
                                vm.updateLanguage(code)
                            },
                            onRefreshLibrary = {
                                vm.refreshLibrary().join()
                            },
                            onOpenAbout = { showAboutScreen = true },
                        )
                    }
                } // end else (showAboutScreen)

                // ── TXT 导入对话框 ──
                TxtImportDialog(
                    show = vm.showTxtImport,
                    title = vm.txtImportTitle,
                    author = vm.txtImportAuthor,
                    customRegex = vm.txtImportRegex,
                    useHeuristic = vm.txtImportHeuristic,
                    previews = vm.txtImportPreviews,
                    converting = vm.txtImportConverting,
                    error = vm.txtImportError,
                    onTitleChange = { vm.txtImportTitle = it },
                    onAuthorChange = { vm.txtImportAuthor = it },
                    onRegexChange = { vm.txtImportRegex = it },
                    onHeuristicChange = { vm.txtImportHeuristic = it },
                    onRefreshPreview = { vm.refreshTxtPreviews() },
                    onConvert = { vm.convertTxtToEpub() },
                    onDismiss = { vm.dismissTxtImport() },
                )
            }
            if (book != null && (!bookMotion.active || bookMotion.mountReader)) {
                Box(Modifier.fillMaxSize().zIndex(1f).graphicsLayer { alpha=if(bookMotion.active && bookMotion.showShelf) 0f else 1f }) {
                    // ---- 阅读器界面（包裹在目录抽屉中）----
                    ModalNavigationDrawer(
                        drawerState = drawerState,
                        gesturesEnabled = drawerState.isOpen,
                        drawerContent = {
                            ModalDrawerSheet {
                                TocDrawerContent(
                                    toc = book.toc,
                                    currentChapter = vm.currentChapter,
                                    language = vm.readerLanguage,
                                    onSelectChapter = { idx ->
                                        vm.goToChapter(idx)
                                        scope.launch { drawerState.close() }
                                    },
                                    onClose = {
                                        scope.launch { drawerState.close() }
                                    },
                                )
                            }
                        },
                    ) {
                        val chapter =
                            if (book.chapters.isNotEmpty()) {
                                book.chapters[vm.currentChapter.coerceIn(0, book.chapters.size - 1)]
                            } else null

                        ReaderScreen(
                            onInitialLayout = { bookMotion.ready=true },
                            introductionRequested = !vm.readerIntroductionSeen && !bookMotion.active && !bookMotion.showShelf,
                            onIntroductionShown = vm::markReaderIntroductionShown,
                            correctionContributionContent = { closeCorrection ->
                                Column(verticalArrangement=Arrangement.spacedBy(8.dp)) {
                                    Text(I18n.t("account.contribution"),style=MaterialTheme.typography.titleMedium)
                                    Text(I18n.t("account.contribution_hint"),style=MaterialTheme.typography.bodySmall)
                                    OutlinedButton(onClick={closeCorrection();vm.prepareContribution();vm.showContributeDialog=true}) {Text(I18n.t("csc.contribute_title"))}
                                }
                            },
                            appearance = vm.readingAppearance,
                            appearanceLinkage = vm.appearanceLinkage,
                            onLinkageChange = vm::updateAppearanceLinkage,
                            systemAppearanceDark = vm.systemAppearanceDark,
                            ambientAppearanceDark = vm.ambientAppearanceDark,
                            automaticAppearanceDark = automaticDark,
                            ambientSensorAvailable = ambientSensorAvailable,
                            onAppearanceChange = vm::updateReadingAppearance,
                            book = book,
                            bookPath = vm.originalBookPath,
                            currentChapter = vm.currentChapter,
                            currentBlock = vm.currentBlock,
                            fontSize = vm.fontSize,
                            isDarkMode = vm.isDarkMode,
                            scrollMode = vm.isScrollMode,
                            bgColorIndex = vm.readerBgColorIndex,
                            customBgColorArgb = vm.readerCustomBgColorArgb,
                            fontColorIndex = vm.readerFontColorIndex,
                            customFontColorArgb = vm.readerCustomFontColorArgb,
                            fontFamilyName = vm.readerFontFamily,
                            pageAnimation = vm.readerPageAnimation,
                            bgImageUri = vm.readerBgImageUri,
                            bgImageAlpha = vm.readerBgImageAlpha,
                            language = vm.readerLanguage,
                            showImmersiveStatus = vm.showImmersiveStatus,
                            systemFonts = vm.systemFonts,
                            showToc = drawerState.isOpen,
                            onNavigateBack = { closeReader() },
                            onChapterChange = { vm.goToChapter(it) },
                            onPositionChange = { vm.updateReadingPosition(it) },
                            previousChapter = vm.previousChapter,
                            onGoBackChapter = { vm.goBackChapter() },
                            onFontSizeChange = { vm.updateFontSize(it) },
                            onToggleDarkMode = { vm.updateDarkMode(!vm.isDarkMode) },
                            onToggleScrollMode = { vm.updateScrollMode(!vm.isScrollMode) },
                            onUpdateScrollMode = { vm.updateScrollMode(it) },
                            onUpdateDarkMode = { vm.updateDarkMode(it) },
                            onUpdateBgColor = { vm.updateReaderBgColor(it) },
                            onUpdateCustomBgColor = { vm.updateReaderCustomBgColor(it) },
                            onUpdateFontColor = { vm.updateReaderFontColor(it) },
                            onUpdateCustomFontColor = { vm.updateReaderCustomFontColor(it) },
                            onUpdateFontFamily = { vm.updateReaderFontFamily(it) },
                            onUpdatePageAnimation = { vm.updateReaderPageAnimation(it) },
                            onUpdateBgImageAlpha = { vm.updateReaderBgImageAlpha(it) },
                            onUpdateLanguage = { vm.updateLanguage(it) },
                            onUpdateShowImmersiveStatus = { vm.updateShowImmersiveStatus(it) },
                            onOpenBackgroundPicker = {
                                backgroundImagePicker.launch(arrayOf("image/*"))
                            },
                            onClearBackgroundImage = { vm.updateReaderBgImage(null) },
                            onToggleToc = {
                                scope.launch { drawerState.open() }
                            },
                            toolsOverlayVisible = vm.showSearch || vm.showAnnotationsPanel || vm.showStatistics,
                            onToggleSearch = { vm.showSearch = !vm.showSearch },
                            onShowStatistics = { vm.openStatistics(true) },
                            onToggleDayNight = { vm.toggleReaderDayNight(readerDark) },
                            onReadingActivityEnabled = { vm.activityTracker.enabled=it && !vm.showStatistics && !vm.showSearch && !vm.showAnnotationsPanel },
                            isChapterBookmarked = vm.isChapterBookmarked,
                            onToggleBookmark = { vm.toggleBookmark() },
                            onShowAnnotations = { vm.showAnnotationsPanel = true },
                            highlights = vm.bookConfig?.highlights ?: emptyList(),
                            notes = vm.bookConfig?.notes ?: emptyList(),
                            onAddHighlight = { ch, sb, so, eb, eo, color ->
                                vm.addHighlight(ch, sb, so, eb, eo, color)
                            },
                            onSaveNote = { hlId, content -> vm.saveNote(hlId, content) },
                            lineSpacing = vm.lineSpacing,
                            paraSpacing = vm.paraSpacing,
                            textIndent = vm.textIndent,
                            titleFontScale = vm.titleFontScale,
                            onLineSpacingChange = { vm.updateLineSpacing(it) },
                            onParaSpacingChange = { vm.updateParaSpacing(it) },
                            onTextIndentChange = { vm.updateTextIndent(it) },
                            onTitleFontScaleChange = { vm.updateTitleFontScale(it) },
                            translateApiUrl = vm.translateApiUrl,
                            translateApiKey = vm.translateApiKey,
                            dictionaryApiUrl = vm.dictionaryApiUrl,
                            dictionaryApiKey = vm.dictionaryApiKey,
                            onTranslateApiUrlChange = { vm.updateTranslateApiUrl(it) },
                            onTranslateApiKeyChange = { vm.updateTranslateApiKey(it) },
                            onDictionaryApiUrlChange = { vm.updateDictionaryApiUrl(it) },
                            onDictionaryApiKeyChange = { vm.updateDictionaryApiKey(it) },
                            ttsVoiceName = vm.ttsVoiceName,
                            ttsRate = vm.ttsRate,
                            ttsVolume = vm.ttsVolume,
                            onTtsVoiceNameChange = { vm.updateTtsVoiceName(it) },
                            onTtsRateChange = { vm.updateTtsRate(it) },
                            onTtsVolumeChange = { vm.updateTtsVolume(it) },
                            // TTS playback
                            showTtsBar = vm.showTtsBar,
                            ttsPlaying = vm.ttsManager.playing.collectAsState().value,
                            ttsPaused = vm.ttsManager.paused.collectAsState().value,
                            ttsStatus = vm.ttsManager.status.collectAsState().value,
                            ttsCurrentBlock = vm.ttsManager.currentBlock.collectAsState().value,
                            onTtsPlay = { vm.ttsStartPlayback() },
                            onTtsPause = { vm.ttsTogglePause() },
                            onTtsResume = { vm.ttsTogglePause() },
                            onTtsStop = { vm.ttsStopPlayback() },
                            onTtsClose = { vm.ttsCloseTtsBar() },
                            cscMode = vm.cscMode,
                            cscThreshold = vm.cscThreshold,
                            onCscModeChange = { vm.updateCscMode(it) },
                            onCscThresholdChange = { vm.updateCscThreshold(it) },
                            cscModelReady = vm.cscModelReady,
                            cscModelLoading = vm.cscModelLoading,
                            cscCorrections = vm.cscCorrections,
                            onDownloadCscModel = { vm.downloadCscModel() },
                            onCscCorrectionStatusChange = { correction, status ->
                                vm.updateCorrectionStatus(correction, status)
                            },
                            // 段评
                            reviewChapterIndices = vm.reviewChapterIndices,
                            showReviewPanel = vm.showReviewPanel,
                            reviewPanelChapter = vm.reviewPanelChapter,
                            onOpenReviewPanel = { chapter, anchor ->
                                vm.openReviewPanel(chapter, anchor)
                            },
                            onCloseReviewPanel = { vm.closeReviewPanel() },
                        )

                        // 段评面板返回键拦截
                        BackHandler(enabled = vm.showReviewPanel) {
                            vm.closeReviewPanel()
                        }

                        // 段评面板
                        if (vm.showReviewPanel && vm.reviewPanelChapter != null) {
                            val reviewCh = vm.reviewPanelChapter?.let { book.chapters.getOrNull(it) }
                            if (reviewCh != null) {
                                ReviewPanel(
                                    chapterTitle = reviewCh.title,
                                    blocks = reviewCh.blocks,
                                    anchorId = vm.reviewPanelAnchor,
                                    fontSize = vm.fontSize,
                                    showAll = vm.reviewPanelShowAll,
                                    onShowAllChanged = { vm.reviewPanelShowAll = it },
                                    onDismiss = { vm.closeReviewPanel() },
                                )
                            }
                        }

                        // 标注面板
                        if (vm.showAnnotationsPanel) {
                            AnnotationsSheet(
                                bookmarks = vm.bookConfig?.bookmarks ?: emptyList(),
                                highlights = vm.bookConfig?.highlights ?: emptyList(),
                                notes = vm.bookConfig?.notes ?: emptyList(),
                                corrections = vm.bookConfig?.corrections ?: emptyList(),
                                chapters = book.chapters.map { it.title },
                                onNavigateToChapter = { ch ->
                                    vm.goToChapter(ch)
                                    vm.showAnnotationsPanel = false
                                },
                                onRemoveBookmark = { chapter ->
                                    vm.removeBookmarkForChapter(chapter)
                                },
                                onRemoveHighlight = { hlId -> vm.removeHighlight(hlId) },
                                onEditNote = { hlId, content -> vm.saveNote(hlId, content) },
                                onDismiss = { vm.showAnnotationsPanel = false },
                            )
                        }

                        SearchDialog(
                            visible = vm.showSearch,
                            query = vm.searchQuery,
                            results = vm.searchResults,
                            onQueryChange = { vm.searchQuery = it },
                            onSearch = { vm.performSearch(it) },
                            onResultClick = { chapterIdx ->
                                vm.goToChapter(chapterIdx)
                                vm.showSearch = false
                            },
                            onDismiss = { vm.showSearch = false },
                        )

                        ContributeDialog(
                            show = vm.showContributeDialog && !showMultiPlatformSyncDialog,
                            sampleCount = vm.contributeSampleCount,
                            samples = vm.contributeSamples,
                            githubUsername = vm.githubUsername,
                            inProgress = vm.contributeInProgress,
                            status = vm.contributeStatus,
                            prUrl = vm.contributePrUrl,
                            onPrepare = { vm.prepareContribution() },
                            onLogin = { showMultiPlatformSyncDialog = true },
                            onSubmit = { vm.submitContribution() },
                            onDismiss = { vm.dismissContributeDialog() },
                        )
                    }
                }
            }
        }

        if(bookMotion.active) Box(Modifier.fillMaxSize().zIndex(3f)) {
            BookPresentationOverlay(bookMotion,androidx.compose.ui.graphics.Color(vm.readingAppearance.style.variant(readerDark).paper))
        }
        // Generic imports retain their loading state; opening a shelf book uses the paper surface.
        // 加载遮罩：带淡出动画
        AnimatedVisibility(
            visible = vm.isLoading && !bookMotion.active,
            enter = fadeIn(animationSpec = tween(0)),
            exit = fadeOut(animationSpec = tween(400)),
        ) {
            Box(
                Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background),
                contentAlignment = Alignment.Center,
            ) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    CircularProgressIndicator(modifier=Modifier.size(24.dp),strokeWidth=2.dp)
                    vm.loadingMessage?.let { msg ->
                        Spacer(Modifier.height(16.dp))
                        Text(
                            text = I18n.message(msg),
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            style = MaterialTheme.typography.bodyMedium,
                        )
                    }
                }
            }
        }
    }
}
