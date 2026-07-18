package com.zhongbai233.epub.reader.model

import android.content.Context
import com.zhongbai233.epub.reader.RustBridge
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.json.Json
import java.io.File

/**
 * Android 端书库管理器（Rust 驱动版）
 * - 所有持久化/迁移/去重/uuid.json 写入均交由 Rust core 统一处理
 * - Kotlin 仅保留轻量状态与 UI 友好接口
 */
class Library(private val context: Context) {

    private val json = Json { ignoreUnknownKeys = true; prettyPrint = true }
    private val dataDir: String
        get() = context.filesDir.absolutePath

    var books: MutableList<BookEntry> = mutableListOf()
        private set

    init {
        load()
    }

    private fun load() {
        val payload = RustBridge.loadLibrary(dataDir)
        books = try {
            if (payload.isNullOrBlank()) {
                mutableListOf()
            } else {
                val result = json.decodeFromString<List<BookEntry>>(payload).toMutableList()
                result.forEach { e ->
                    android.util.Log.d("READER-RESUME",
                        "Library.load: title=${e.title} lastChapter=${e.lastChapter} " +
                            "lastChapterTitle=${e.lastChapterTitle} configPath=${e.configPath}")
                }
                result
            }
        } catch (ex: Exception) {
            android.util.Log.e("READER-RESUME", "Library.load FAILED: ${ex.message}", ex)
            mutableListOf()
        }
    }

    /** Rust core 已在每次变更后持久化，保留此函数用于兼容旧调用方。 */
    fun save() {
        // no-op
    }

    fun addOrUpdate(
        title: String,
        uri: String,
        chapter: Int = 0,
        chapterTitle: String? = null
    ): BookEntry {
        val result = RustBridge.addOrUpdateBook(
            dataDir,
            title,
            uri,
            chapter,
            chapterTitle ?: ""
        )

        val returnedEntry = try {
            if (result.isNullOrBlank()) null else json.decodeFromString<BookEntry>(result)
        } catch (_: Exception) {
            null
        }

        load()

        return returnedEntry
            ?: books.firstOrNull { it.uri == uri }
            ?: BookEntry(
                title = title.ifBlank { File(uri).nameWithoutExtension.ifBlank { "Untitled" } },
                uri = uri,
                lastChapter = chapter,
                lastChapterTitle = chapterTitle,
                lastOpened = System.currentTimeMillis()
            )
    }

    fun remove(index: Int) {
        if (index in books.indices) {
            removeByUri(books[index].uri)
        }
    }

    fun removeByUri(uri: String): Boolean {
        val removed = RustBridge.removeBookByPath(dataDir, uri)
        if (removed) {
            load()
        }
        return removed
    }

    fun updateChapter(uri: String, chapter: Int, chapterTitle: String? = null) {
        RustBridge.updateChapter(dataDir, uri, chapter, chapterTitle ?: "")

        // 轻量更新本地内存态，避免每次翻页都完整 reload
        val idx = books.indexOfFirst { it.uri == uri }
        if (idx >= 0) {
            val prev = books[idx]
            val mergedTitle = chapterTitle
                ?: if (prev.lastChapter == chapter) prev.lastChapterTitle else null
            books[idx] = prev.copy(
                lastChapter = chapter,
                lastChapterTitle = mergedTitle,
                lastOpened = System.currentTimeMillis()
            )
        } else {
            load()
        }
    }

    fun updatePosition(
        uri: String,
        chapter: Int,
        chapterTitle: String? = null,
        block: Int = 0,
        charOffset: Int = 0
    ) {
        val safeBlock = block.coerceAtLeast(0)
        val safeOffset = charOffset.coerceAtLeast(0)
        RustBridge.updatePosition(
            dataDir,
            uri,
            chapter,
            chapterTitle ?: "",
            safeBlock,
            safeOffset
        )
        val idx = books.indexOfFirst { it.uri == uri }
        if (idx >= 0) {
            val previous = books[idx]
            books[idx] = previous.copy(
                lastChapter = chapter,
                lastBlock = safeBlock,
                lastCharOffset = safeOffset,
                lastChapterTitle = chapterTitle ?: previous.lastChapterTitle,
                lastOpened = System.currentTimeMillis()
            )
        }
    }

    /** 按最近打开时间排序，返回索引列表 */
    fun sortedIndicesByRecent(): List<Int> {
        return books.indices.sortedByDescending { books[it].lastOpened }
    }
}
