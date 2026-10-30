package com.zhongbai233.epub.reader.viewmodel

import com.zhongbai233.epub.reader.ui.reader.readerSourceText
import com.zhongbai233.epub.reader.ui.reader.unicodeScalarOffset
import com.zhongbai233.epub.reader.ui.reader.utf16TextOffset
import android.app.Application
import android.net.Uri
import android.net.wifi.WifiManager
import android.content.Context
import android.widget.Toast
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.zhongbai233.epub.reader.model.*
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.util.FontDiscovery
import com.zhongbai233.epub.reader.util.FontItem
import com.zhongbai233.epub.reader.RustBridge
import com.zhongbai233.epub.reader.BuildConfig
import com.zhongbai233.epub.reader.util.UpdateChecker
import com.zhongbai233.epub.reader.tts.TtsManager
import com.zhongbai233.epub.reader.csc.CscEngine
import com.zhongbai233.epub.reader.csc.CorrectionInfo
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import java.io.File
import java.io.FileOutputStream
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import android.util.Base64
import org.json.JSONObject
import com.zhongbai233.epub.reader.util.GitHubTokenStore
import com.zhongbai233.epub.reader.util.resolveResumeChapter
import com.zhongbai233.epub.reader.util.canApplyGithubSync
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.sync.withLock

data class TxtChapterPreview(val title: String, val lineStart: Int, val charCount: Int, val isHeading: Boolean = true, val excerpt: String = "")

class ReaderViewModel(application: Application) : AndroidViewModel(application) {
    // Resume may follow a cached library load without suspending. Its session
    // must exist before any startup coroutine can call openFromPath().
    private val openingSession = BookOpeningSession { RustBridge.cancelBookOpen(it) }

    val activityTracker by lazy { com.zhongbai233.epub.reader.util.ReadingActivityTracker(dataDir) }
    var showStatistics by mutableStateOf(false)
    var statisticsCurrentBook = false
    fun openStatistics(current: Boolean) { activityTracker.flush(); statisticsCurrentBook=current; showStatistics=true }
    suspend fun loadStatistics(current: Boolean): com.zhongbai233.epub.reader.ui.reader.ActivityReport = withContext(Dispatchers.IO) {
        val request=JSONObject().put("op","activity_report").put("today",java.text.SimpleDateFormat("yyyy-MM-dd",java.util.Locale.ROOT).format(java.util.Date()))
        if(current) currentBookId?.let {request.put("id",it)}
        val envelope=JSONObject(RustBridge.readingActivity(dataDir,request.toString()) ?: error(I18n.t("common.error_activity_load")))
        if(!envelope.optBoolean("ok")) error(envelope.optString("error"))
        Json {ignoreUnknownKeys=true}.decodeFromString<com.zhongbai233.epub.reader.ui.reader.ActivityReport>(envelope.getJSONObject("result").toString())
    }
    init { viewModelScope.launch { delay(1000); while(isActive) {
        activityTracker.sample(currentBookId,ttsManager.audible)
        delay(1000)
    } } }

    private val context get() = getApplication<Application>()
    private val library = Library(context, loadImmediately = false)
    private var libraryStartup: Job? = null
    private var libraryRefresh: Job? = null
    private var currentBookUri: String? = null
    val originalBookPath: String get() = currentBookUri.orEmpty()
    val currentBookId: String?
        get() = currentBookUri?.let { uri -> library.books.firstOrNull { it.uri == uri }?.id }
    private val prefs by lazy {
        context.getSharedPreferences("reader_settings", Context.MODE_PRIVATE)
    }
    private var multicastLock: WifiManager.MulticastLock? = null

    // ---- ���״̬ ----
    val books = mutableStateListOf<BookEntry>()
    var classification by mutableStateOf(com.zhongbai233.epub.reader.model.BookClassification()); private set
    var categoriesBusy by mutableStateOf(false); private set
    var categoriesError by mutableStateOf<String?>(null); private set
    var archivedBooks by mutableStateOf<List<JSONObject>>(emptyList())
        private set
    var githubSyncMeter by mutableStateOf<JSONObject?>(null)
        private set
    private var githubMeterJob: Job? = null
    private val shelfCovers by lazy {
        ShelfCoverLoader(viewModelScope, {uri->books.any {it.uri==uri}},
            {uri->libraryRenderObserver?.coverRequested(uri)},
            {uri->RustBridge.getShelfCover(dataDir, uri)?.let {Base64.decode(it, Base64.DEFAULT)}})
    }
    val coverCache get() = shelfCovers.cache
    internal var libraryRenderObserver: com.zhongbai233.epub.reader.ui.library.LibraryRenderObserver? = null
    internal var readerSelectionObserver: com.zhongbai233.epub.reader.ui.reader.ReaderSelectionObserver? = null

