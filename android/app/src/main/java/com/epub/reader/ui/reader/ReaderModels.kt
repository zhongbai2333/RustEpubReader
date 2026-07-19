/**
 * ReaderModels.kt — 阅读器数据模型与工具类
 *
 * 包含文本选择状态、文本块布局信息等数据类，以及线程安全的 LRU 缓存工具函数。
 */
package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.text.TextLayoutResult
import java.util.Collections
import java.util.LinkedHashMap

/** Thread-safe LRU cache backed by [LinkedHashMap] with access-order eviction. */
internal fun <K, V> lruCache(maxSize: Int): MutableMap<K, V> =
    Collections.synchronizedMap(
        object : LinkedHashMap<K, V>(maxSize, 0.75f, true) {
            override fun removeEldestEntry(eldest: MutableMap.MutableEntry<K, V>?): Boolean =
                size > maxSize
        }
    )

data class TextSelectionState(
    val startBlock: Int,
    val startChar: Int,
    val endBlock: Int,
    val endChar: Int
)

data class BlockLayoutInfo(
    val text: String,
    val layoutResult: TextLayoutResult,
    val coordinates: LayoutCoordinates,
    val sourceCharOffset: Int = 0
)
