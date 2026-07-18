package com.zhongbai233.epub.reader

object RustBridge {
    init {
        try {
            System.loadLibrary("android_bridge")
        } catch (e: Exception) {
            e.printStackTrace()
        }
    }

    external fun openBook(path: String): String?
    external fun getChapter(path: String, chapterIndex: Int): String?
    external fun getCover(path: String): String?

    // Library management
    external fun loadLibrary(dataDir: String): String?
    external fun addOrUpdateBook(dataDir: String, title: String, path: String, chapter: Int, chapterTitle: String): String?
    external fun updateChapter(dataDir: String, path: String, chapter: Int, chapterTitle: String)
    external fun updatePosition(dataDir: String, path: String, chapter: Int, chapterTitle: String, block: Int, charOffset: Int)
    external fun removeBook(dataDir: String, index: Int)
    external fun removeBookByPath(dataDir: String, path: String): Boolean

    // i18n
    external fun getAvailableLanguages(): String?
    external fun getTranslations(langCode: String): String?

    // Sharing
    external fun generatePin(): String?
    external fun startSharingServer(dataDir: String, booksDir: String, pin: String): String?
    external fun stopSharingServer()
    external fun connectAndListBooks(addr: String, pin: String, deviceId: String, dataDir: String, booksDir: String): String?
    external fun requestBookFromPeer(addr: String, pin: String, deviceId: String, dataDir: String, booksDir: String, hash: String): String?
    external fun syncProgressWithPeer(addr: String, pin: String, deviceId: String, dataDir: String, booksDir: String): String?
    external fun getPairedDevices(dataDir: String): String?
    external fun removePairedDevice(dataDir: String, deviceId: String): Boolean
    external fun startDiscoveryListener(dataDir: String)
    external fun stopDiscoveryListener()
    external fun getDiscoveredPeers(): String?
    external fun getSyncedProgress(dataDir: String): String?
    external fun fileHash(path: String): String?
    external fun readEpubMetadata(path: String): String?

    // TXT → EPUB
    external fun previewTxtChapters(txtPath: String, useHeuristic: Boolean, customRegex: String?): String?
    external fun convertTxtToEpub(txtPath: String, outputDir: String, title: String?, author: String?, useHeuristic: Boolean, customRegex: String?): String?

    // Search
    external fun searchBook(path: String, query: String): String?

    // Bookmarks / Highlights / Notes
    external fun getBookConfig(dataDir: String, bookId: String): String?
    external fun toggleBookmark(dataDir: String, bookId: String, chapter: Int): String?
    external fun addHighlight(dataDir: String, bookId: String, jsonPayload: String): String?
    external fun removeHighlight(dataDir: String, bookId: String, highlightId: String)
    external fun saveNote(dataDir: String, bookId: String, highlightId: String, content: String)

    // CSC Contribution
    external fun getCscCorrectionCount(dataDir: String, bookId: String): Int
    external fun collectCscSamples(dataDir: String, bookPath: String, bookId: String): String?

    // CSC Correction persistence
    external fun upsertCorrection(dataDir: String, bookId: String, jsonPayload: String): String?
}
