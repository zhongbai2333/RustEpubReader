package com.zhongbai233.epub.reader.viewmodel

import android.app.Application
import android.net.Uri
import android.net.wifi.WifiManager
import android.content.Context
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

class ReaderViewModel(application: Application) : AndroidViewModel(application) {

    private val context get() = getApplication<Application>()
    private val library = Library(context)
    private var currentBookUri: String? = null
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
    var isScrollMode by mutableStateOf(true)
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
    }

    private fun loadSettings() {
        fontSize = prefs.getFloat(PrefKeys.FONT_SIZE, 18f).coerceIn(12f, 40f)
        isDarkMode = prefs.getBoolean(PrefKeys.DARK_MODE, false)
        isScrollMode = prefs.getBoolean(PrefKeys.SCROLL_MODE, true)

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
            try {
                tempFile = withContext(Dispatchers.IO) {
                    copyUriToTempFile(uri)
                }
                parseAndOpen(tempFile ?: throw Exception(I18n.t("error.temp_file_failed")))
            } catch (e: Exception) {
                errorMessage = "��ʧ��: ${e.message}"
            } finally {
                tempFile?.let { file ->
                    if (file.exists() && file.parentFile?.name == "imports") {
                        runCatching { file.delete() }
                    }
                }
                isLoading = false
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
    }

    private suspend fun copyUriToTempFile(uri: Uri): File = withContext(Dispatchers.IO) {
        val importDir = File(context.cacheDir, "imports").also { it.mkdirs() }
        val dest = File(importDir, "import_${System.currentTimeMillis()}.epub")
        context.contentResolver.openInputStream(uri)?.use { input ->
            FileOutputStream(dest).use { output ->
                input.copyTo(output)
            }
        } ?: throw Exception("�޷���ȡ�ļ�")
        dest
    }

    fun goToChapter(index: Int) {
        val book = currentBook ?: return
        val target = index.coerceIn(0, book.chapters.size - 1)
        if (target != currentChapter) {
            previousChapter = currentChapter
        }
        currentChapter = target
        currentPage = 0
        saveProgress()
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
            currentChapter++
            currentPage = 0
            saveProgress()
        }
    }

    fun prevChapter() {
        if (currentChapter > 0) {
            currentChapter--
            currentPage = 0
            saveProgress()
        }
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

    override fun onCleared() {
        super.onCleared()
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
