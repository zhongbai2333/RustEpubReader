package com.zhongbai233.epub.reader.ui.reader

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.util.Base64
import android.util.LruCache
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/** Shared by the current, underlying and curled-back page compositions. */
internal object ReaderImageCache {
    // Enough for three maximum-size decoded pages, bounded independently of book length.
    private val cache = object : LruCache<String, Bitmap>(64 * 1024 * 1024) {
        override fun sizeOf(key: String, value: Bitmap): Int = value.allocationByteCount + key.length * 2
    }
    private val decodeLock = Mutex()
    private val boundsCache = object : LruCache<String, Pair<Int, Int>>(8 * 1024 * 1024) {
        override fun sizeOf(key: String, value: Pair<Int, Int>) = key.length * 2 + 16
    }
    fun dimensions(data: String): Pair<Int, Int>? = runCatching { synchronized(boundsCache) {
        boundsCache.get(data)?.let { return@synchronized it }
        runCatching {
            val bytes = Base64.decode(data, Base64.DEFAULT)
            val options = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options)
            if (options.outWidth > 0 && options.outHeight > 0) {
                (options.outWidth to options.outHeight).also { boundsCache.put(data, it) }
            } else null
        }.getOrNull()
    } }.getOrNull()
    fun peek(data: String): Bitmap? = cache.get(data)
    suspend fun load(data: String): Bitmap? = peek(data) ?: decodeLock.withLock {
        peek(data) ?: runCatching {
            val bytes = Base64.decode(data, Base64.DEFAULT)
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
            if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return@runCatching null
            val options = BitmapFactory.Options().apply { inSampleSize = 1 }
            while (bounds.outWidth.toLong() * bounds.outHeight / options.inSampleSize / options.inSampleSize > 4_194_304L) {
                options.inSampleSize *= 2
            }
            BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options)?.also { cache.put(data, it) }
        }.getOrNull()
    }
    // Eviction drops the cache reference; visible Compose layers may still use the bitmap.
    // Do not recycle it while those layers are drawing.
}
