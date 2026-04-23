package com.zhongbai233.epub.reader

import android.net.Uri
import android.os.Bundle
import androidx.activity.compose.BackHandler
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.*
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.foundation.background
import androidx.compose.animation.core.tween
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.ui.library.AboutScreen
import com.zhongbai233.epub.reader.ui.library.LibraryScreen
import com.zhongbai233.epub.reader.ui.library.SharingDialog
import com.zhongbai233.epub.reader.ui.library.TxtImportDialog
import com.zhongbai233.epub.reader.ui.reader.ReaderScreen
import com.zhongbai233.epub.reader.ui.reader.SearchDialog
import com.zhongbai233.epub.reader.ui.reader.ContributeDialog
import com.zhongbai233.epub.reader.ui.reader.AnnotationsSheet
import com.zhongbai233.epub.reader.ui.reader.ReviewPanel
import com.zhongbai233.epub.reader.ui.reader.TocDrawerContent
import com.zhongbai233.epub.reader.ui.theme.EpubReaderTheme
import com.zhongbai233.epub.reader.viewmodel.ReaderViewModel
import com.zhongbai233.epub.reader.i18n.I18n
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

        setContent {
            val vm: ReaderViewModel = viewModel()

            EpubReaderTheme(darkTheme = vm.isDarkMode) {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background
                ) {
                    MainContent(vm)
                }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
private fun MainContent(vm: ReaderViewModel) {
    // 读取 I18n.version 以确保语言切换时触发重组
    @Suppress("UNUSED_VARIABLE")
    val langVersion = I18n.version
    val scope = rememberCoroutineScope()
    val uriHandler = LocalUriHandler.current

    // SAF 文件选择器
    val filePicker = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.OpenDocument()
    ) { uri: Uri? ->
        uri?.let { vm.openFromUri(it) }
    }

    val backgroundImagePicker = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.OpenDocument()
    ) { uri: Uri? ->
        vm.updateReaderBgImage(uri?.toString())
    }

    // 目录抽屉状态
    val drawerState = rememberDrawerState(DrawerValue.Closed)

    // 共享对话框状态
    var showSharingDialog by remember { mutableStateOf(false) }

    // 关于界面状态
    var showAboutScreen by remember { mutableStateOf(false) }

    val currentBook = vm.currentBook

    BackHandler(
        enabled = vm.showTxtImport ||
            showSharingDialog ||
            vm.showContributeDialog ||
            vm.showAnnotationsPanel ||
            vm.showSearch ||
            drawerState.isOpen ||
            currentBook != null ||
            showAboutScreen
    ) {
        when {
            vm.showTxtImport -> vm.dismissTxtImport()
            showSharingDialog -> showSharingDialog = false
            vm.showContributeDialog -> vm.dismissContributeDialog()
            vm.showAnnotationsPanel -> vm.showAnnotationsPanel = false
            vm.showSearch -> vm.showSearch = false
            drawerState.isOpen -> scope.launch { drawerState.close() }
            currentBook != null -> vm.closeBook()
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
            text = { Text(msg) }
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
                        verticalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        TextButton(onClick = {
                            vm.dismissUpdateDialog()
                            runCatching { uriHandler.openUri(info.cdnDownloadUrl) }
                        }) {
                            Text(I18n.t("update.download_cdn"))
                        }
                        TextButton(onClick = {
                            vm.dismissUpdateDialog()
                            runCatching { uriHandler.openUri(info.githubDownloadUrl) }
                        }) {
                            Text(I18n.t("update.download_github"))
                        }
                        TextButton(onClick = { vm.dismissUpdateDialog() }) {
                            Text(I18n.t("feedback.not_now"))
                        }
                    }
                },
                title = { Text(I18n.t("update.check")) },
                text = { Text(I18n.tf1("update.new_version", info.tagName)) }
            )
        }
    }

    // 主内容 + 加载遮罩叠加
    Box(Modifier.fillMaxSize()) {
        // 主内容（加载完成后显示）
        if (!vm.isLoading) {
            val book = currentBook

            if (book == null) {
                if (showAboutScreen) {
                    // ---- 关于界面 ----
                    AboutScreen(
                        onNavigateBack = { showAboutScreen = false },
                        onExportLogs = { vm.exportFeedbackLogs() }
                    )
                } else {
                // ---- 书库界面 ----
                LibraryScreen(
                    books = vm.books,
                    coverCache = vm.coverCache,
                    language = vm.readerLanguage,
                    onOpenFilePicker = {
                        filePicker.launch(arrayOf("application/epub+zip", "application/epub", "*/*"))
                    },
                    onOpenBook = { uri, chapter ->
                        vm.openFromPath(uri, chapter)
                    },
                    onRemoveBook = { uri ->
                        vm.removeBookByUri(uri)
                    },
                    onUpdateLanguage = { code ->
                        vm.updateLanguage(code)
                    },
                    onOpenSharing = {
                        if (!vm.sharingServerRunning && vm.sharingPin.isEmpty()) {
                            vm.generatePin()
                        }
                        vm.loadPairedDevices()
                        showSharingDialog = true
                    },
                    onRefreshLibrary = {
                        vm.refreshLibrary()
                    },
                    onOpenAbout = { showAboutScreen = true }
                )

                // ── 共享对话框 ──
                SharingDialog(
                    showDialog = showSharingDialog,
                    onDismiss = { showSharingDialog = false },
                    serverRunning = vm.sharingServerRunning,
                    serverAddr = vm.sharingServerAddr,
                    pin = vm.sharingPin,
                    connectAddr = vm.connectAddrInput,
                    connectPin = vm.connectPinInput,
                    sharingStatus = vm.sharingStatus,
                    autoStartSharing = vm.autoStartSharing,
                    pairedDevices = vm.pairedDevices,
                    discoveredPeers = vm.discoveredPeers,
                    onPinChange = { vm.sharingPin = it },
                    onConnectAddrChange = { vm.connectAddrInput = it },
                    onConnectPinChange = { vm.connectPinInput = it },
                    onAutoStartChange = { vm.updateAutoStartSharing(it) },
                    onStartServer = { vm.startSharingServer() },
                    onStopServer = { vm.stopSharingServer() },
                    onManualSync = { vm.manualSync() },
                    onConnectToPeer = { addr, pin, deviceId -> vm.connectToPeer(addr, pin, deviceId) },
                    onStartDiscovery = { vm.startDiscovery() },
                    onRefreshPeers = { vm.refreshDiscoveredPeers() },
                    onRemovePaired = { vm.removePairedDevice(it) },
                    onOpenGithubFeedback = {
                        runCatching {
                            uriHandler.openUri("https://github.com/zhongbai233/RustEpubReader/issues/new/choose")
                        }
                    },
                )
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
            } else {
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
                                }
                            )
                        }
                    }
                ) {
                    val chapter = if (book.chapters.isNotEmpty()) {
                        book.chapters[vm.currentChapter.coerceIn(0, book.chapters.size - 1)]
                    } else null

                    ReaderScreen(
                        book = book,
                        currentChapter = vm.currentChapter,
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
                        systemFonts = vm.systemFonts,
                        showToc = drawerState.isOpen,
                        onNavigateBack = { vm.closeBook() },
                        onChapterChange = { vm.goToChapter(it) },
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
                        onOpenBackgroundPicker = {
                            backgroundImagePicker.launch(arrayOf("image/*"))
                        },
                        onClearBackgroundImage = { vm.updateReaderBgImage(null) },
                        onToggleToc = {
                            scope.launch { drawerState.open() }
                        },
                        onToggleSearch = { vm.showSearch = !vm.showSearch },
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
                        onLineSpacingChange = { vm.updateLineSpacing(it) },
                        onParaSpacingChange = { vm.updateParaSpacing(it) },
                        onTextIndentChange = { vm.updateTextIndent(it) },
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
                        onCscCorrectionStatusChange = { correction, status -> vm.updateCorrectionStatus(correction, status) },
                        // 段评
                        reviewChapterIndices = vm.reviewChapterIndices,
                        showReviewPanel = vm.showReviewPanel,
                        reviewPanelChapter = vm.reviewPanelChapter,
                        onOpenReviewPanel = { chapter, anchor -> vm.openReviewPanel(chapter, anchor) },
                        onCloseReviewPanel = { vm.closeReviewPanel() }
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
                                onDismiss = { vm.closeReviewPanel() }
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
                            onDismiss = { vm.showAnnotationsPanel = false }
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
                        onDismiss = { vm.showSearch = false }
                    )

                    ContributeDialog(
                        show = vm.showContributeDialog,
                        sampleCount = vm.contributeSampleCount,
                        samples = vm.contributeSamples,
                        githubUsername = vm.githubUsername,
                        githubUserCode = vm.githubUserCode,
                        githubVerificationUri = vm.githubVerificationUri,
                        githubAuthPolling = vm.githubAuthPolling,
                        inProgress = vm.contributeInProgress,
                        status = vm.contributeStatus,
                        prUrl = vm.contributePrUrl,
                        onPrepare = { vm.prepareContribution() },
                        onLogin = { vm.startGitHubLogin() },
                        onSubmit = { vm.submitContribution() },
                        onDismiss = { vm.dismissContributeDialog() }
                    )
                }
            }
        }

        // 加载遮罩：带淡出动画
        AnimatedVisibility(
            visible = vm.isLoading,
            enter = fadeIn(animationSpec = tween(0)),
            exit = fadeOut(animationSpec = tween(400))
        ) {
            Box(
                Modifier
                    .fillMaxSize()
                    .background(MaterialTheme.colorScheme.background),
                contentAlignment = Alignment.Center
            ) {
                CircularProgressIndicator()
            }
        }
    }
}
