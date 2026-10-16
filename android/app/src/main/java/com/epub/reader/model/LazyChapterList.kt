package com.zhongbai233.epub.reader.model

/** Bounded decoded bodies; the catalogue does not read chapter content. */
internal class LazyChapterList(
    override val size: Int,
    private val load: (Int) -> Chapter,
) : AbstractList<Chapter>() {
    private val cache = object : LinkedHashMap<Int, Chapter>(8, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<Int, Chapter>?): Boolean = size > 8
    }
    val residentCount: Int get() = synchronized(cache) { cache.size }
    override fun get(index: Int): Chapter {
        if (index !in 0 until size) throw IndexOutOfBoundsException()
        return synchronized(cache) { cache.getOrPut(index) { load(index) } }
    }
}
