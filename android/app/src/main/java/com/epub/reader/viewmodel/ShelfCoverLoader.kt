package com.zhongbai233.epub.reader.viewmodel

import androidx.compose.runtime.mutableStateMapOf
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withPermit
import kotlinx.coroutines.withContext

/** Main-thread request/cache ownership; only thumbnail reads run on IO. */
internal class ShelfCoverLoader(
    private val scope: CoroutineScope,
    private val contains: (String) -> Boolean,
    private val requested: (String) -> Unit,
    private val read: suspend (String) -> ByteArray?,
    private val io: CoroutineDispatcher = Dispatchers.IO,
    private val capacity: Int = 96,
    parallelism: Int = 2,
) {
    val cache = mutableStateMapOf<String, ByteArray?>()
    private val pending = mutableSetOf<String>()
    private val workers = Semaphore(parallelism)
    private val access = java.util.LinkedHashMap<String, Unit>(capacity, 0.75f, true)

    init { require(capacity > 0 && parallelism > 0) }

    fun request(uri: String) {
        if (cache.containsKey(uri)) { access[uri] = Unit; return }
        if (!pending.add(uri)) return
        requested(uri)
        scope.launch {
            try {
                val bytes = workers.withPermit { withContext(io) { runCatching { read(uri) }.getOrNull() } }
                if (contains(uri)) {
                    access[uri] = Unit
                    while (access.size > capacity) {
                        val oldest = access.keys.first(); access.remove(oldest); cache.remove(oldest)
                    }
                    cache[uri] = bytes
                }
            } finally { pending.remove(uri) }
        }
    }
}