    // ---- �Ķ���״̬ ----
    var currentBook by mutableStateOf<EpubBook?>(null)
        private set
    var currentChapter by mutableIntStateOf(0)
        private set
    var currentPage by mutableIntStateOf(0)
        private set
    var currentBlock by mutableIntStateOf(0)
        private set
    var currentCharOffset by mutableIntStateOf(0)
        private set
    var positionRevision by mutableIntStateOf(0)
        private set
    var returnHistory by mutableStateOf<List<ReaderReturnPoint>>(emptyList())
        private set
    var navigationPulse by mutableIntStateOf(0)
        private set
    var currentPageLabel by mutableStateOf("")
        private set
    private var navigationGeneration = 0
    private var remoteNavigation: Int? = null
    fun updatePageLabel(label: String) { currentPageLabel = label }
    private fun historyKey(uri: String) = "reading_return_history." + (library.books.firstOrNull { it.uri == uri }?.id ?: uri)
    private fun persistNavigation() {
        val uri = currentBookUri ?: return
        prefs.edit().putString(historyKey(uri), jsonParser.encodeToString(kotlinx.serialization.builtins.ListSerializer(ReaderReturnPoint.serializer()), returnHistory)).apply()
    }
    fun persistReadingSession() { if (currentBookUri != null) { if (remoteNavigation == null) saveProgress(); persistNavigation() } }
    fun goToPosition(chapter: Int, block: Int = 0, charOffset: Int = 0, remember: Boolean = true, remote: Boolean = false, returning: Boolean = false, scalarOffset: Boolean = false) {
        val book = currentBook ?: return
        if (chapter !in book.chapters.indices) return
        val generation = ++navigationGeneration
        remoteNavigation = if (remote) generation else null
        val revisionAtRequest = localProgressRevision
        val returnTarget = if (returning) returnHistory.lastOrNull()?.location else null
        viewModelScope.launch {
            try {
                val source = withContext(Dispatchers.IO) { book.chapters[chapter] }
                if (generation != navigationGeneration || currentBook !== book || (remote && localProgressRevision != revisionAtRequest)) return@launch
                val at = block.coerceIn(0, source.blocks.lastIndex.coerceAtLeast(0))
                val content = source.blocks.getOrNull(at)
                val text = when (content) { is ContentBlock.Paragraph -> content.spans.joinToString("") { it.text }; is ContentBlock.Heading -> content.spans.joinToString("") { it.text }; else -> "" }
                val offset = if (scalarOffset) utf16TextOffset(text, charOffset) else charOffset.coerceIn(0, text.length)
                val target = ReaderLocation(chapter, at, offset)
                val origin = ReaderReturnPoint(ReaderLocation(currentChapter, currentBlock, currentCharOffset),
                    book.chapterTitles.getOrNull(currentChapter) ?: "", currentPageLabel)
                if (target == origin.location) return@launch
                if (returning && returnHistory.lastOrNull()?.location != returnTarget) return@launch
                if (returning) returnHistory = returnHistory.dropLast(1)
                else if (remember) returnHistory = recordReaderJump(returnHistory, origin, target)
                currentChapter = chapter; currentBlock = at; currentCharOffset = offset; currentPage = 0
                if (remember || returning || remote) { positionRevision++; navigationPulse++ }
                showReviewPanel = false; reviewPanelChapter = null
                if (!remote) saveProgress()
                else prefs.edit().putInt(PrefKeys.LAST_BOOK_CHAPTER, chapter).apply()
                persistNavigation(); updateBookmarkState()
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (error: Exception) { addLog("Navigation failed: ${error.message}"); errorMessage = error.message ?: I18n.t("common.error_unknown") }
            finally { if (remoteNavigation == generation) remoteNavigation = null }
        }
    }

    // ---- ���� ----
    var fontSize by mutableFloatStateOf(18f)
        private set
    var appearanceLinkage by mutableStateOf(com.zhongbai233.epub.reader.model.AppearanceLinkage.firstRun())
        private set
    var systemAppearanceDark by mutableStateOf((context.resources.configuration.uiMode and android.content.res.Configuration.UI_MODE_NIGHT_MASK) == android.content.res.Configuration.UI_MODE_NIGHT_YES)
        private set
    var ambientAppearanceDark by mutableStateOf<Boolean?>(null)
        private set
    var ambientSensorAvailable by mutableStateOf(false)
        private set
    val isDarkMode: Boolean get() = appearanceLinkage.appDark(systemAppearanceDark, ambientAppearanceDark)
    val readingDark: Boolean get() = appearanceLinkage.readingDark(systemAppearanceDark, ambientAppearanceDark)
    var isScrollMode by mutableStateOf(false)
    var readerBgColorIndex by mutableIntStateOf(0)
        private set
    var readerCustomBgColorArgb by mutableIntStateOf(0xFFF5F0E8.toInt())
        private set
    var readerFontColorIndex by mutableIntStateOf(0)
        private set
    var readerCustomFontColorArgb by mutableIntStateOf(0xFF1A1A1A.toInt())
        private set
    var readerLatinFontFamily by mutableStateOf("")
        private set
    fun updateLatinFontFamily(value:String){readerLatinFontFamily=value;prefs.edit().putString("reader.latin_font_family",value).apply()}
    var readerFontFamily by mutableStateOf("Sans")
        private set
    var readerPageAnimation by mutableStateOf("Realistic")
        private set
    var readerBgImageUri by mutableStateOf<String?>(null)
        private set
    var readerBgImageAlpha by mutableFloatStateOf(0.22f)
        private set
    var readingAppearance by mutableStateOf(com.zhongbai233.epub.reader.model.ReadingAppearance())
        private set
    var readerLanguage by mutableStateOf("auto")
        private set
    var readerIntroductionSeen by mutableStateOf(false)
        private set
    fun markReaderIntroductionShown() {
        readerIntroductionSeen=true
        prefs.edit().putBoolean("reader.introduction.v1",true).apply()
    }
    var showImmersiveStatus by mutableStateOf(true)
        private set
    var systemFonts by mutableStateOf<List<FontItem>>(emptyList())
        private set

    // ---- 排版设置 ----
    var lineSpacing by mutableFloatStateOf(1.5f)
        private set
    var forceSingleColumn by mutableStateOf(false)
        private set
    var paraSpacing by mutableFloatStateOf(0.5f)
        private set
    var textIndent by mutableIntStateOf(2)
        private set
    var titleFontScale by mutableFloatStateOf(1.5f)
        private set

    var translateApiModel by mutableStateOf("gpt-4o-mini")
        private set
    var dictionaryApiModel by mutableStateOf("gpt-4o-mini")
        private set
    var translateApiProvider by mutableStateOf("openai")
        private set
    var dictionaryApiProvider by mutableStateOf("openai")
        private set
    fun updateTranslateApiModel(value:String){translateApiModel=value;persistSettings()}
    fun updateDictionaryApiModel(value:String){dictionaryApiModel=value;persistSettings()}
    fun updateTranslateApiProvider(value:String){translateApiProvider=value;persistSettings()}
    fun updateDictionaryApiProvider(value:String){dictionaryApiProvider=value;persistSettings()}
    // ---- API 设置 ----
    var translateApiUrl by mutableStateOf("")
        private set
    var translateApiKey by mutableStateOf("")
        private set
    var dictionaryApiUrl by mutableStateOf("")
        private set
    var dictionaryApiKey by mutableStateOf("")
        private set

    // ---- TTS 设置 ----
    var ttsVoiceName by mutableStateOf("zh-CN-XiaoxiaoNeural")
        private set
    var ttsRate by mutableIntStateOf(0)
        private set
    var ttsVolume by mutableIntStateOf(0)
        private set

    // ---- TTS 播放状态 ----
    val ttsManager = TtsManager(application)
    var showTtsBar by mutableStateOf(false)
        private set

    // ---- CSC 设置 ----
    var cscMode by mutableStateOf("none")
        private set
    var cscThreshold by mutableStateOf("standard")
        private set

    // ---- CSC 引擎 ----
    val cscEngine = CscEngine()
    var cscModelReady by mutableStateOf(false)
        private set
    var cscModelLoading by mutableStateOf(false)
        private set
    var cscCorrections by mutableStateOf<List<CorrectionInfo>>(emptyList())
        private set

    // ---- 段评状态 ----
    var reviewChapterIndices by mutableStateOf<Set<Int>>(emptySet())
        private set
    var chapterReviews by mutableStateOf<Map<Int, Int>>(emptyMap())
        private set
    var showReviewPanel by mutableStateOf(false)
    var reviewPanelChapter by mutableStateOf<Int?>(null)
    var reviewPanelAnchor by mutableStateOf<String?>(null)
    var reviewPanelShowAll by mutableStateOf(false)

    fun updateCorrectionStatus(correction: com.zhongbai233.epub.reader.ui.reader.CscBlockCorrection, newStatus: com.zhongbai233.epub.reader.csc.CorrectionStatus) {
        cscCorrections = cscCorrections.map { c ->
            if (c.charOffset == correction.globalCharOffset) c.copy(status = newStatus) else c
        }
        // Persist to BookConfig
        val bookId = currentBookId ?: return
        viewModelScope.launch(Dispatchers.IO) {
            try {
                val payload = org.json.JSONObject().apply {
                    put("chapter", currentChapter)
                    put("block_idx", correction.blockIndex)
                    put("char_offset", correction.globalCharOffset)
                    put("original", correction.original)
                    put("corrected", correction.corrected)
                    put("status", newStatus.name.lowercase())
                }.toString()
                RustBridge.upsertCorrection(dataDir, bookId, payload)
                reloadBookConfig(dataDir, bookId)
            } catch (e: Exception) {
                android.util.Log.e("ReaderViewModel", "upsertCorrection failed", e)
            }
        }
    }

    // ---- ����״̬ ----
    var isLoading by mutableStateOf(false)
        private set
    var backupBusy by mutableStateOf(false)
        private set
    val backupAvailable: Boolean get() = !backupBusy && !isLoading && currentBook == null && !githubSyncRunning && !autoSyncInProgress
    suspend fun <T> backupFileOperation(block: suspend ()->T): T {
        check(backupAvailable) { I18n.t("backup.wait") }
        backupBusy=true
        try { return withContext(kotlinx.coroutines.NonCancellable) {block()} }
        finally {backupBusy=false}
    }
    suspend fun backupOperation(action: String, path: String, expected: String?=null): JSONObject = backupFileOperation {
            libraryStartup?.join()
            val result=withContext(Dispatchers.IO) {
                val request=JSONObject().put("action",action).put("path",path)
                if(expected!=null) request.put("expected_manifest",expected)
                val envelope=JSONObject(RustBridge.backupTools(dataDir,request.toString()) ?: error("Missing backup result"))
                check(envelope.optBoolean("ok")) {envelope.optString("error")}
                if(action=="restore") library.reload()
                envelope.getJSONObject("result")
            }
            if(action=="restore") { loadLibrary(); localProgressRevision++ }
            result
    }
    suspend fun backupProgress(): JSONObject? = withContext(Dispatchers.IO) {
        runCatching { JSONObject(RustBridge.backupTools(dataDir,"{\"action\":\"status\"}") ?: return@runCatching null).optJSONObject("result") }.getOrNull()
    }
    var errorMessage by mutableStateOf<String?>(null)
        private set
    var loadingMessage by mutableStateOf<String?>(null)
        private set

    // ---- ����״̬ ----
    var sharingServerRunning by mutableStateOf(false)
        private set
    var sharingServerAddr by mutableStateOf("")
        private set
    var sharingPin by mutableStateOf("")
    var connectAddrInput by mutableStateOf("")
    var connectPinInput by mutableStateOf("")
    var sharingStatus by mutableStateOf("")
        private set
    var autoStartSharing by mutableStateOf(false)
        private set
    val pairedDevices = mutableStateListOf<PairedDevice>()
    val discoveredPeers = mutableStateListOf<DiscoveredPeer>()
    val sharingLogs = mutableStateListOf<String>()

    // ---- 更新状态 ----
    var updateInfo by mutableStateOf<UpdateChecker.UpdateInfo?>(null)
        private set
    var showUpdateDialog by mutableStateOf(false)
        private set

    // ---- TXT 导入状态 ----
    var showTxtImport by mutableStateOf(false)
        private set
    var txtImportPath by mutableStateOf<String?>(null)
        private set

    // ---- 搜索状态 ----
    var showSearch by mutableStateOf(false)
    var searchQuery by mutableStateOf("")
    var searchResults by mutableStateOf<List<SearchResult>>(emptyList())
        private set
    var txtImportTitle by mutableStateOf("")
    var txtImportAuthor by mutableStateOf("")
    var txtImportRegex by mutableStateOf("")
    var txtImportHeuristic by mutableStateOf(false)
    var txtImportPreviews by mutableStateOf<List<TxtChapterPreview>>(emptyList())
        private set
    var txtImportConverting by mutableStateOf(false)
        private set
    var txtImportError by mutableStateOf<String?>(null)
        private set

    // ---- 书签 / 高亮 / 标注状态 ----
    var isChapterBookmarked by mutableStateOf(false)
        private set
    var bookConfig by mutableStateOf<FullBookConfig?>(null)
        private set
    var showAnnotationsPanel by mutableStateOf(false)

    // ---- CSC 贡献状态 ----
    var showContributeDialog by mutableStateOf(false)
    var contributeStatus by mutableStateOf("")
        private set
    var contributeInProgress by mutableStateOf(false)
        private set
    var contributePrUrl by mutableStateOf<String?>(null)
        private set
    var contributeSamples by mutableStateOf("")
        private set
    var contributeSampleCount by mutableStateOf(0)
        private set
    // GitHub OAuth
    val bookSharing by lazy { com.zhongbai233.epub.reader.ui.library.BookShareModel(context.filesDir.absolutePath, beforeImport = { cancelGithubSyncWork() }) { refreshLibrary() } }
    var receivingSharedBooks by mutableStateOf(false)
    var githubToken by mutableStateOf<String?>(null)
        private set
    var githubUsername by mutableStateOf<String?>(null)
        private set
    var githubDeviceCode by mutableStateOf<String?>(null)
        private set
    var githubUserCode by mutableStateOf<String?>(null)
        private set
    var githubVerificationUri by mutableStateOf<String?>(null)
        private set
    var githubAuthPolling by mutableStateOf(false)
        private set
    var githubRepoAuthorized by mutableStateOf(false)
        private set
    var githubSyncLastSuccess by mutableStateOf<Long?>(null)
        private set
    var githubAuthForSync by mutableStateOf(false)
        private set
    var githubSyncContributions by mutableStateOf(false)
        private set
    var githubSyncEnabled by mutableStateOf(false)
        private set
    var githubSyncRunning by mutableStateOf(false)
        private set
    var githubSyncNeedsAuth by mutableStateOf(false)
        private set
    var githubSyncStatus by mutableStateOf("")
        private set
    var githubSyncFailures by mutableStateOf<List<JSONObject>>(emptyList())
        private set
    var githubSyncThrottled by mutableStateOf(false)
        private set
    var githubSyncOffer by mutableStateOf<JSONObject?>(null)
        private set
    private var githubSyncForeground = false
    private var githubSyncAttemptAt = 0L
    private var githubSyncRetryMs = 300_000L
    private var githubSyncGeneration = 0L
    private var githubSyncJob: Job? = null
    private var localProgressRevision=0L
    private var uploadedProgressRevision=0L
    private var githubLoginJob: Job? = null

    private var contributePrompted = false
    private var contributeDismissed = false

    @Volatile
    private var autoSyncInProgress: Boolean = false
    private val autoSyncLastAttemptAt = ConcurrentHashMap<String, Long>()
    private val autoSyncCooldownMs = 12_000L
    private var discoveryStarted: Boolean = false
    private var discoveryPollingJob: Job? = null

    private fun addLog(msg: String) {
        val ts = java.text.SimpleDateFormat("HH:mm:ss", java.util.Locale.getDefault()).format(java.util.Date())
        val entry = "[$ts] $msg"
        android.util.Log.d("SHARE-DBG", entry)
        viewModelScope.launch(Dispatchers.Main) {
            sharingLogs.add(entry)
            if (sharingLogs.size > 200) sharingLogs.removeAt(0)
        }
    }

    private val jsonParser = Json { 
        ignoreUnknownKeys = true 
        classDiscriminator = "type" 
    }

    init {
        loadSettings()
        viewModelScope.launch {
            while (isActive) {
                delay(5_000)
                if (githubSyncEnabled && githubSyncForeground && !isLoading && !githubSyncNeedsAuth && !githubSyncRunning && (githubSyncAttemptAt == 0L || android.os.SystemClock.elapsedRealtime() - githubSyncAttemptAt >= githubSyncRetryMs || (!githubSyncThrottled && localProgressRevision != uploadedProgressRevision && android.os.SystemClock.elapsedRealtime() - githubSyncAttemptAt >= 5_000))) {
                    syncGithubNow()
                }
            }
        }
        if (autoStartSharing) {
            try { startSharingServer() } catch (_: Exception) { }
        }
        // Dispatch rather than running Main.immediate inside this constructor:
        // cached IO can otherwise return inline before later fields are ready.
        libraryStartup = viewModelScope.launch(Dispatchers.Main) {
            try {
                withContext(Dispatchers.IO) { library.reload(); scanBooksDir() }
                loadLibrary()
                val lastUri = prefs.getString(PrefKeys.LAST_BOOK_URI, null)
                if (lastUri != null) openFromPath(lastUri, prefs.getInt(PrefKeys.LAST_BOOK_CHAPTER, 0))
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) {
                addLog("Library load failed: ${failure.message}")
                errorMessage = I18n.t("library.error_read_failed")
            }
        }
        try { loadPairedDevices() } catch (_: Exception) { }
        try { startDiscovery() } catch (_: Exception) { }
        viewModelScope.launch(Dispatchers.IO) {
            val fonts = FontDiscovery.discoverFonts()
            withContext(Dispatchers.Main) {
                systemFonts = fonts
            }
        }
        // �Զ��ָ��ϴ��Ķ�

        // 启动时检查更新
        viewModelScope.launch {
            val info = UpdateChecker.checkForUpdate(BuildConfig.APP_VERSION_NAME)
            if (info != null) {
                updateInfo = info
                showUpdateDialog = true
            }
        }

        // Auto-load CSC model if enabled
        if (cscMode != "none") {
            loadCscModel()
        }
    }

    private object PrefKeys {
        const val FONT_SIZE = "font_size"
        const val DARK_MODE = "dark_mode"
        const val SCROLL_MODE = "scroll_mode"
        const val BG_COLOR_INDEX = "reader_bg_color_index"
        const val CUSTOM_BG_COLOR = "reader_custom_bg_color_argb"
        const val FONT_COLOR_INDEX = "reader_font_color_index"
        const val CUSTOM_FONT_COLOR = "reader_custom_font_color_argb"
        const val FONT_FAMILY = "reader_font_family"
        const val PAGE_ANIMATION = "reader_page_animation"
        const val BG_IMAGE_URI = "reader_bg_image_uri"
        const val BG_IMAGE_ALPHA = "reader_bg_image_alpha"
        const val LANGUAGE = "reader_language"
        const val SHOW_IMMERSIVE_STATUS = "show_immersive_status"
        const val LAST_BOOK_URI = "last_book_uri"
        const val LAST_BOOK_CHAPTER = "last_book_chapter"
        const val AUTO_START_SHARING = "auto_start_sharing"
        const val GITHUB_TOKEN = "github_token"
        const val GITHUB_USERNAME = "github_username"
        const val LINE_SPACING = "line_spacing"
        const val FORCE_SINGLE_COLUMN = "force_single_column"
        const val PARA_SPACING = "para_spacing"
        const val TEXT_INDENT = "text_indent"
        const val TITLE_FONT_SCALE = "title_font_scale"
        const val TRANSLATE_API_URL = "translate_api_url"
        const val TRANSLATE_API_KEY = "translate_api_key"
        const val DICTIONARY_API_URL = "dictionary_api_url"
        const val DICTIONARY_API_KEY = "dictionary_api_key"
        const val TTS_VOICE_NAME = "tts_voice_name"
        const val TTS_RATE = "tts_rate"
        const val TTS_VOLUME = "tts_volume"
        const val CSC_MODE = "csc_mode"
        const val CSC_THRESHOLD = "csc_threshold"
    }

    private fun loadSettings() {
        val guideKey="reader.introduction.v1"
        if(!prefs.contains(guideKey)) prefs.edit().putBoolean(guideKey,prefs.getString(PrefKeys.LAST_BOOK_URI,null)!=null).apply()
        readerIntroductionSeen=prefs.getBoolean(guideKey,false)
        val hadAppearance = prefs.contains(PrefKeys.DARK_MODE) || prefs.contains(com.zhongbai233.epub.reader.model.ReadingAppearance.storageKey) || prefs.contains(PrefKeys.BG_COLOR_INDEX)
        val legacyDark = prefs.getBoolean(PrefKeys.DARK_MODE, false)
        fontSize = prefs.getFloat(PrefKeys.FONT_SIZE, 18f).coerceIn(12f, 40f)
        isScrollMode = prefs.getBoolean(PrefKeys.SCROLL_MODE, false)

        readerBgColorIndex = prefs.getInt(PrefKeys.BG_COLOR_INDEX, 0).coerceAtLeast(0)
        readerCustomBgColorArgb = prefs.getInt(PrefKeys.CUSTOM_BG_COLOR, 0xFFF5F0E8.toInt())

        readerFontColorIndex = prefs.getInt(PrefKeys.FONT_COLOR_INDEX, 0).coerceAtLeast(0)
        readerCustomFontColorArgb = prefs.getInt(PrefKeys.CUSTOM_FONT_COLOR, 0xFF1A1A1A.toInt())

        readerLatinFontFamily=prefs.getString("reader.latin_font_family","")?:""
        readerFontFamily = prefs.getString(PrefKeys.FONT_FAMILY, "Sans") ?: "Sans"
        readerPageAnimation = prefs.getString(PrefKeys.PAGE_ANIMATION, "Realistic") ?: "Realistic"
        readerBgImageUri = prefs.getString(PrefKeys.BG_IMAGE_URI, null)
        readerBgImageAlpha = prefs.getFloat(PrefKeys.BG_IMAGE_ALPHA, 0.22f).coerceIn(0f, 1f)
        readingAppearance = com.zhongbai233.epub.reader.model.ReadingAppearance.decode(
            prefs.getString(com.zhongbai233.epub.reader.model.ReadingAppearance.storageKey, null)
        ) ?: com.zhongbai233.epub.reader.model.ReadingAppearance.migrate(
            readerBgColorIndex, readerCustomBgColorArgb, readerFontColorIndex, readerCustomFontColorArgb,
            readerBgImageUri, readerBgImageAlpha
        ).also { prefs.edit().putString(com.zhongbai233.epub.reader.model.ReadingAppearance.storageKey, it.encoded()).apply() }
        appearanceLinkage = com.zhongbai233.epub.reader.model.AppearanceLinkage.decode(prefs.getString(com.zhongbai233.epub.reader.model.AppearanceLinkage.storageKey, null))
            ?: (if(hadAppearance) com.zhongbai233.epub.reader.model.AppearanceLinkage.migrate(legacyDark, readingAppearance.mode) else com.zhongbai233.epub.reader.model.AppearanceLinkage.firstRun())
                .also { prefs.edit().putString(com.zhongbai233.epub.reader.model.AppearanceLinkage.storageKey,it.encoded()).apply() }
        readerLanguage = I18n.normalize(prefs.getString(PrefKeys.LANGUAGE, "auto") ?: "auto")
        showImmersiveStatus = prefs.getBoolean(PrefKeys.SHOW_IMMERSIVE_STATUS, true)
        I18n.setLanguage(readerLanguage)
        autoStartSharing = prefs.getBoolean(PrefKeys.AUTO_START_SHARING, false)
        githubToken = GitHubTokenStore.load(context)
        githubRepoAuthorized = githubToken != null && prefs.getBoolean("github_repo_authorized", false)
        githubSyncEnabled = prefs.getBoolean("github_sync_enabled", false)
        githubSyncContributions = prefs.getBoolean("github_sync_contributions", false)
        githubSyncNeedsAuth = githubSyncEnabled && githubToken == null
        githubUsername = if (githubToken != null) prefs.getString(PrefKeys.GITHUB_USERNAME, null) else null
        lineSpacing = prefs.getFloat(PrefKeys.LINE_SPACING, 1.5f).coerceIn(1.0f, 3.0f)
        forceSingleColumn = prefs.getBoolean(PrefKeys.FORCE_SINGLE_COLUMN, false)
        paraSpacing = prefs.getFloat(PrefKeys.PARA_SPACING, 0.5f).coerceIn(0.0f, 2.0f)
        textIndent = prefs.getInt(PrefKeys.TEXT_INDENT, 2).coerceIn(0, 4)
        titleFontScale = prefs.getFloat(PrefKeys.TITLE_FONT_SCALE, 1.5f).coerceIn(1.0f, 2.5f)
        translateApiUrl = prefs.getString(PrefKeys.TRANSLATE_API_URL, "") ?: ""
        translateApiModel=prefs.getString("translate_api_model","gpt-4o-mini")?:"gpt-4o-mini"
        dictionaryApiModel=prefs.getString("dictionary_api_model","gpt-4o-mini")?:"gpt-4o-mini"
        translateApiProvider=prefs.getString("translate_api_provider",if(translateApiUrl.isEmpty()||translateApiUrl.contains("/chat/completions"))"openai" else "json")?:"openai"
        dictionaryApiProvider=prefs.getString("dictionary_api_provider",if((prefs.getString(PrefKeys.DICTIONARY_API_URL,"")?:"").let{it.isEmpty()||it.contains("/chat/completions")})"openai" else "json")?:"openai"
        translateApiKey = loadServiceKey("translate",PrefKeys.TRANSLATE_API_KEY)
        dictionaryApiUrl = prefs.getString(PrefKeys.DICTIONARY_API_URL, "") ?: ""
        dictionaryApiKey = loadServiceKey("dictionary",PrefKeys.DICTIONARY_API_KEY)
        ttsVoiceName = prefs.getString(PrefKeys.TTS_VOICE_NAME, "zh-CN-XiaoxiaoNeural") ?: "zh-CN-XiaoxiaoNeural"
        ttsRate = prefs.getInt(PrefKeys.TTS_RATE, 0)
        ttsVolume = prefs.getInt(PrefKeys.TTS_VOLUME, 0)
        cscMode = prefs.getString(PrefKeys.CSC_MODE, "none") ?: "none"
        cscThreshold = prefs.getString(PrefKeys.CSC_THRESHOLD, "standard") ?: "standard"
    }

    private fun persistSettings() {
        prefs.edit()
            .putFloat(PrefKeys.FONT_SIZE, fontSize)
            .putBoolean(PrefKeys.DARK_MODE, isDarkMode)
            .putBoolean(PrefKeys.SCROLL_MODE, isScrollMode)
            .putInt(PrefKeys.BG_COLOR_INDEX, readerBgColorIndex)
            .putInt(PrefKeys.CUSTOM_BG_COLOR, readerCustomBgColorArgb)
            .putInt(PrefKeys.FONT_COLOR_INDEX, readerFontColorIndex)
            .putInt(PrefKeys.CUSTOM_FONT_COLOR, readerCustomFontColorArgb)
            .putString(PrefKeys.FONT_FAMILY, readerFontFamily)
            .putString(PrefKeys.PAGE_ANIMATION, readerPageAnimation)
            .putString(PrefKeys.BG_IMAGE_URI, readerBgImageUri)
            .putFloat(PrefKeys.BG_IMAGE_ALPHA, readerBgImageAlpha)
            .putString(PrefKeys.LANGUAGE, readerLanguage)
            .putBoolean(PrefKeys.SHOW_IMMERSIVE_STATUS, showImmersiveStatus)
            .putBoolean(PrefKeys.AUTO_START_SHARING, autoStartSharing)
            .putFloat(PrefKeys.LINE_SPACING, lineSpacing)
            .putBoolean(PrefKeys.FORCE_SINGLE_COLUMN, forceSingleColumn)
            .putFloat(PrefKeys.PARA_SPACING, paraSpacing)
            .putInt(PrefKeys.TEXT_INDENT, textIndent)
            .putFloat(PrefKeys.TITLE_FONT_SCALE, titleFontScale)
            .putString("translate_api_model",translateApiModel).putString("dictionary_api_model",dictionaryApiModel)
            .putString("translate_api_provider",translateApiProvider).putString("dictionary_api_provider",dictionaryApiProvider)
            .putString(PrefKeys.TRANSLATE_API_URL, translateApiUrl)
            .putString(PrefKeys.DICTIONARY_API_URL, dictionaryApiUrl)
            .putString(PrefKeys.TTS_VOICE_NAME, ttsVoiceName)
            .putInt(PrefKeys.TTS_RATE, ttsRate)
            .putInt(PrefKeys.TTS_VOLUME, ttsVolume)
            .putString(PrefKeys.CSC_MODE, cscMode)
            .putString(PrefKeys.CSC_THRESHOLD, cscThreshold)
            .apply()
    }

    // ---- ������ ----

    private fun loadLibrary() {
        classification = library.classification
        reloadArchive()

        val sorted = library.sortedIndicesByRecent().map { library.books[it] }
        androidx.compose.runtime.snapshots.Snapshot.withMutableSnapshot {
            books.clear(); books.addAll(sorted)
        }
    }

    /** Public refresh: scan books dir for new files + apply progress + reload library UI */
    fun refreshLibrary(): Job {
        libraryRefresh?.takeIf { it.isActive }?.let { return it }
        return viewModelScope.launch {
            try {
                libraryStartup?.join()
                withContext(Dispatchers.IO) { library.reload(); scanBooksDir(); applySyncedProgress() }
                loadLibrary()
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) { errorMessage = failure.message ?: I18n.t("common.error_unknown") }
        }.also { libraryRefresh = it }
    }

    /** Rust validates and commits selected records before notifying the native viewport. */
    private suspend fun applySyncedProgress() {
        val revision = withContext(Dispatchers.Main) { localProgressRevision }
        val progressJson = RustBridge.getSyncedProgress(dataDir) ?: return
        val arr = org.json.JSONArray(progressJson)
        if (arr.length() == 0) return
        library.reload()
        for (i in 0 until arr.length()) {
            val remote = arr.getJSONObject(i)
            val path = remote.getString("path")
            val chapter = remote.getInt("chapter")
            val block = remote.optInt("block", 0)
            val offset = remote.optInt("char_offset", 0)
            withContext(Dispatchers.Main) {
                if (currentBookUri == path && localProgressRevision == revision) {
                    val source = currentBook?.chapters?.getOrNull(currentChapter)?.blocks?.getOrNull(currentBlock)
                    val scalar = unicodeScalarOffset(readerSourceText(source), currentCharOffset)
                    if (currentChapter != chapter || currentBlock != block || scalar != offset) goToPosition(chapter, block, offset, remote = true, scalarOffset = true)
                }
            }
        }
        addLog("applySyncedProgress: committed ${arr.length()} position(s)")
    }

    fun requestCover(uri: String) { shelfCovers.request(uri) }

    fun reportExportFailure(detail: String = "") { errorMessage = I18n.tf1("book.export_failed", detail) }

    fun renameBook(uri: String, title: String) {
        try { library.rename(uri, title); loadLibrary() }
        catch (error: Exception) { errorMessage = error.message ?: I18n.t("common.error_unknown") }
    }

    fun categoriesAction(request: JSONObject, done: () -> Unit = {}) {
        if (categoriesBusy) return
        categoriesBusy = true; categoriesError = null
        viewModelScope.launch {
            try {
                withContext(Dispatchers.IO) {
                    val response = JSONObject(RustBridge.bookCategories(dataDir, request.toString()) ?: error("Missing category result"))
                    check(response.getBoolean("ok")) { response.optString("error") }
                    library.reload()
                }
                loadLibrary(); done()
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) {
                val message = failure.message.orEmpty()
                categoriesError = if (message.startsWith("category.error_")) I18n.t(message) else message
            } finally { categoriesBusy = false }
        }
    }

    private fun reloadArchive() {
        runCatching {
            val response = JSONObject(RustBridge.bookArchive(dataDir, "{}") ?: error("Missing archive"))
            check(response.getBoolean("ok")) { response.optString("error") }
            val values=response.getJSONArray("result")
            archivedBooks=(0 until values.length()).map { values.getJSONObject(it) }
        }.onFailure { errorMessage=it.message }
    }
    fun archiveAction(hash: String, action: String) {
        cancelGithubSyncWork()
        viewModelScope.launch {
            try {
                val response=withContext(Dispatchers.IO) { JSONObject(RustBridge.bookArchive(dataDir,JSONObject().put("hash",hash).put("action",action).toString()) ?: error("Missing archive result")) }
                check(response.getBoolean("ok")) { response.optString("error") }
                withContext(Dispatchers.IO) { library.reload() }; loadLibrary()
                githubSyncAttemptAt=0L
            } catch (error:Exception) { errorMessage=error.message }
        }
    }

    fun removeBook(entry: BookEntry) {
        removeBookByUri(entry.uri)
    }

    fun removeBookByUri(uri: String) {
        cancelGithubSyncWork()
        githubSyncAttemptAt=0L
        val removed = library.removeByUri(uri)
        if (removed) {
            coverCache.remove(uri)
            clearLastBookIfMatches(uri)
        }
        loadLibrary()
    }

    fun removeBookByIndex(index: Int) {
        if (index in books.indices) {
            removeBookByUri(books[index].uri)
        }
    }

    private fun clearLastBookIfMatches(uri: String) {
        val last = prefs.getString(PrefKeys.LAST_BOOK_URI, null)
        if (last == uri) {
            prefs.edit()
                .remove(PrefKeys.LAST_BOOK_URI)
                .remove(PrefKeys.LAST_BOOK_CHAPTER)
                .apply()
        }
    }

    /** Scan books/ dir and register any new epub files into the library */
    private fun scanBooksDir() {
        val dir = File(booksDir)
        if (!dir.isDirectory) return
        val knownUris = library.books.map { it.uri }.toMutableSet()
        val archivePayload=JSONObject(RustBridge.bookArchive(dataDir,"{}") ?: return)
        if (!archivePayload.optBoolean("ok")) return
        val archives=archivePayload.getJSONArray("result")
        for (index in 0 until archives.length()) { archives.getJSONObject(index).optJSONObject("entry")?.optString("path")?.let { knownUris.add(it) } }
        val epubs = dir.listFiles { f -> f.extension.equals("epub", ignoreCase = true) } ?: return
        var added = 0
        for (file in epubs) {
            val path = file.absolutePath
            if (path !in knownUris) {
                // Try to read title from lightweight metadata endpoint (no full chapter parse)
                val title = try {
                    val metaJson = RustBridge.readEpubMetadata(path)
                    if (metaJson != null) {
                        val meta = org.json.JSONObject(metaJson)
                        meta.optString("title", "").ifEmpty { null }
                    } else null
                } catch (_: Exception) { null } ?: file.nameWithoutExtension
                val entry = library.addOrUpdate(title, path, 0)
                knownUris.add(entry.uri)
                added++
            }
        }
        if (added > 0) {
            addLog("scanBooksDir: registered $added new book(s)")
        }
    }

    // ---- ���鼮 ----

    fun importBooksFromUris(uris: List<Uri>) {
        if (uris.isEmpty()) return
        if (uris.size == 1 && isTxtUri(uris.first())) {
            openFromUri(uris.first())
            return
        }

        viewModelScope.launch {
            libraryStartup?.join()
            isLoading = true
            loadingMessage = if (uris.size == 1) I18n.t("common.importing_book") else I18n.tf("common.importing_batch_books", (uris.size).toString())
            errorMessage = null

            var imported = 0
            var txtCount = 0
            val failures = mutableListOf<String>()

            try {
                withContext(Dispatchers.IO) {
                    uris.forEachIndexed { index, uri ->
                        withContext(Dispatchers.Main) {
                            loadingMessage = I18n.tf("common.importing_progress", (index + 1).toString(), (uris.size).toString())
                        }

                        if (isTxtUri(uri)) {
                            txtCount++
                            return@forEachIndexed
                        }

                        runCatching {
                            val file = copyUriToTempFile(uri, "epub")
                            try {
                                registerImportedEpub(file)
                            } finally {
                                runCatching { file.delete() }
                            }
                        }.onSuccess {
                            imported++
                        }.onFailure { e ->
                            failures.add(e.message ?: I18n.t("common.error_unknown"))
                        }
                    }
                }

                loadLibrary()
                val parts = mutableListOf<String>()
                if (imported > 0) parts.add(I18n.tf("common.imported_books_count", (imported).toString()))
                if (txtCount > 0) parts.add(I18n.tf("common.skipped_txt_count", (txtCount).toString()))
                if (failures.isNotEmpty()) parts.add(I18n.tf("common.failed_count", (failures.size).toString()))
                if (parts.isNotEmpty()) {
                    Toast.makeText(context, parts.joinToString("，"), Toast.LENGTH_SHORT).show()
                }
                if (imported == 0 && failures.isNotEmpty()) {
                    errorMessage = I18n.tf("common.error_import_failed_detail", (failures.first()).toString())
                }
            } catch (e: Exception) {
                errorMessage = I18n.tf("common.error_import_failed_detail", (e.message).toString())
            } finally {
                loadingMessage = null
                isLoading = false
            }
        }
    }

    fun openFromUri(uri: Uri) {
        cancelPendingBookOpen()
        val generation=openingSession.generation
        openingSession.job = viewModelScope.launch {
            libraryStartup?.join()
            isLoading = true
            loadingMessage = I18n.t("common.opening_book")
            errorMessage = null
            var tempFile: File? = null
            val isTxt = isTxtUri(uri)
            try {
                tempFile = withContext(Dispatchers.IO) {
                    copyUriToTempFile(uri, if (isTxt) "txt" else "epub")
                }
                val file = tempFile ?: throw Exception(I18n.t("error.temp_file_failed"))
                if (isTxt) {
                    // TXT: 打开导入对话框（临时文件保留给对话框使用）
                    openTxtImportDialog(file.absolutePath)
                } else {
                    parseAndOpen(file)
                }
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: Exception) {
                if(generation==openingSession.generation) {
                    errorMessage = I18n.tf("common.error_open_failed_detail", (e.message).toString())
                }
            } finally {
                if (!isTxt) {
                    tempFile?.let { file ->
                        if (file.exists() && file.parentFile?.name == "imports") {
                            runCatching { file.delete() }
                        }
                    }
                }
                if(generation==openingSession.generation) { loadingMessage = null; isLoading = false }
            }
        }
    }

    fun performSearch(query: String) {
        if (query.isBlank()) {
            searchResults = emptyList()
            return
        }
        val path = currentBookUri ?: return
        viewModelScope.launch(Dispatchers.IO) {
            val json = RustBridge.searchBook(path, query)
            val results = if (json != null) {
                try {
                    jsonParser.decodeFromString<List<SearchResult>>(json)
                } catch (_: Exception) {
                    emptyList()
                }
            } else emptyList()
            withContext(Dispatchers.Main) {
                searchResults = results
            }
        }
    }

    fun cancelPendingBookOpen() {
        openingSession.cancel()
        isLoading=false; loadingMessage=null
    }
    fun openFromPath(filePath: String, chapter: Int = 0) {
        android.util.Log.d("READER-RESUME", "openFromPath path=$filePath chapter=$chapter")
        cancelPendingBookOpen()
        val generation=openingSession.generation
        openingSession.job = viewModelScope.launch {
            libraryStartup?.join()
            isLoading = true
            loadingMessage = I18n.t("common.opening_book")
            errorMessage = null
            try {
                val file = File(filePath)
                if (!file.exists()) {
                    errorMessage = I18n.t("common.error_read_file_failed")
                    return@launch
                }
                val (resolvedChapter, resolvedBlock) = resolveStartPosition(filePath, chapter)
                parseAndOpen(file, resolvedChapter, resolvedBlock)
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: Exception) {
                // A cancelled blocking JNI call reports IOException. Its stale
                // failure must not become an alert after returning to the shelf.
                if(generation==openingSession.generation) {
                    errorMessage = I18n.tf("common.error_open_failed_detail", e.message.orEmpty())
                }
            } finally {
                if(generation==openingSession.generation) { loadingMessage = null; isLoading = false }
            }
        }
    }

    private fun resolveStartPosition(filePath: String, chapter: Int): Pair<Int, Int> {
        val requested = chapter.coerceAtLeast(0)
        val entry = library.books.firstOrNull { it.uri == filePath }
        val fromLibrary = entry?.lastChapter?.coerceAtLeast(0) ?: 0

        // Source 2: per-book SharedPreferences (simple int, no serialization)
        val bookId = entry?.id ?: ""
        val fromPrefs = if (bookId.isNotEmpty()) {
            prefs.getInt("book_ch_$bookId", 0)
        } else 0

        // Source 3: Rust-written book config file (raw JSON, bypasses kotlinx.serialization)
        val fromConfig = entry?.configPath?.let { cfgPath ->
            runCatching {
                val cfgFile = File(cfgPath)
                if (!cfgFile.exists()) return@runCatching 0
                val jsonObj = org.json.JSONObject(cfgFile.readText())
                jsonObj.optInt("last_chapter", 0)
            }.getOrDefault(0)
        } ?: 0

        val resolved = resolveResumeChapter(entry?.lastChapter, requested, maxOf(fromPrefs, fromConfig))
        android.util.Log.d("READER-RESUME",
            "resolveStartChapter path=$filePath requested=$requested " +
                "fromLibrary=$fromLibrary fromPrefs=$fromPrefs fromConfig=$fromConfig => $resolved")
        val resolvedBlock = if (entry?.lastChapter == resolved) entry.lastBlock else 0
        return resolved to resolvedBlock.coerceAtLeast(0)
    }

    private suspend fun parseAndOpen(file: File, startChapter: Int = 0, startBlock: Int = 0) {
        val ticket = RustBridge.beginBookOpen()
        openingSession.started(ticket)
        try {
        val inputPath = file.absolutePath
        val managedEntry = withContext(Dispatchers.IO) {
            val fallbackTitle = file.nameWithoutExtension.ifBlank { "Untitled" }
            library.addOrUpdate(fallbackTitle, inputPath, startChapter.coerceAtLeast(0))
        }
        val managedPath = managedEntry.uri

        val book = withContext(Dispatchers.IO) {
            val metadataJson = RustBridge.openBookCancellable(managedPath, ticket, startChapter)
                ?: throw Exception(I18n.t("reader.error_epub_read_failed"))
            
            val metadata = jsonParser.decodeFromString<BookMetadataDto>(metadataJson)

            // Parse review chapter info from DTO with safe fallback
            val reviewIndices = try {
                metadata.reviewChapterIndices.toMutableSet()
            } catch (e: Exception) {
                android.util.Log.e("ReaderViewModel", I18n.t("common.error_parse_paragraph_reviews_failed"), e)
                mutableSetOf<Int>()
            }
            val chapterReviewMap = try {
                metadata.chapterReviews.associate { it.main to it.review }.toMutableMap()
            } catch (e: Exception) {
                android.util.Log.e("ReaderViewModel", I18n.t("common.error_parse_paragraph_reviews_failed"), e)
                mutableMapOf<Int, Int>()
            }
            withContext(Dispatchers.Main) {
                reviewChapterIndices = reviewIndices
                chapterReviews = chapterReviewMap
                // Reset review panel state so a stale panel never persists across book switches
                showReviewPanel = false
                reviewPanelChapter = null
            }
            
            val lazyChapters = LazyChapterList(metadata.chapterCount) { index ->
                try {
                    val json = RustBridge.getChapter(managedPath, index)
                        ?: return@LazyChapterList Chapter("Error", emptyList())
                    val dto = jsonParser.decodeFromString<ChapterDto>(json)
                    Chapter(title = dto.title, blocks = dto.blocks, sourceHref = dto.sourceHref, images = dto.images)
                } catch (e: Exception) {
                    e.printStackTrace()
                    Chapter("Error", emptyList())
                }
            }

            // The resume chapter must not trigger JSON parsing/bridge IO from Compose.
            if(lazyChapters.isNotEmpty()) lazyChapters[startChapter.coerceIn(0,lazyChapters.lastIndex)]
            var coverBytes: ByteArray? = null
            if (metadata.hasCover) {
                val coverB64 = RustBridge.getCover(managedPath)
                if (coverB64 != null) {
                    coverBytes = Base64.decode(coverB64, Base64.DEFAULT)
                }
            }
            
            EpubBook(
                title = metadata.title,
                chapters = lazyChapters,
                toc = metadata.toc.map { it.title to it.chapterIndex },
                coverData = coverBytes,
                chapterTitles = metadata.chapterTitles
            )
        } ?: throw Exception(I18n.t("reader.error_epub_read_failed"))

        withContext(Dispatchers.IO) { RustBridge.recordBookOpened(dataDir, managedPath) }
        localProgressRevision++ // Recent-reading metadata is also part of the fast sync phase.
        currentBook = book
        currentBookUri = managedPath
        currentChapter = startChapter.coerceIn(0, (book.chapters.size - 1).coerceAtLeast(0))
        currentPage = 0
        currentBlock = startBlock.coerceIn(
            0,
            (book.chapters.getOrNull(currentChapter)?.blocks?.lastIndex ?: 0).coerceAtLeast(0)
        )
        android.util.Log.d("READER-RESUME",
            "parseAndOpen startChapter=$startChapter chapterCount=${book.chapters.size} => currentChapter=$currentChapter")

        val savedPosition=library.books.firstOrNull { it.uri==managedPath }
        val savedBlock=book.chapters.getOrNull(currentChapter)?.blocks?.getOrNull(currentBlock)
        val savedText=when(savedBlock) {is ContentBlock.Paragraph->savedBlock.spans.joinToString(""){it.text};is ContentBlock.Heading->savedBlock.spans.joinToString(""){it.text};else->""}
        currentCharOffset=utf16TextOffset(savedText, savedPosition?.lastCharOffset ?: 0)
        positionRevision++
        val chapterTitle = try {
            book.chapters[currentChapter].title
        } catch (_: Exception) { null }

        val persistedEntry = library.addOrUpdate(book.title, managedPath, currentChapter, chapterTitle)
        if (book.coverData != null) {
            requestCover(persistedEntry.uri)
        }
        currentBookUri = persistedEntry.uri
        githubSyncOffer = null
        returnHistory = if (prefs.getString(PrefKeys.LAST_BOOK_URI, null) == persistedEntry.uri) {
            runCatching { jsonParser.decodeFromString(kotlinx.serialization.builtins.ListSerializer(ReaderReturnPoint.serializer()), prefs.getString(historyKey(persistedEntry.uri), "[]") ?: "[]").takeLast(16) }.getOrDefault(emptyList())
        } else emptyList()
        currentPageLabel = ""
        prefs.edit()
            .putString(PrefKeys.LAST_BOOK_URI, persistedEntry.uri)
            .putInt(PrefKeys.LAST_BOOK_CHAPTER, currentChapter)
            .apply {
                if (persistedEntry.id.isNotEmpty()) {
                    putInt("book_ch_${persistedEntry.id}", currentChapter)
                }
            }
            .apply()

        loadLibrary()
        loadBookConfig()
        } finally {
            RustBridge.finishBookOpen(ticket)
            openingSession.finished(ticket)
        }
    }

    private fun isTxtUri(uri: Uri): Boolean {
        val mime = context.contentResolver.getType(uri)
        if (mime == "text/plain") return true
        val path = uri.path ?: uri.lastPathSegment ?: ""
        return path.endsWith(".txt", ignoreCase = true)
    }

    private suspend fun copyUriToTempFile(uri: Uri, ext: String = "epub"): File = withContext(Dispatchers.IO) {
        val importDir = File(context.cacheDir, "imports").also { it.mkdirs() }
        val dest = File(importDir, "import_${System.currentTimeMillis()}_${UUID.randomUUID()}.$ext")
        context.contentResolver.openInputStream(uri)?.use { input ->
            FileOutputStream(dest).use { output ->
                input.copyTo(output, bufferSize = 1024 * 1024)
            }
        } ?: throw Exception(I18n.t("common.error_read_file_failed"))
        dest
    }

    private fun registerImportedEpub(file: File): BookEntry {
        val title = runCatching {
            val metaJson = RustBridge.readEpubMetadata(file.absolutePath)
            if (metaJson != null) {
                val meta = org.json.JSONObject(metaJson)
                meta.optString("title", "").ifBlank { null }
            } else null
        }.getOrNull() ?: file.nameWithoutExtension.ifBlank { "Untitled" }

        return library.addOrUpdate(title, file.absolutePath, 0)
    }

    fun goToChapter(index: Int, remember: Boolean = true) {
        if (reviewChapterIndices.contains(index)) { openReviewPanel(index); return }
        goToPosition(index, remember = remember)
    }

    fun goBackChapter() {
        val previous = returnHistory.lastOrNull() ?: return
        goToPosition(previous.location.chapter, previous.location.block, previous.location.charOffset, remember = false, returning = true)
    }

    fun nextChapter() {
        val book = currentBook ?: return
        if (currentChapter < book.chapters.size - 1) {
            var next = currentChapter + 1
            // Skip review chapters
            while (next < book.chapters.size && reviewChapterIndices.contains(next)) {
                next++
            }
            if (next < book.chapters.size) {
                currentChapter = next
                currentPage = 0
                currentBlock = 0
                currentCharOffset = 0
                saveProgress()
                updateBookmarkState()
                checkContributionPrompt()
            } else {
                Toast.makeText(context, I18n.t("reader.at_last_chapter"), Toast.LENGTH_SHORT).show()
            }
        }
    }

    fun prevChapter() {
        val book = currentBook ?: return
        if (currentChapter > 0) {
            var prev = currentChapter - 1
            // Skip review chapters
            while (prev >= 0 && reviewChapterIndices.contains(prev)) {
                prev--
            }
            if (prev >= 0) {
                currentChapter = prev
                currentPage = 0
                currentBlock = 0
                currentCharOffset = 0
                saveProgress()
                updateBookmarkState()
                checkContributionPrompt()
            } else {
                Toast.makeText(context, I18n.t("reader.at_first_chapter"), Toast.LENGTH_SHORT).show()
            }
        }
    }

    fun openReviewPanel(chapterIndex: Int, anchorId: String? = null) {
        showReviewPanel = true
        reviewPanelChapter = chapterIndex
        reviewPanelAnchor = anchorId
        reviewPanelShowAll = false
    }

    fun openReviewPanelForCurrentChapter() {
        val reviewCh = chapterReviews[currentChapter] ?: return
        showReviewPanel = true
        reviewPanelChapter = reviewCh
        reviewPanelAnchor = null
    }

    fun closeReviewPanel() {
        showReviewPanel = false
        reviewPanelChapter = null
        reviewPanelAnchor = null
        reviewPanelShowAll = false
    }

    fun setPage(page: Int) {
        currentPage = page
    }

    fun updateReadingPosition(block: Int, charOffset: Int = 0) {
        val maxBlock = currentBook?.chapters?.getOrNull(currentChapter)?.blocks?.lastIndex ?: 0
        val next = block.coerceIn(0, maxBlock.coerceAtLeast(0))
        if (next == currentBlock && charOffset == currentCharOffset) return
        currentBlock = next
        currentCharOffset = charOffset.coerceAtLeast(0)
        saveProgress()
    }

    fun adjustFontSize(delta: Float) {
        fontSize = (fontSize + delta).coerceIn(12f, 40f)
        persistSettings()
    }

    fun updateFontSize(size: Float) {
        fontSize = size.coerceIn(12f, 40f)
        persistSettings()
    }

    fun updateScrollMode(scroll: Boolean) {
        isScrollMode = scroll
        persistSettings()
    }

    fun updateAppearanceSignals(system: Boolean, ambient: Boolean?, available: Boolean) {
        systemAppearanceDark = system; ambientAppearanceDark = ambient; ambientSensorAvailable = available
    }
    fun updateAppearanceLinkage(value: com.zhongbai233.epub.reader.model.AppearanceLinkage) {
        appearanceLinkage = value
        prefs.edit().putString(com.zhongbai233.epub.reader.model.AppearanceLinkage.storageKey,value.encoded()).apply()
    }
    fun toggleReaderDayNight(@Suppress("UNUSED_PARAMETER") night: Boolean) {
        updateAppearanceLinkage(appearanceLinkage.toggleReading(systemAppearanceDark,ambientAppearanceDark))
    }
    fun updateReadingAppearance(value: com.zhongbai233.epub.reader.model.ReadingAppearance) {
        readingAppearance = value.normalized()
        prefs.edit().putString(com.zhongbai233.epub.reader.model.ReadingAppearance.storageKey, readingAppearance.encoded()).apply()
    }
    fun updateDarkMode(dark: Boolean) {
        updateAppearanceLinkage(appearanceLinkage.setApp(dark))
        persistSettings()
    }

    fun updateReaderBgColor(index: Int) {
        readerBgColorIndex = index.coerceAtLeast(0)
        persistSettings()
    }

    fun updateReaderCustomBgColor(argb: Int) {
        readerCustomBgColorArgb = argb
        persistSettings()
    }

    fun updateReaderFontColor(index: Int) {
        readerFontColorIndex = index.coerceAtLeast(0)
        persistSettings()
    }

    fun updateReaderCustomFontColor(argb: Int) {
        readerCustomFontColorArgb = argb
        persistSettings()
    }

    fun updateReaderFontFamily(value: String) {
        readerFontFamily = value
        persistSettings()
    }

    fun updateReaderPageAnimation(value: String) {
        readerPageAnimation = value
        persistSettings()
    }

    fun updateReaderBgImage(uri: String?) {
        readerBgImageUri = uri
        persistSettings()
    }

    fun updateReaderBgImageAlpha(alpha: Float) {
        readerBgImageAlpha = alpha.coerceIn(0f, 1f)
        persistSettings()
    }

    fun updateAutoStartSharing(start: Boolean) {
        autoStartSharing = start
        persistSettings()
    }

    fun updateLanguage(code: String) {
        readerLanguage = I18n.normalize(code)
        I18n.setLanguage(readerLanguage)
        persistSettings()
    }

    fun updateShowImmersiveStatus(show: Boolean) {
        showImmersiveStatus = show
        persistSettings()
    }

    fun updateLineSpacing(value: Float) {
        lineSpacing = value.coerceIn(1.0f, 3.0f)
        persistSettings()
    }

    fun updateForceSingleColumn(value: Boolean) {
        forceSingleColumn = value
        persistSettings()
    }

    fun updateParaSpacing(value: Float) {
        paraSpacing = value.coerceIn(0.0f, 2.0f)
        persistSettings()
    }

    fun updateTextIndent(value: Int) {
        textIndent = value.coerceIn(0, 4)
        persistSettings()
    }

    fun updateTitleFontScale(value: Float) {
        titleFontScale = value.coerceIn(1.0f, 2.5f)
        persistSettings()
    }

    fun updateTranslateApiUrl(value: String) {
        translateApiUrl = value
        persistSettings()
    }

    private fun loadServiceKey(kind:String,legacyKey:String):String {
        val saved=GitHubTokenStore.load(context,"service_$kind")
        if(saved!=null)return saved
        val legacy=prefs.getString(legacyKey,"")?:""
        if(legacy.isNotEmpty() && saveServiceKey(kind,legacy))prefs.edit().remove(legacyKey).commit()
        return legacy
    }
    private fun saveServiceKey(kind:String,value:String):Boolean {
        val result=if(value.isEmpty())runCatching{GitHubTokenStore.delete(context,"service_$kind")}else GitHubTokenStore.save(context,value,"service_$kind")
        result.exceptionOrNull()?.let{errorMessage=it.message?:I18n.t("common.error_unknown")}
        return result.isSuccess
    }
    fun updateTranslateApiKey(value: String) {
        if(!saveServiceKey("translate",value))return
        prefs.edit().remove(PrefKeys.TRANSLATE_API_KEY).apply()
        translateApiKey = value
        persistSettings()
    }

    fun updateDictionaryApiUrl(value: String) {
        dictionaryApiUrl = value
        persistSettings()
    }

    fun updateDictionaryApiKey(value: String) {
        if(!saveServiceKey("dictionary",value))return
        prefs.edit().remove(PrefKeys.DICTIONARY_API_KEY).apply()
        dictionaryApiKey = value
        persistSettings()
    }

    fun updateTtsVoiceName(value: String) {
        ttsVoiceName = value
        persistSettings()
    }

    fun updateTtsRate(value: Int) {
        ttsRate = value
        persistSettings()
    }

    fun updateTtsVolume(value: Int) {
        ttsVolume = value
        persistSettings()
    }

    fun updateCscMode(value: String) {
        android.util.Log.d("CscEngine", "updateCscMode: old=$cscMode new=$value isReady=${cscEngine.isReady} loading=$cscModelLoading")
        cscMode = value
        persistSettings()
        // If enabling CSC, try to load model
        if (value != "none" && !cscEngine.isReady && !cscModelLoading) {
            android.util.Log.d("CscEngine", "updateCscMode: loading model")
            loadCscModel()
        }
        // If model is ready and mode changed, re-run check on current chapter
        if (cscEngine.isReady && value != "none") {
            android.util.Log.d("CscEngine", "updateCscMode: running check")
            runCscCheckOnCurrentChapter()
        }
        if (value == "none") {
            cscCorrections = emptyList()
        }
    }

    fun updateCscThreshold(value: String) {
        cscThreshold = value
        persistSettings()
        // Re-run check with new threshold if active
        if (cscMode != "none" && cscEngine.isReady) {
            runCscCheckOnCurrentChapter()
        }
    }

    private fun loadCscModel() {
        val dataDir = context.filesDir.absolutePath
        if (!CscEngine.isModelAvailable(dataDir)) {
            android.util.Log.i("CSC", "Model not downloaded yet")
            return
        }
        if (!com.epub.reader.csc.CscNativeLoader.isAvailable(context)) {
            android.util.Log.i("CSC", "ONNX Runtime native libs not downloaded yet")
            return
        }
        cscModelLoading = true
        viewModelScope.launch(Dispatchers.IO) {
            // dlopen the runtime libs before any ai.onnxruntime.* class is touched.
            val nativeResult = com.epub.reader.csc.CscNativeLoader.ensureLoaded(context)
            if (nativeResult.isFailure) {
                withContext(Dispatchers.Main) {
                    cscModelLoading = false
                    android.util.Log.e(
                        "CSC",
                        "Native loader failed",
                        nativeResult.exceptionOrNull()
                    )
                }
                return@launch
            }
            val result = cscEngine.load(dataDir)
            withContext(Dispatchers.Main) {
                cscModelLoading = false
                cscModelReady = result.isSuccess
                if (result.isSuccess && cscMode != "none") {
                    runCscCheckOnCurrentChapter()
                }
                result.exceptionOrNull()?.let {
                    android.util.Log.e("CSC", "Failed to load model", it)
                }
            }
        }
    }

    fun downloadCscModel() {
        val dataDir = context.filesDir.absolutePath
        val modelDir = CscEngine.modelDir(dataDir)
        if (!modelDir.exists()) modelDir.mkdirs()
        cscModelLoading = true
        viewModelScope.launch(Dispatchers.IO) {
            try {
                // 1. Pull the ONNX Runtime native libraries that we deliberately
                //    excluded from the APK to keep the install size small.
                val nativeRes = com.epub.reader.csc.CscNativeLoader.ensureLoaded(context)
                if (nativeRes.isFailure) {
                    throw nativeRes.exceptionOrNull()
                        ?: IllegalStateException("Native loader failed")
                }
                // 2. Pull the model + vocab.
                downloadFile(CscEngine.MODEL_URL, CscEngine.modelPath(dataDir))
                downloadFile(CscEngine.VOCAB_URL, CscEngine.vocabPath(dataDir))
                withContext(Dispatchers.Main) {
                    loadCscModel()
                }
            } catch (e: Exception) {
                withContext(Dispatchers.Main) {
                    cscModelLoading = false
                    android.util.Log.e("CSC", "Download failed", e)
                }
            }
        }
    }

    private fun downloadFile(url: String, dest: File) {
        val conn = java.net.URL(url).openConnection() as java.net.HttpURLConnection
        try {
            conn.connectTimeout = 30000
            conn.readTimeout = 60000
            conn.inputStream.use { input ->
                FileOutputStream(dest).use { output ->
                    input.copyTo(output)
                }
            }
        } finally {
            conn.disconnect()
        }
    }

    private fun runCscCheckOnCurrentChapter() {
        val book = currentBook ?: run {
            android.util.Log.d("CscEngine", "runCscCheck: currentBook is null, skipping")
            return
        }
        val chapter = book.chapters.getOrNull(currentChapter) ?: run {
            android.util.Log.d("CscEngine", "runCscCheck: chapter[$currentChapter] is null, skipping")
            return
        }
        val threshold = when (cscThreshold) {
            "conservative" -> 0.95f
            "standard" -> 0.90f
            "aggressive" -> 0.80f
            else -> 0.90f
        }
        android.util.Log.d("CscEngine", "runCscCheck: threshold=$cscThreshold($threshold) blocks=${chapter.blocks.size}")
        viewModelScope.launch(Dispatchers.IO) {
            val fullText = chapter.blocks
                .mapNotNull { block ->
                    when (block) {
                        is ContentBlock.Paragraph -> block.spans.joinToString("") { it.text }
                        is ContentBlock.Heading -> block.spans.joinToString("") { it.text }
                        else -> null
                    }
                }
                .joinToString("\n")
            android.util.Log.d("CscEngine", "runCscCheck: fullTextLen=${fullText.length}")
            val results = cscEngine.check(fullText, threshold)
            android.util.Log.d("CscEngine", "runCscCheck: results=${results.size}")
            withContext(Dispatchers.Main) {
                cscCorrections = results
            }
        }
    }

    // ── TTS playback controls ──

    fun ttsStartPlayback() {
        val book = currentBook ?: return
        val chapter = book.chapters.getOrNull(currentChapter) ?: return
        ttsManager.voiceName = ttsVoiceName
        ttsManager.rate = ttsRate
        ttsManager.volume = ttsVolume
        showTtsBar = true
        ttsManager.start(chapter.blocks)
    }

    fun ttsStopPlayback() {
        ttsManager.stop()
    }

    fun ttsTogglePause() {
        ttsManager.togglePause()
    }

    fun ttsToggleBar() {
        showTtsBar = !showTtsBar
        if (!showTtsBar) {
            ttsManager.stop()
        }
    }

    fun ttsCloseTtsBar() {
        ttsManager.stop()
        showTtsBar = false
    }

    fun closeBook() {
        navigationGeneration++
        remoteNavigation = null
        currentBookUri?.let { prefs.edit().remove(historyKey(it)).apply() }
        returnHistory = emptyList()
        val closingUri = currentBookUri
        android.util.Log.d("READER-RESUME", "closeBook uri=$closingUri currentChapter=$currentChapter")
        saveProgress()
        if (closingUri != null) {
            clearLastBookIfMatches(closingUri)
        }
        currentBook = null
        currentBookUri = null
        githubSyncOffer = null
        currentChapter = 0
        currentPage = 0
        currentBlock = 0
        currentCharOffset = 0
    }

    private fun saveProgress() {
        val uri = currentBookUri ?: return
        if(library.books.none {it.uri==uri}) return
        val chapterTitle = try {
            currentBook?.chapters?.get(currentChapter)?.title
        } catch (_: Exception) { null }
        val block=currentBook?.chapters?.getOrNull(currentChapter)?.blocks?.getOrNull(currentBlock)
        val text=when(block) { is ContentBlock.Paragraph->block.spans.joinToString("") {it.text}; is ContentBlock.Heading->block.spans.joinToString("") {it.text}; else->"" }
        val scalarOffset=unicodeScalarOffset(text, currentCharOffset)
        val previous=library.books.firstOrNull {it.uri==uri}
        if(previous?.lastChapter != currentChapter || previous.lastBlock != currentBlock || previous.lastCharOffset != scalarOffset) localProgressRevision++
        try { library.updatePosition(uri, currentChapter, chapterTitle, currentBlock, scalarOffset) }
        catch (failure: Exception) {
            addLog("Progress save failed: ${failure.message}")
            errorMessage = I18n.t("reader.error_save_book_failed")
            return
        }

        // Write chapter to SharedPreferences: global key + per-book key (resilient backup)
        val bookId = library.books.firstOrNull { it.uri == uri }?.id
        prefs.edit().apply {
            putInt(PrefKeys.LAST_BOOK_CHAPTER, currentChapter)
            if (bookId != null) {
                putInt("book_ch_$bookId", currentChapter)
            }
        }.apply()

        val shelfIndex=books.indexOfFirst {it.uri==uri}
        if(shelfIndex>=0) library.books.firstOrNull {it.uri==uri}?.let {books[shelfIndex]=it}
    }

    fun dismissError() {
        errorMessage = null
    }

    fun dismissUpdateDialog() {
        showUpdateDialog = false
    }

    // ---- �������� ----

    private val booksDir: String
        get() = File(context.filesDir, "books").also { it.mkdirs() }.absolutePath

    private val dataDir: String
        get() = context.filesDir.absolutePath

    fun exportFeedbackLogs(): String? {
        return try {
            val outDir = File(context.getExternalFilesDir(null) ?: context.filesDir, "feedback_logs")
            outDir.mkdirs()
            val outFile = File(outDir, "feedback_log_${System.currentTimeMillis()}.txt")

            val pkgInfo = context.packageManager.getPackageInfo(context.packageName, 0)
            val logsSnapshot = sharingLogs.toList()
            val booksSnapshot = books.toList()

            val report = buildString {
                appendLine("RustEpubReader Android Feedback Log")
                appendLine("generated_at=${java.util.Date()}")
                appendLine("package=${context.packageName}")
                appendLine("version_name=${pkgInfo.versionName ?: "unknown"}")
                appendLine("version_code=${androidx.core.content.pm.PackageInfoCompat.getLongVersionCode(pkgInfo)}")
                appendLine("device=${android.os.Build.MANUFACTURER} ${android.os.Build.MODEL}")
                appendLine("android=${android.os.Build.VERSION.RELEASE} (SDK ${android.os.Build.VERSION.SDK_INT})")
                appendLine("data_dir=$dataDir")
                appendLine("books_dir=$booksDir")
                appendLine("books_count=${booksSnapshot.size}")
                appendLine("sharing_server_running=$sharingServerRunning")
                appendLine("sharing_server_addr=$sharingServerAddr")
                appendLine("language=$readerLanguage")
                appendLine("----")
                appendLine("Books:")
                booksSnapshot.forEach {
                    appendLine("- ${it.title} | ${it.uri} | chapter=${it.lastChapter} | lastOpened=${it.lastOpened}")
                }
                appendLine("----")
                appendLine("Sharing Logs:")
                logsSnapshot.forEach { appendLine(it) }
            }

            outFile.writeText(report, Charsets.UTF_8)
            addLog("exportFeedbackLogs: ${outFile.absolutePath}")
            outFile.absolutePath
        } catch (e: Exception) {
            addLog("exportFeedbackLogs failed: ${e.message}")
            null
        }
    }

    fun generatePin() {
        sharingPin = RustBridge.generatePin() ?: "0000"
    }

    fun startSharingServer() {
        if (sharingServerRunning) return
        if (sharingPin.isEmpty()) generatePin()
        val pin = sharingPin  // capture before switching threads
        addLog("startServer: pin='$pin'")
        viewModelScope.launch(Dispatchers.IO) {
            val result = RustBridge.startSharingServer(dataDir, booksDir, pin)
            addLog("startServer result: $result")
            withContext(Dispatchers.Main) {
                if (result != null) {
                    try {
                        val json = org.json.JSONObject(result)
                        if (json.optBoolean("ok")) {
                            sharingServerRunning = true
                            sharingServerAddr = json.optString("addr", "")
                            val identity=org.json.JSONObject(RustBridge.readingTools(org.json.JSONObject().put("op","peer_identity").put("root",dataDir).toString())?:"{}").optJSONObject("result")
                            if(identity!=null)bonjour.publish(identity.optString("id"),identity.optString("name"),sharingServerAddr.substringAfterLast(':').toIntOrNull()?:0)
                            sharingStatus = ""
                        } else {
                            sharingStatus = json.optString("error", I18n.t("common.error_unknown"))
                        }
                    } catch (_: Exception) {
                        sharingStatus = "Server start failed"
                    }
                }
            }
        }
    }

    fun stopSharingServer() {
        bonjour.unpublish()
        viewModelScope.launch(Dispatchers.IO) {
            RustBridge.stopSharingServer()
            withContext(Dispatchers.Main) {
                sharingServerRunning = false
                sharingServerAddr = ""
            }
        }
    }

    /** Connect to a specific peer using their PIN �� called from the pairing dialog */
    fun connectToPeer(addr: String, pin: String, deviceId: String) {
        connectToPeerInternal(addr, pin, deviceId, isAuto = false)
    }

    private fun connectToPeerInternal(addr: String, pin: String, deviceId: String, isAuto: Boolean) {
        if (isAuto) {
            addLog("autoSync connectToPeer: addr=$addr deviceId=$deviceId")
        } else {
            addLog("connectToPeer: addr=$addr pin='$pin' deviceId=$deviceId")
        }
        sharingStatus = I18n.t("share.status_connecting")
        viewModelScope.launch(Dispatchers.IO) {
            try {
                val result = RustBridge.connectAndListBooks(addr, pin, deviceId, dataDir, booksDir)
                if (isAuto) {
                    addLog("autoSync connectToPeer result: $result")
                } else {
                    addLog("connectToPeer result: $result")
                }
                withContext(Dispatchers.Main) {
                    var hasError = false
                    if (result != null) {
                        try {
                            val jsonObj = org.json.JSONObject(result)
                            if (jsonObj.has("error")) {
                                val err = jsonObj.getString("error")
                                val phase = jsonObj.optString("phase", "connect")
                                sharingStatus = if (phase == "sync") {
                                    I18n.tf1("share.status_sync_failed", err)
                                } else {
                                    I18n.tf1("share.status_connect_failed", err)
                                }
                                hasError = true
                            }
                        } catch (_: Exception) { }

                        if (!hasError) {
                            sharingStatus = I18n.t("share.status_sync_done")
                            refreshLibrary()
                        }
                    } else {
                        sharingStatus = I18n.tf1("share.status_connect_failed", "no response")
                    }
                }
            } catch (e: Exception) {
                addLog("connectToPeer exception: ${e.message}")
                withContext(Dispatchers.Main) {
                    sharingStatus = I18n.tf1("share.status_connect_failed", e.message ?: "exception")
                }
            } finally {
                if (isAuto) {
                    withContext(Dispatchers.Main) {
                        autoSyncInProgress = false
                    }
                }
            }
        }
    }

    private fun tryAutoSyncWithPairedPeer(peers: List<DiscoveredPeer>) {
        if (autoSyncInProgress || backupBusy) return
        if (pairedDevices.isEmpty() || peers.isEmpty()) return

        val pairedPeer = peers.firstOrNull { peer ->
            pairedDevices.any { it.deviceId == peer.deviceId }
        } ?: return

        val now = System.currentTimeMillis()
        val last = autoSyncLastAttemptAt[pairedPeer.deviceId] ?: 0L
        if (now - last < autoSyncCooldownMs) return

        autoSyncLastAttemptAt[pairedPeer.deviceId] = now
        autoSyncInProgress = true
        addLog("autoSync trigger: peer=${pairedPeer.deviceName} addr=${pairedPeer.addr}")
        connectToPeerInternal(pairedPeer.addr, "", pairedPeer.deviceId, isAuto = true)
    }

    fun manualSync() {
        val addr = connectAddrInput.trim()
        val pin = connectPinInput.trim()
        if (addr.isEmpty()) return
        connectToPeer(addr, pin, "")
    }

    fun loadPairedDevices() {
        viewModelScope.launch(Dispatchers.IO) {
            val result = RustBridge.getPairedDevices(dataDir)
            withContext(Dispatchers.Main) {
                if (result != null) {
                    try {
                        val devices = jsonParser.decodeFromString<List<PairedDevice>>(result)
                        pairedDevices.clear()
                        pairedDevices.addAll(devices)
                        tryAutoSyncWithPairedPeer(discoveredPeers.toList())
                    } catch (_: Exception) { }
                }
            }
        }
    }

    fun removePairedDevice(deviceId: String) {
        addLog("removePairedDevice: $deviceId")
        viewModelScope.launch(Dispatchers.IO) {
            RustBridge.removePairedDevice(dataDir, deviceId)
            loadPairedDevices()
        }
    }

    private var bonjourPeers:List<DiscoveredPeer> = emptyList()
    private val bonjour by lazy {com.zhongbai233.epub.reader.util.NearbyDiscovery(context){peers->
        bonjourPeers=peers;refreshDiscoveredPeers()
    }}
    fun startDiscovery() {
        bonjour.start()
        if (!discoveryStarted) {
            addLog("startDiscovery")
            // Acquire multicast lock so Android doesn't filter UDP broadcasts
            if (multicastLock == null) {
                val wifi = context.applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
                multicastLock = wifi?.createMulticastLock("epub_reader_discovery")?.apply {
                    setReferenceCounted(false)
                    acquire()
                }
                addLog("multicastLock acquired: ${multicastLock?.isHeld}")
            }
            viewModelScope.launch(Dispatchers.IO) {
                RustBridge.startDiscoveryListener(dataDir)
            }
            discoveryStarted = true
        }

        if (discoveryPollingJob?.isActive != true) {
            discoveryPollingJob = viewModelScope.launch(Dispatchers.IO) {
                while (isActive) {
                    refreshDiscoveredPeersOnce()
                    delay(3000)
                }
            }
        }
    }

    fun refreshDiscoveredPeers() {
        viewModelScope.launch(Dispatchers.IO) {
            refreshDiscoveredPeersOnce()
        }
    }

    private suspend fun refreshDiscoveredPeersOnce() {
        val result = RustBridge.getDiscoveredPeers() ?: return
        addLog("refreshPeers: $result")
        withContext(Dispatchers.Main) {
            try {
                val peers = (jsonParser.decodeFromString<List<DiscoveredPeer>>(result)+bonjourPeers).distinctBy{it.deviceId}
                discoveredPeers.clear()
                discoveredPeers.addAll(peers)
                tryAutoSyncWithPairedPeer(peers)
            } catch (_: Exception) { }
        }
    }

    // ── TXT 导入 ──

    fun openTxtImportDialog(path: String) {
        txtExcluded=emptySet();txtIncluded=emptySet();txtSourceQuery="";txtSourceHits=emptyList();txtImportPreviews=emptyList()
        txtImportPath = path
        txtImportTitle = File(path).nameWithoutExtension
        txtImportAuthor = ""
        txtImportRegex = ""
        txtImportHeuristic = true
        txtImportError = null
        txtImportConverting = false
        showTxtImport = true
        refreshTxtPreviews()
    }

    var txtExcluded by mutableStateOf<Set<Int>>(emptySet())
    var txtIncluded by mutableStateOf<Set<Int>>(emptySet())
    var txtSourceQuery by mutableStateOf("")
    var txtSourceHits by mutableStateOf<List<TxtChapterPreview>>(emptyList())
    var txtHeadingCount by mutableIntStateOf(0)
    var txtChapterCount by mutableIntStateOf(0)
    var txtAnalyzing by mutableStateOf(false)
    private var txtPreviewJob:Job?=null
    private val txtWorkLock=kotlinx.coroutines.sync.Mutex()
    private var txtPreviewRevision = 0L
    private var txtSearchRevision = 0L
    private fun txtRequest(action: String) = org.json.JSONObject().put("op", "txt").put("action", action)
        .put("path", txtImportPath).put("use_heuristic", txtImportHeuristic).put("custom_regex", txtImportRegex)
        .put("excluded_lines", org.json.JSONArray(txtExcluded.toList())).put("included_lines", org.json.JSONArray(txtIncluded.toList()))
    private fun txtRows(arr: org.json.JSONArray) = (0 until arr.length()).map { i -> val obj=arr.getJSONObject(i)
        TxtChapterPreview(obj.getString("title"),obj.getInt("line_start"),obj.optInt("char_count"),obj.optBoolean("is_heading"),obj.optString("excerpt")) }
    fun setTxtHeading(line: Int, selected: Boolean) { txtExcluded=if(selected)txtExcluded-line else txtExcluded+line; refreshTxtPreviews() }
    fun setTxtRule(regex: String = txtImportRegex, heuristic: Boolean = txtImportHeuristic) {
        txtImportRegex=regex;txtImportHeuristic=heuristic;txtExcluded=emptySet();txtIncluded=emptySet();txtImportPreviews=emptyList();refreshTxtPreviews()
    }
    fun searchTxtSource() {
        val path=txtImportPath?:return;val revision=++txtSearchRevision;val request=txtRequest("lines").put("query",txtSourceQuery).toString()
        viewModelScope.launch(Dispatchers.IO) { try {
            val envelope=org.json.JSONObject(RustBridge.readingTools(request)?:error("Invalid TXT response"));if(!envelope.optBoolean("ok"))error(envelope.optString("error"))
            val rows=txtRows(envelope.getJSONArray("result")).take(80)
            withContext(Dispatchers.Main){if(revision==txtSearchRevision && txtImportPath==path && showTxtImport)txtSourceHits=rows}
        }catch(e:kotlinx.coroutines.CancellationException){throw e}catch(e:Exception){withContext(Dispatchers.Main){if(revision==txtSearchRevision)txtImportError=e.message}} }
    }
    fun addTxtHeading(line: Int){txtIncluded=txtIncluded+line;txtExcluded=txtExcluded-line;refreshTxtPreviews()}

    fun refreshTxtPreviews() {
        val path = txtImportPath ?: return
        val revision = ++txtPreviewRevision
        txtAnalyzing=true
        val request=txtRequest("analyze").toString()
        txtPreviewJob?.cancel()
        txtPreviewJob=viewModelScope.launch(Dispatchers.IO) {
            delay(250)
            try {
                val envelope=org.json.JSONObject(txtWorkLock.withLock {RustBridge.readingTools(request)?:error("Invalid TXT response")})
                if(!envelope.optBoolean("ok"))error(envelope.optString("error"))
                val analysis=envelope.getJSONObject("result");val rows=txtRows(analysis.getJSONArray("chapters"))
                withContext(Dispatchers.Main) { if(revision==txtPreviewRevision && path==txtImportPath && showTxtImport){
                    val unchecked=txtImportPreviews.filter {it.lineStart in txtExcluded}
                    txtHeadingCount=analysis.getInt("heading_count");txtChapterCount=analysis.getInt("chapter_count")
                    txtImportPreviews=(unchecked+rows).distinctBy {it.lineStart}.sortedBy {it.lineStart};txtImportError=null;txtAnalyzing=false
                } }
            }catch(e:kotlinx.coroutines.CancellationException){throw e}catch(e:Exception){withContext(Dispatchers.Main){if(revision==txtPreviewRevision && path==txtImportPath && showTxtImport){txtImportError=e.message;txtAnalyzing=false}}}
        }
    }

    fun convertTxtToEpub() {
        val path = txtImportPath ?: return
        if(txtAnalyzing || txtImportError!=null)return
        val request=txtRequest("convert").put("directory",booksDir).put("title",txtImportTitle).put("author",txtImportAuthor).toString()
        txtImportConverting = true
        txtImportError = null
        viewModelScope.launch(Dispatchers.IO) {
            val result = RustBridge.readingTools(request)
            withContext(Dispatchers.Main) {
                txtImportConverting = false
                if (result == null) {
                    txtImportError = I18n.t("common.error_conversion_failed")
                    return@withContext
                }
                try {
                    val obj = org.json.JSONObject(result)
                    if (!obj.optBoolean("ok")) {
                        txtImportError = obj.optString("error")
                    } else {
                        val epubPath = obj.getJSONObject("result").getString("epubPath")
                        showTxtImport = false
                        txtImportPath = null
                        viewModelScope.launch(Dispatchers.IO){discardOwnedTxt(path)}
                        openFromPath(epubPath, 0)
                    }
                } catch (e: Exception) {
                    txtImportError = e.message
                }
            }
        }
    }

    fun dismissTxtImport() {
        if(txtImportConverting)return
        ++txtPreviewRevision;++txtSearchRevision;txtPreviewJob?.cancel();txtAnalyzing=false
        showTxtImport = false
        txtImportPath?.let{path->viewModelScope.launch(Dispatchers.IO){discardOwnedTxt(path)}}
        txtImportPath = null
    }

    private fun discardOwnedTxt(path:String){
        runCatching{
            val file=File(path)
            if(file.name.startsWith("import_") && file.extension.equals("txt",true) && file.canonicalFile.parentFile==File(context.cacheDir,"imports").canonicalFile){file.delete()}
        }
    }

    // ── CSC Contribution ──

    /** Check if user should be prompted to contribute correction data. */
    fun checkContributionPrompt() {
        if (contributePrompted || contributeDismissed) return
        val uri = currentBookUri ?: return
        val bookId = library.books.firstOrNull { it.uri == uri }?.id ?: return

        viewModelScope.launch(Dispatchers.IO) {
            val count = RustBridge.getCscCorrectionCount(dataDir, bookId)
            if (count >= 10) {
                withContext(Dispatchers.Main) {
                    contributePrompted = true
                    showContributeDialog = true
                }
            }
        }
    }

    /** Collect samples and prepare for contribution dialog. */
    fun prepareContribution() {
        val uri = currentBookUri ?: return
        val bookId = library.books.firstOrNull { it.uri == uri }?.id ?: return

        viewModelScope.launch(Dispatchers.IO) {
            val jsonl = RustBridge.collectCscSamples(dataDir, uri, bookId)
            withContext(Dispatchers.Main) {
                if (jsonl.isNullOrBlank()) {
                    contributeSamples = ""
                    contributeSampleCount = 0
                } else {
                    contributeSamples = jsonl
                    contributeSampleCount = jsonl.lines().count { it.isNotBlank() }
                }
            }
        }
    }

    /** Start GitHub Device Flow OAuth. */
    fun startGitHubLogin(privateSync: Boolean = false, enableAfterLogin: Boolean = privateSync) {
        if (githubLoginJob?.isActive == true) return
        githubAuthPolling = true
        githubAuthForSync = privateSync
        githubSyncStatus = ""
        githubLoginJob = viewModelScope.launch {
            try {
                val device = com.zhongbai233.epub.reader.util.GitHubContributor.requestDeviceCode(privateSync).getOrThrow()
                githubDeviceCode = device.deviceCode
                githubUserCode = device.userCode
                githubVerificationUri = device.verificationUri
                when (val auth = com.zhongbai233.epub.reader.util.GitHubContributor.pollForToken(device)) {
                    is com.zhongbai233.epub.reader.util.GitHubContributor.AuthResult.Success -> {
                        // Keep credential persistence and session replacement in one main-thread
                        // step: cancellation cannot save a new token while retaining the old user.
                        GitHubTokenStore.save(context, auth.token).getOrThrow()
                        cancelGithubSyncWork()
                        githubSyncLastSuccess = null
                        githubRepoAuthorized = auth.repoAuthorized
                        bookSharing.cancelAuthenticatedWork()
                        githubToken = auth.token
                        githubUsername = auth.username
                        prefs.edit().putString(PrefKeys.GITHUB_USERNAME, auth.username).putBoolean("github_repo_authorized", auth.repoAuthorized).apply()
                        githubSyncNeedsAuth = githubSyncEnabled && !auth.repoAuthorized
                        githubSyncAttemptAt = 0L
                        githubSyncRetryMs = 300_000L
                        githubSyncStatus = ""
                        githubSyncFailures = emptyList()
                        githubSyncThrottled = false
                        if (!enableAfterLogin) {
                            githubSyncEnabled = false
                            prefs.edit().putBoolean("github_sync_enabled", false).apply()
                        }
                        if (enableAfterLogin && auth.repoAuthorized) {
                            updateGithubSyncEnabled(true)
                            syncGithubNow()
                        } else if (privateSync && !auth.repoAuthorized) {
                            githubSyncNeedsAuth = true
                            githubSyncStatus = I18n.t("account.scope_missing")
                        }
                    }
                    is com.zhongbai233.epub.reader.util.GitHubContributor.AuthResult.Error -> {
                        contributeStatus = auth.message
                        githubSyncStatus = auth.message
                    }
                }
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (error: Exception) {
                contributeStatus = error.message ?: I18n.t("sync.error_github_login_failed")
                githubSyncStatus = contributeStatus
            } finally {
                githubAuthPolling = false
                githubAuthForSync = false
                githubDeviceCode = null
                githubUserCode = null
                githubVerificationUri = null
            }
        }
    }

    fun cancelGitHubLogin() { githubLoginJob?.cancel() }

    private fun cancelGithubSyncWork() {
        githubSyncGeneration++
        runCatching { RustBridge.cancelGithubSync() }
        githubMeterJob?.cancel(); githubMeterJob=null; githubSyncMeter=null
        githubSyncJob?.cancel()
        githubSyncRunning = false
        githubSyncOffer = null
    }

    fun setGithubSyncForeground(foreground: Boolean) {
        githubSyncForeground = foreground
        if (!foreground) cancelGithubSyncWork()
        else if (githubSyncRetryMs == 300_000L) githubSyncAttemptAt = 0L
    }

    fun updateGithubSyncContributions(value: Boolean) {
        if (githubSyncRunning) return
        githubSyncContributions = value
        prefs.edit().putBoolean("github_sync_contributions", value).apply()
    }

    fun updateGithubSyncEnabled(enabled: Boolean) {
        githubSyncEnabled = enabled
        prefs.edit().putBoolean("github_sync_enabled", enabled).apply()
        if (!enabled) { cancelGithubSyncWork(); if (githubAuthPolling) cancelGitHubLogin() }
        else { githubSyncNeedsAuth = githubToken == null; githubSyncAttemptAt = 0L }
    }

    fun prepareGithubSync() {
        if (githubToken != null && githubRepoAuthorized && !githubSyncNeedsAuth) {
            updateGithubSyncEnabled(true)
            syncGithubNow()
        } else startGitHubLogin(privateSync = true)
    }

    fun logoutGitHub() {
        bookSharing.cancelAuthenticatedWork()
        updateGithubSyncEnabled(false)
        cancelGitHubLogin()
        GitHubTokenStore.delete(context)
        githubToken = null
        githubUsername = null
        githubSyncFailures = emptyList()
        githubSyncThrottled = false
        githubRepoAuthorized = false
        githubSyncLastSuccess = null
        prefs.edit().putBoolean("github_repo_authorized", false).apply()
        prefs.edit().remove(PrefKeys.GITHUB_USERNAME).apply()
        githubSyncStatus = ""
    }

    fun syncGithubNow() = syncGithubStages(false)
    fun retryGithubFiles() = syncGithubStages(true)
    private fun syncGithubStages(retryFailed: Boolean) {
        if (!githubSyncEnabled || !githubSyncForeground || githubSyncRunning || backupBusy) return
        val now = android.os.SystemClock.elapsedRealtime()
        if (githubSyncThrottled && now - githubSyncAttemptAt < githubSyncRetryMs) return
        val token = githubToken ?: run { githubSyncNeedsAuth = true; return }
        val owner = githubUsername ?: run { githubSyncNeedsAuth = true; return }
        val generation = githubSyncGeneration
        val progressRevision=localProgressRevision
        val nativeEpoch = RustBridge.githubSyncEpoch()
        val countContributions = githubSyncContributions
        githubSyncRunning = true
        githubSyncMeter=null
        githubMeterJob=viewModelScope.launch {
            while (isActive && generation == githubSyncGeneration) {
                val status=withContext(Dispatchers.IO) { runCatching { JSONObject(RustBridge.githubSyncStatus(dataDir) ?: "{}") }.getOrNull() }
                if (generation != githubSyncGeneration) return@launch
                githubSyncMeter=status
                delay(200)
            }
        }
        githubSyncStatus = I18n.t("cloud.syncing")
        githubSyncJob = viewModelScope.launch {
            libraryStartup?.join()
            try {
                for (progressOnly in listOf(true, false)) {
                    if (!canApplyGithubSync(generation, githubSyncGeneration, owner, githubUsername, githubSyncEnabled, githubSyncForeground)) return@launch
                    githubSyncStatus = I18n.t(if (progressOnly) "cloud.progress_syncing" else "cloud.books_syncing")
                    val response = withContext(Dispatchers.IO) { RustBridge.runGithubSync(dataDir, token, owner, nativeEpoch, countContributions, progressOnly, retryFailed) }
                    if (!canApplyGithubSync(generation, githubSyncGeneration, owner, githubUsername, githubSyncEnabled, githubSyncForeground)) return@launch
                    val decoded = JSONObject(response ?: throw IllegalStateException("Missing native sync result"))
                    if (!decoded.getBoolean("ok")) throw IllegalStateException(decoded.getString("error"))
                    while (isLoading) { delay(100) }
                    if (!canApplyGithubSync(generation, githubSyncGeneration, owner, githubUsername, githubSyncEnabled, githubSyncForeground)) return@launch
                    // Apply to the live library only after generation/account checks on the UI thread.
                    val applied = JSONObject(RustBridge.applyGithubSync(dataDir, decoded.getJSONObject("result").toString(), currentBookUri ?: "") ?: throw IllegalStateException("Missing native apply result"))
                    if (!applied.getBoolean("ok")) throw IllegalStateException(applied.getString("error"))
                    val result = applied.getJSONObject("result")
                    library.reload()
                    loadLibrary()
                    if(currentBookUri != null && library.books.none {it.uri==currentBookUri}) closeBook()
                    val updated = result.getJSONArray("updated")
                    prefs.edit().apply {
                        for (index in 0 until updated.length()) {
                            val update = updated.getJSONObject(index)
                            val id = library.books.firstOrNull { it.uri == update.getString("path") }?.id ?: continue
                            putInt("book_ch_$id", update.getInt("chapter"))
                        }
                    }.apply()
                    if (progressOnly) uploadedProgressRevision=progressRevision
                    githubSyncOffer = result.optJSONObject("offer")
                    if (githubSyncOffer != null) applyGithubProgress()
                    githubSyncNeedsAuth = false
                    githubRepoAuthorized = true
                    prefs.edit().putBoolean("github_repo_authorized", true).apply()
                    githubSyncLastSuccess = System.currentTimeMillis()
                    githubSyncRetryMs = if (result.isNull("next_retry_after")) 300_000L else result.getLong("next_retry_after").coerceIn(1, 86_400) * 1000
                    githubSyncThrottled = false
                    githubSyncStatus = "${I18n.t("cloud.done")} (↑ ${result.getInt("uploaded")}, ↓ ${result.getInt("downloaded")}, ${I18n.t("cloud.skipped")}: ${result.getInt("skipped")})"
                    if (!progressOnly) {
                        val failures = result.optJSONArray("failed")
                        githubSyncFailures = if (failures == null) emptyList() else (0 until failures.length()).map { failures.getJSONObject(it) }
                        if (githubSyncFailures.isNotEmpty()) githubSyncStatus += " · ${I18n.tf1("cloud.files_failed", githubSyncFailures.size.toString())}"
                    }
                    currentBookId?.let { reloadBookConfig(dataDir, it) }
                    if (!result.isNull("terminal_error")) throw IllegalStateException(result.getString("terminal_error"))
                    if (!result.isNull("annotation_error")) throw IllegalStateException(result.getString("annotation_error"))
                }
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (error: Exception) {
                val message = error.message ?: I18n.t("cloud.failed")
                githubSyncThrottled = message.startsWith("RETRY_AFTER:")
                githubSyncNeedsAuth = message.startsWith("REAUTHORIZE:") || message.contains("login expired")
                if (githubSyncNeedsAuth) {
                    githubRepoAuthorized = false
                    prefs.edit().putBoolean("github_repo_authorized", false).apply()
                }
                githubSyncRetryMs = if (message.startsWith("RETRY_AFTER:")) (message.substringAfter(":").substringBefore(":").toLongOrNull() ?: 900L).coerceIn(300, 86_400) * 1000 else 300_000L
                githubSyncStatus = if (githubSyncNeedsAuth) I18n.t("cloud.need_auth") else "${I18n.t("cloud.failed")}: ${if (message.startsWith("RETRY_AFTER:")) message.substringAfter(":").substringAfter(":") else message}"
            } finally {
                if (generation == githubSyncGeneration) {
                    githubMeterJob?.cancel(); githubMeterJob=null; githubSyncMeter=null
                    githubSyncRunning = false
                    githubSyncAttemptAt = android.os.SystemClock.elapsedRealtime()
                }
            }
        }
    }

    fun applyGithubProgress() {
        val offer = githubSyncOffer ?: return
        val book = currentBook ?: return
        val uri = currentBookUri ?: return
        val hash = RustBridge.fileHash(uri) ?: return
        if (offer.optString("book_hash") != hash) return
        val chapter = offer.getInt("chapter")
        if (chapter !in book.chapters.indices) return
        goToPosition(chapter, offer.optInt("block", 0), offer.optInt("char_offset", 0), remote = true, scalarOffset = true)
        githubSyncOffer = null
    }

    fun updateContributionSamples(value:String){if(contributeInProgress)return;contributeSamples=value;contributeSampleCount=value.lineSequence().count{it.isNotBlank()}}
    /** Submit collected samples to GitHub. */
    fun submitContribution() {
        val token = githubToken ?: return
        val username = githubUsername ?: return
        val jsonl = contributeSamples
        if (jsonl.isBlank()) return

        contributeInProgress = true
        contributeStatus = ""
        contributePrUrl = null

        viewModelScope.launch {
            val result = com.zhongbai233.epub.reader.util.GitHubContributor.submitContribution(token, username, jsonl)
            contributeInProgress = false
            when (result) {
                is com.zhongbai233.epub.reader.util.GitHubContributor.ContributeResult.Success -> {
                    contributePrUrl = result.prUrl
                    contributeStatus = ""
                }
                is com.zhongbai233.epub.reader.util.GitHubContributor.ContributeResult.Error -> {
                    contributeStatus = result.message
                }
            }
        }
    }

    /** Dismiss the contribute dialog (don't ask again this session). */
    fun dismissContributeDialog() {
        showContributeDialog = false
        contributeDismissed = true
        contributeStatus = ""
        contributePrUrl = null
    }

    // ---- 书签 / 高亮 / 标注 ----

    fun loadBookConfig() {
        val bookId = currentBookId ?: return
        val dataDir = getApplication<Application>().filesDir.absolutePath
        viewModelScope.launch(Dispatchers.IO) {
            val json = RustBridge.getBookConfig(dataDir, bookId)
            if (json != null) {
                try {
                    val cfg = jsonParser.decodeFromString<FullBookConfig>(json)
                    withContext(Dispatchers.Main) {
                        bookConfig = cfg
                        updateBookmarkState()
                    }
                } catch (_: Exception) { }
            }
        }
    }

    private fun updateBookmarkState() {
        val ch = currentChapter
        isChapterBookmarked = bookConfig?.bookmarks?.any { it.chapter == ch } ?: false
    }

    private val storageFailureHandler = CoroutineExceptionHandler { _, failure ->
        viewModelScope.launch(Dispatchers.Main) {
            addLog("Storage operation failed: ${failure.message}")
            errorMessage = I18n.t("reader.error_save_book_failed")
            updateBookmarkState()
        }
    }

    fun toggleBookmark(): Boolean {
        val bookId = currentBookId ?: return false
        val dataDir = getApplication<Application>().filesDir.absolutePath
        val chapter = currentChapter
        viewModelScope.launch(Dispatchers.IO + storageFailureHandler) {
            val result = RustBridge.toggleBookmark(dataDir, bookId, chapter)
            // reload config
            val json = RustBridge.getBookConfig(dataDir, bookId)
            if (json != null) {
                try {
                    val cfg = jsonParser.decodeFromString<FullBookConfig>(json)
                    withContext(Dispatchers.Main) {
                        bookConfig = cfg
                        updateBookmarkState()
                    }
                } catch (_: Exception) { }
            }
        }
        // return the toggled state optimistically
        val was = isChapterBookmarked
        isChapterBookmarked = !was
        return !was
    }

    fun removeBookmarkForChapter(chapter: Int) {
        val bookId = currentBookId ?: return
        val dataDir = getApplication<Application>().filesDir.absolutePath
        viewModelScope.launch(Dispatchers.IO + storageFailureHandler) {
            // toggleBookmark will remove if present
            RustBridge.toggleBookmark(dataDir, bookId, chapter)
            reloadBookConfig(dataDir, bookId)
        }
    }

    fun addHighlight(
        chapter: Int,
        startBlock: Int,
        startOffset: Int,
        endBlock: Int,
        endOffset: Int,
        color: String = "Yellow"
    ) {
        val bookId = currentBookId ?: return
        val dataDir = getApplication<Application>().filesDir.absolutePath
        val publication = currentBook ?: return
        viewModelScope.launch(Dispatchers.IO + storageFailureHandler) {
            val source = publication.chapters[chapter]
            val start = unicodeScalarOffset(readerSourceText(source.blocks.getOrNull(startBlock)), startOffset)
            val end = unicodeScalarOffset(readerSourceText(source.blocks.getOrNull(endBlock)), endOffset)
            val payload = org.json.JSONObject().put("chapter", chapter).put("start_block", startBlock)
                .put("start_offset", start).put("end_block", endBlock).put("end_offset", end).put("color", color)
            checkNotNull(RustBridge.addHighlight(dataDir, bookId, payload.toString())) {"Cannot save highlight"}
            reloadBookConfig(dataDir, bookId)
        }
    }

    fun removeHighlight(highlightId: String) {
        val bookId = currentBookId ?: return
        val dataDir = getApplication<Application>().filesDir.absolutePath
        viewModelScope.launch(Dispatchers.IO + storageFailureHandler) {
            RustBridge.removeHighlight(dataDir, bookId, highlightId)
            reloadBookConfig(dataDir, bookId)
        }
    }

    fun saveNote(highlightId: String, content: String, originalNote:String?=bookConfig?.notes?.firstOrNull {it.highlightId==highlightId}?.content, completed:(Boolean)->Unit={}) {
        val bookId=currentBookId ?: return
        val dataDir=getApplication<Application>().filesDir.absolutePath
        viewModelScope.launch {
            try {
                withContext(Dispatchers.IO) {
                    val request=JSONObject().put("action","note").put("id",bookId).put("highlight_id",highlightId).put("content",content).put("original_note",originalNote ?: JSONObject.NULL)
                    val envelope=JSONObject(RustBridge.annotationTools(dataDir,request.toString()) ?: error("Missing note result"))
                    check(envelope.getBoolean("ok")) {envelope.optString("error")}
                }
                reloadBookConfig(dataDir,bookId);completed(true)
            } catch(cancelled:CancellationException) {throw cancelled}
            catch(error:Exception) {errorMessage=I18n.t("reader.error_save_book_failed");completed(false)}
        }
    }

    private suspend fun reloadBookConfig(dataDir: String, bookId: String) {
        val json = RustBridge.getBookConfig(dataDir, bookId) ?: return
        try {
            val cfg = jsonParser.decodeFromString<FullBookConfig>(json)
            withContext(Dispatchers.Main) {
                if(currentBookId==bookId) {bookConfig = cfg;updateBookmarkState()}
            }
        } catch (_: Exception) { }
    }

    override fun onCleared() {
        libraryRenderObserver = null
        cancelPendingBookOpen()
        bookSharing.close()
        activityTracker.close()
        cancelGithubSyncWork()
        cancelGitHubLogin()
        super.onCleared()
        ttsManager.destroy()
        cscEngine.release()
        discoveryPollingJob?.cancel()
        bonjour.close()
        RustBridge.stopDiscoveryListener()
        discoveryStarted = false
        autoSyncInProgress = false
        autoSyncLastAttemptAt.clear()
        try { multicastLock?.release() } catch (_: Exception) { }
        multicastLock = null
    }
}
