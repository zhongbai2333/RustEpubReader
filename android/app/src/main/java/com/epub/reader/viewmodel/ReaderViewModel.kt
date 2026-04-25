package com.zhongbai233.epub.reader.viewmodel

import android.app.Application
import android.net.Uri
import android.net.wifi.WifiManager
import android.content.Context
import android.widget.Toast
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
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
import java.util.concurrent.ConcurrentHashMap
import android.util.Base64

data class TxtChapterPreview(val title: String, val lineStart: Int, val charCount: Int)

class ReaderViewModel(application: Application) : AndroidViewModel(application) {

    private val context get() = getApplication<Application>()
    private val library = Library(context)
    private var currentBookUri: String? = null
    val currentBookId: String?
        get() = currentBookUri?.let { uri -> library.books.firstOrNull { it.uri == uri }?.id }
    private val prefs by lazy {
        context.getSharedPreferences("reader_settings", Context.MODE_PRIVATE)
    }
    private var multicastLock: WifiManager.MulticastLock? = null

    // ---- ���״̬ ----
    val books = mutableStateListOf<BookEntry>()
    val coverCache = mutableStateMapOf<String, ByteArray?>()

    // ---- �Ķ���״̬ ----
    var currentBook by mutableStateOf<EpubBook?>(null)
        private set
    var currentChapter by mutableIntStateOf(0)
        private set
    var currentPage by mutableIntStateOf(0)
        private set
    var previousChapter by mutableStateOf<Int?>(null)
        private set

    // ---- ���� ----
    var fontSize by mutableFloatStateOf(18f)
        private set
    var isDarkMode by mutableStateOf(false)
    var isScrollMode by mutableStateOf(false)
    var readerBgColorIndex by mutableIntStateOf(0)
        private set
    var readerCustomBgColorArgb by mutableIntStateOf(0xFFF5F0E8.toInt())
        private set
    var readerFontColorIndex by mutableIntStateOf(0)
        private set
    var readerCustomFontColorArgb by mutableIntStateOf(0xFF1A1A1A.toInt())
        private set
    var readerFontFamily by mutableStateOf("Sans")
        private set
    var readerPageAnimation by mutableStateOf("Slide")
        private set
    var readerBgImageUri by mutableStateOf<String?>(null)
        private set
    var readerBgImageAlpha by mutableFloatStateOf(0.22f)
        private set
    var readerLanguage by mutableStateOf("auto")
        private set
    var systemFonts by mutableStateOf<List<FontItem>>(emptyList())
        private set

    // ---- 排版设置 ----
    var lineSpacing by mutableFloatStateOf(1.5f)
        private set
    var paraSpacing by mutableFloatStateOf(0.5f)
        private set
    var textIndent by mutableIntStateOf(2)
        private set

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
    var errorMessage by mutableStateOf<String?>(null)
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
        if (autoStartSharing) {
            try { startSharingServer() } catch (_: Exception) { }
        }
        loadLibrary()
        scanBooksDir()
        try { loadPairedDevices() } catch (_: Exception) { }
        try { startDiscovery() } catch (_: Exception) { }
        viewModelScope.launch(Dispatchers.IO) {
            val fonts = FontDiscovery.discoverFonts()
            withContext(Dispatchers.Main) {
                systemFonts = fonts
            }
        }
        // �Զ��ָ��ϴ��Ķ�
        val lastUri = prefs.getString(PrefKeys.LAST_BOOK_URI, null)
        if (lastUri != null) {
            val lastChapter = prefs.getInt(PrefKeys.LAST_BOOK_CHAPTER, 0)
            openFromPath(lastUri, lastChapter)
        }

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
        const val LAST_BOOK_URI = "last_book_uri"
        const val LAST_BOOK_CHAPTER = "last_book_chapter"
        const val AUTO_START_SHARING = "auto_start_sharing"
        const val GITHUB_TOKEN = "github_token"
        const val GITHUB_USERNAME = "github_username"
        const val LINE_SPACING = "line_spacing"
        const val PARA_SPACING = "para_spacing"
        const val TEXT_INDENT = "text_indent"
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
        fontSize = prefs.getFloat(PrefKeys.FONT_SIZE, 18f).coerceIn(12f, 40f)
        isDarkMode = prefs.getBoolean(PrefKeys.DARK_MODE, false)
        isScrollMode = prefs.getBoolean(PrefKeys.SCROLL_MODE, false)

        readerBgColorIndex = prefs.getInt(PrefKeys.BG_COLOR_INDEX, 0).coerceAtLeast(0)
        readerCustomBgColorArgb = prefs.getInt(PrefKeys.CUSTOM_BG_COLOR, 0xFFF5F0E8.toInt())

        readerFontColorIndex = prefs.getInt(PrefKeys.FONT_COLOR_INDEX, 0).coerceAtLeast(0)
        readerCustomFontColorArgb = prefs.getInt(PrefKeys.CUSTOM_FONT_COLOR, 0xFF1A1A1A.toInt())

        readerFontFamily = prefs.getString(PrefKeys.FONT_FAMILY, "Sans") ?: "Sans"
        readerPageAnimation = prefs.getString(PrefKeys.PAGE_ANIMATION, "Slide") ?: "Slide"
        readerBgImageUri = prefs.getString(PrefKeys.BG_IMAGE_URI, null)
        readerBgImageAlpha = prefs.getFloat(PrefKeys.BG_IMAGE_ALPHA, 0.22f).coerceIn(0f, 1f)
        readerLanguage = prefs.getString(PrefKeys.LANGUAGE, "auto") ?: "auto"
        I18n.setLanguage(readerLanguage)
        autoStartSharing = prefs.getBoolean(PrefKeys.AUTO_START_SHARING, false)
        githubToken = prefs.getString(PrefKeys.GITHUB_TOKEN, null)
        githubUsername = prefs.getString(PrefKeys.GITHUB_USERNAME, null)
        lineSpacing = prefs.getFloat(PrefKeys.LINE_SPACING, 1.5f).coerceIn(1.0f, 3.0f)
        paraSpacing = prefs.getFloat(PrefKeys.PARA_SPACING, 0.5f).coerceIn(0.0f, 2.0f)
        textIndent = prefs.getInt(PrefKeys.TEXT_INDENT, 2).coerceIn(0, 4)
        translateApiUrl = prefs.getString(PrefKeys.TRANSLATE_API_URL, "") ?: ""
        translateApiKey = prefs.getString(PrefKeys.TRANSLATE_API_KEY, "") ?: ""
        dictionaryApiUrl = prefs.getString(PrefKeys.DICTIONARY_API_URL, "") ?: ""
        dictionaryApiKey = prefs.getString(PrefKeys.DICTIONARY_API_KEY, "") ?: ""
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
            .putBoolean(PrefKeys.AUTO_START_SHARING, autoStartSharing)
            .putFloat(PrefKeys.LINE_SPACING, lineSpacing)
            .putFloat(PrefKeys.PARA_SPACING, paraSpacing)
            .putInt(PrefKeys.TEXT_INDENT, textIndent)
            .putString(PrefKeys.TRANSLATE_API_URL, translateApiUrl)
            .putString(PrefKeys.TRANSLATE_API_KEY, translateApiKey)
            .putString(PrefKeys.DICTIONARY_API_URL, dictionaryApiUrl)
            .putString(PrefKeys.DICTIONARY_API_KEY, dictionaryApiKey)
            .putString(PrefKeys.TTS_VOICE_NAME, ttsVoiceName)
            .putInt(PrefKeys.TTS_RATE, ttsRate)
            .putInt(PrefKeys.TTS_VOLUME, ttsVolume)
            .putString(PrefKeys.CSC_MODE, cscMode)
            .putString(PrefKeys.CSC_THRESHOLD, cscThreshold)
            .apply()
    }

    // ---- ������ ----

    private fun loadLibrary() {
        books.clear()
        books.addAll(library.sortedIndicesByRecent().map { library.books[it] })
        preloadCoversFromLibrary()
    }

    /** Public refresh: scan books dir for new files + apply progress + reload library UI */
    fun refreshLibrary() {
        scanBooksDir()
        applySyncedProgress()
        loadLibrary()
    }

    /** Read synced progress from PeerStore and apply chapter numbers to library entries */
    private fun applySyncedProgress() {
        val progressJson = RustBridge.getSyncedProgress(dataDir) ?: return
        try {
            val arr = org.json.JSONArray(progressJson)
            if (arr.length() == 0) return
            data class SyncedProgress(
                val chapter: Int,
                val chapterTitle: String?,
                val timestamp: Long,
            )

            // Build hash -> progress map from synced progress
            val progressMap = mutableMapOf<String, SyncedProgress>()
            for (i in 0 until arr.length()) {
                val obj = arr.getJSONObject(i)
                val hash = obj.getString("book_hash")
                val chapter = obj.getInt("chapter")
                val chapterTitle = obj.optString("chapter_title", "")
                    .takeIf { it.isNotBlank() }
                val ts = obj.optLong("timestamp", 0)
                progressMap[hash] = SyncedProgress(chapter, chapterTitle, ts)
            }
            // Match library books by hash
            var updated = 0
            for (book in library.books) {
                val hash = RustBridge.fileHash(book.uri) ?: continue
                val remote = progressMap[hash] ?: continue
                val shouldUpdate =
                    book.lastChapter != remote.chapter ||
                        (book.lastChapter == remote.chapter &&
                            remote.chapterTitle != null &&
                            remote.chapterTitle != book.lastChapterTitle)

                if (shouldUpdate) {
                    library.updateChapter(book.uri, remote.chapter, remote.chapterTitle)
                    updated++
                }
            }
            if (updated > 0) {
                addLog("applySyncedProgress: updated $updated book(s)")
            }
        } catch (e: Exception) {
            addLog("applySyncedProgress error: ${e.message}")
        }
    }

    private fun preloadCoversFromLibrary() {
        val entries = books.toList()
        viewModelScope.launch(Dispatchers.IO) {
            entries.forEach { entry ->
                if (coverCache.containsKey(entry.uri)) return@forEach
                val file = File(entry.uri)
                if (!file.exists()) return@forEach
                runCatching {
                    val coverBase64 = RustBridge.getCover(entry.uri)
                    if (coverBase64 != null) {
                        val decoded = Base64.decode(coverBase64, Base64.DEFAULT)
                        withContext(Dispatchers.Main) {
                            coverCache[entry.uri] = decoded
                        }
                    }
                }
            }
        }
    }

    fun removeBook(entry: BookEntry) {
        removeBookByUri(entry.uri)
    }

    fun removeBookByUri(uri: String) {
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
            loadLibrary()
        }
    }

    // ---- ���鼮 ----

    fun openFromUri(uri: Uri) {
        viewModelScope.launch {
            isLoading = true
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
            } catch (e: Exception) {
                errorMessage = "打开失败: ${e.message}"
            } finally {
                if (!isTxt) {
                    tempFile?.let { file ->
                        if (file.exists() && file.parentFile?.name == "imports") {
                            runCatching { file.delete() }
                        }
                    }
                }
                isLoading = false
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

    fun openFromPath(filePath: String, chapter: Int = 0) {
        android.util.Log.d("READER-RESUME", "openFromPath path=$filePath chapter=$chapter")
        viewModelScope.launch {
            isLoading = true
            errorMessage = null
            try {
                val file = File(filePath)
                if (!file.exists()) {
                    errorMessage = "�ļ�������: $filePath"
                    return@launch
                }
                val resolvedChapter = resolveStartChapter(filePath, chapter)
                parseAndOpen(file, resolvedChapter)
            } catch (e: Exception) {
                errorMessage = "��ʧ��: ${e.message}"
            } finally {
                isLoading = false
            }
        }
    }

    private fun resolveStartChapter(filePath: String, chapter: Int): Int {
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

        val resolved = maxOf(requested, fromLibrary, fromPrefs, fromConfig)
        android.util.Log.d("READER-RESUME",
            "resolveStartChapter path=$filePath requested=$requested " +
                "fromLibrary=$fromLibrary fromPrefs=$fromPrefs fromConfig=$fromConfig => $resolved")
        return resolved
    }

    private suspend fun parseAndOpen(file: File, startChapter: Int = 0) {
        val inputPath = file.absolutePath
        val managedEntry = withContext(Dispatchers.IO) {
            val fallbackTitle = file.nameWithoutExtension.ifBlank { "Untitled" }
            library.addOrUpdate(fallbackTitle, inputPath, startChapter.coerceAtLeast(0))
        }
        val managedPath = managedEntry.uri

        val book = withContext(Dispatchers.IO) {
            val metadataJson = RustBridge.openBook(managedPath)
                ?: throw Exception("EPUB ����ʧ�� (Rust Bridge)")
            
            val metadata = jsonParser.decodeFromString<BookMetadataDto>(metadataJson)

            // Parse review chapter info from DTO with safe fallback
            val reviewIndices = try {
                metadata.reviewChapterIndices.toMutableSet()
            } catch (e: Exception) {
                android.util.Log.e("ReaderViewModel", "解析段评数据失败", e)
                mutableSetOf<Int>()
            }
            val chapterReviewMap = try {
                metadata.chapterReviews.associate { it.main to it.review }.toMutableMap()
            } catch (e: Exception) {
                android.util.Log.e("ReaderViewModel", "解析段评数据失败", e)
                mutableMapOf<Int, Int>()
            }
            withContext(Dispatchers.Main) {
                reviewChapterIndices = reviewIndices
                chapterReviews = chapterReviewMap
                // Reset review panel state so a stale panel never persists across book switches
                showReviewPanel = false
                reviewPanelChapter = null
            }
            
            val lazyChapters = object : AbstractList<Chapter>() {
                val cache = ConcurrentHashMap<Int, Chapter>()
                override val size: Int get() = metadata.chapterCount
                
                override fun get(index: Int): Chapter {
                    if (index !in 0 until size) throw IndexOutOfBoundsException()
                    return cache.getOrPut(index) {
                        try {
                            val json = RustBridge.getChapter(managedPath, index)
                                ?: return@getOrPut Chapter("Error", emptyList())
                            val dto = jsonParser.decodeFromString<ChapterDto>(json)
                            Chapter(title = dto.title, blocks = dto.blocks, sourceHref = dto.sourceHref)
                        } catch (e: Exception) {
                            e.printStackTrace()
                            Chapter("Error", emptyList())
                        }
                    }
                }
            }

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
                coverData = coverBytes
            )
        } ?: throw Exception("EPUB ����ʧ��")

        currentBook = book
        currentBookUri = managedPath
        currentChapter = startChapter.coerceIn(0, (book.chapters.size - 1).coerceAtLeast(0))
        currentPage = 0
        android.util.Log.d("READER-RESUME",
            "parseAndOpen startChapter=$startChapter chapterCount=${book.chapters.size} => currentChapter=$currentChapter")

        val chapterTitle = try {
            book.chapters[currentChapter].title
        } catch (_: Exception) { null }

        val persistedEntry = library.addOrUpdate(book.title, managedPath, currentChapter, chapterTitle)
        if (book.coverData != null) {
            coverCache[persistedEntry.uri] = book.coverData
        }
        currentBookUri = persistedEntry.uri
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
    }

    private fun isTxtUri(uri: Uri): Boolean {
        val mime = context.contentResolver.getType(uri)
        if (mime == "text/plain") return true
        val path = uri.path ?: uri.lastPathSegment ?: ""
        return path.endsWith(".txt", ignoreCase = true)
    }

    private suspend fun copyUriToTempFile(uri: Uri, ext: String = "epub"): File = withContext(Dispatchers.IO) {
        val importDir = File(context.cacheDir, "imports").also { it.mkdirs() }
        val dest = File(importDir, "import_${System.currentTimeMillis()}.$ext")
        context.contentResolver.openInputStream(uri)?.use { input ->
            FileOutputStream(dest).use { output ->
                input.copyTo(output)
            }
        } ?: throw Exception("无法读取文件")
        dest
    }

    fun goToChapter(index: Int) {
        val book = currentBook ?: return
        if (reviewChapterIndices.contains(index)) {
            openReviewPanel(index)
            return
        }
        val target = index.coerceIn(0, book.chapters.size - 1)
        if (target != currentChapter) {
            previousChapter = currentChapter
        }
        currentChapter = target
        currentPage = 0
        // Close review panel if open so a stale panel doesn't linger after TOC navigation
        showReviewPanel = false
        reviewPanelChapter = null
        saveProgress()
        updateBookmarkState()
        checkContributionPrompt()
    }

    fun goBackChapter() {
        val prev = previousChapter ?: return
        val book = currentBook ?: return
        val target = prev.coerceIn(0, book.chapters.size - 1)
        previousChapter = null
        currentChapter = target
        currentPage = 0
        saveProgress()
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
                if (currentChapter != next) {
                    previousChapter = currentChapter
                }
                currentChapter = next
                currentPage = 0
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
                if (currentChapter != prev) {
                    previousChapter = currentChapter
                }
                currentChapter = prev
                currentPage = 0
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

    fun updateDarkMode(dark: Boolean) {
        isDarkMode = dark
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
        readerLanguage = code
        I18n.setLanguage(code)
        persistSettings()
    }

    fun updateLineSpacing(value: Float) {
        lineSpacing = value.coerceIn(1.0f, 3.0f)
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

    fun updateTranslateApiUrl(value: String) {
        translateApiUrl = value
        persistSettings()
    }

    fun updateTranslateApiKey(value: String) {
        translateApiKey = value
        persistSettings()
    }

    fun updateDictionaryApiUrl(value: String) {
        dictionaryApiUrl = value
        persistSettings()
    }

    fun updateDictionaryApiKey(value: String) {
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
        val closingUri = currentBookUri
        android.util.Log.d("READER-RESUME", "closeBook uri=$closingUri currentChapter=$currentChapter")
        saveProgress()
        if (closingUri != null) {
            clearLastBookIfMatches(closingUri)
        }
        currentBook = null
        currentBookUri = null
        currentChapter = 0
        currentPage = 0
    }

    private fun saveProgress() {
        val uri = currentBookUri ?: return
        val chapterTitle = try {
            currentBook?.chapters?.get(currentChapter)?.title
        } catch (_: Exception) { null }
        library.updateChapter(uri, currentChapter, chapterTitle)

        // Write chapter to SharedPreferences: global key + per-book key (resilient backup)
        val bookId = library.books.firstOrNull { it.uri == uri }?.id
        prefs.edit().apply {
            putInt(PrefKeys.LAST_BOOK_CHAPTER, currentChapter)
            if (bookId != null) {
                putInt("book_ch_$bookId", currentChapter)
            }
        }.apply()

        loadLibrary()
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
                appendLine("version_code=${pkgInfo.longVersionCode}")
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
                            sharingStatus = ""
                        } else {
                            sharingStatus = json.optString("error", "Unknown error")
                        }
                    } catch (_: Exception) {
                        sharingStatus = "Server start failed"
                    }
                }
            }
        }
    }

    fun stopSharingServer() {
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
                            scanBooksDir()
                            applySyncedProgress()
                            loadLibrary()
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
        if (autoSyncInProgress) return
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

    fun startDiscovery() {
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
                val peers = jsonParser.decodeFromString<List<DiscoveredPeer>>(result)
                discoveredPeers.clear()
                discoveredPeers.addAll(peers)
                tryAutoSyncWithPairedPeer(peers)
            } catch (_: Exception) { }
        }
    }

    // ── TXT 导入 ──

    fun openTxtImportDialog(path: String) {
        txtImportPath = path
        txtImportTitle = File(path).nameWithoutExtension
        txtImportAuthor = ""
        txtImportRegex = ""
        txtImportHeuristic = false
        txtImportError = null
        txtImportConverting = false
        showTxtImport = true
        refreshTxtPreviews()
    }

    fun refreshTxtPreviews() {
        val path = txtImportPath ?: return
        viewModelScope.launch(Dispatchers.IO) {
            val json = RustBridge.previewTxtChapters(
                path,
                txtImportHeuristic,
                txtImportRegex.ifEmpty { null }
            ) ?: "[]"
            val arr = org.json.JSONArray(json)
            val list = (0 until arr.length()).map { i ->
                val obj = arr.getJSONObject(i)
                TxtChapterPreview(
                    title = obj.getString("title"),
                    lineStart = obj.getInt("lineStart"),
                    charCount = obj.getInt("charCount")
                )
            }
            withContext(Dispatchers.Main) {
                txtImportPreviews = list
            }
        }
    }

    fun convertTxtToEpub() {
        val path = txtImportPath ?: return
        txtImportConverting = true
        txtImportError = null
        viewModelScope.launch(Dispatchers.IO) {
            val result = RustBridge.convertTxtToEpub(
                path,
                booksDir,
                txtImportTitle.ifEmpty { null },
                txtImportAuthor.ifEmpty { null },
                txtImportHeuristic,
                txtImportRegex.ifEmpty { null }
            )
            withContext(Dispatchers.Main) {
                txtImportConverting = false
                if (result == null) {
                    txtImportError = "转换失败"
                    return@withContext
                }
                try {
                    val obj = org.json.JSONObject(result)
                    if (obj.has("error")) {
                        txtImportError = obj.getString("error")
                    } else {
                        val epubPath = obj.getString("epubPath")
                        showTxtImport = false
                        txtImportPath = null
                        openFromPath(epubPath, 0)
                    }
                } catch (e: Exception) {
                    txtImportError = e.message
                }
            }
        }
    }

    fun dismissTxtImport() {
        showTxtImport = false
        // 清理临时 TXT 文件
        txtImportPath?.let { path ->
            val file = File(path)
            if (file.exists() && file.parentFile?.name == "imports") {
                runCatching { file.delete() }
            }
        }
        txtImportPath = null
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
    fun startGitHubLogin() {
        viewModelScope.launch {
            val result = com.zhongbai233.epub.reader.util.GitHubContributor.requestDeviceCode()
            result.onSuccess { dc ->
                githubDeviceCode = dc.deviceCode
                githubUserCode = dc.userCode
                githubVerificationUri = dc.verificationUri
                githubAuthPolling = true

                // Start polling for token in background
                viewModelScope.launch {
                    val authResult = com.zhongbai233.epub.reader.util.GitHubContributor.pollForToken(dc)
                    githubAuthPolling = false
                    when (authResult) {
                        is com.zhongbai233.epub.reader.util.GitHubContributor.AuthResult.Success -> {
                            githubToken = authResult.token
                            githubUsername = authResult.username
                            githubUserCode = null
                            githubVerificationUri = null
                            prefs.edit()
                                .putString(PrefKeys.GITHUB_TOKEN, authResult.token)
                                .putString(PrefKeys.GITHUB_USERNAME, authResult.username)
                                .apply()
                        }
                        is com.zhongbai233.epub.reader.util.GitHubContributor.AuthResult.Error -> {
                            contributeStatus = authResult.message
                        }
                    }
                }
            }.onFailure {
                contributeStatus = it.message ?: "Failed to start login"
            }
        }
    }

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

    fun toggleBookmark(): Boolean {
        val bookId = currentBookId ?: return false
        val dataDir = getApplication<Application>().filesDir.absolutePath
        val chapter = currentChapter
        viewModelScope.launch(Dispatchers.IO) {
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
        viewModelScope.launch(Dispatchers.IO) {
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
        val payload = """{"chapter":$chapter,"start_block":$startBlock,"start_offset":$startOffset,"end_block":$endBlock,"end_offset":$endOffset,"color":"$color"}"""
        viewModelScope.launch(Dispatchers.IO) {
            RustBridge.addHighlight(dataDir, bookId, payload)
            reloadBookConfig(dataDir, bookId)
        }
    }

    fun removeHighlight(highlightId: String) {
        val bookId = currentBookId ?: return
        val dataDir = getApplication<Application>().filesDir.absolutePath
        viewModelScope.launch(Dispatchers.IO) {
            RustBridge.removeHighlight(dataDir, bookId, highlightId)
            reloadBookConfig(dataDir, bookId)
        }
    }

    fun saveNote(highlightId: String, content: String) {
        val bookId = currentBookId ?: return
        val dataDir = getApplication<Application>().filesDir.absolutePath
        viewModelScope.launch(Dispatchers.IO) {
            RustBridge.saveNote(dataDir, bookId, highlightId, content)
            reloadBookConfig(dataDir, bookId)
        }
    }

    private suspend fun reloadBookConfig(dataDir: String, bookId: String) {
        val json = RustBridge.getBookConfig(dataDir, bookId) ?: return
        try {
            val cfg = jsonParser.decodeFromString<FullBookConfig>(json)
            withContext(Dispatchers.Main) {
                bookConfig = cfg
                updateBookmarkState()
            }
        } catch (_: Exception) { }
    }

    override fun onCleared() {
        super.onCleared()
        ttsManager.destroy()
        cscEngine.release()
        discoveryPollingJob?.cancel()
        viewModelScope.launch(Dispatchers.IO) {
            RustBridge.stopDiscoveryListener()
        }
        discoveryStarted = false
        autoSyncInProgress = false
        autoSyncLastAttemptAt.clear()
        try { multicastLock?.release() } catch (_: Exception) { }
        multicastLock = null
    }
}
