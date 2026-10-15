package com.zhongbai233.epub.reader.ui.reader

import com.zhongbai233.epub.reader.model.Chapter

/** Background pagination must not race the UI or reuse a different font/stylesheet. */
internal class PageLayoutCache(private val capacity: Int) {
    private data class Entry(val chapter: Chapter, val geometry: String, val pages: List<List<PaginatedBlock>>)
    private val entries=LinkedHashMap<Int,Entry>(capacity,0.75f,true)
    @Synchronized fun get(index:Int, chapter:Chapter, geometry:String):List<List<PaginatedBlock>>? =
        entries[index]?.takeIf {it.chapter==chapter && it.geometry==geometry}?.pages
    @Synchronized fun peek(index:Int, geometry:String):List<List<PaginatedBlock>>? =
        entries[index]?.takeIf {it.geometry==geometry}?.pages
    @Synchronized fun put(index:Int, chapter:Chapter, geometry:String, pages:List<List<PaginatedBlock>>) {
        entries[index]=Entry(chapter,geometry,pages)
        while(entries.size>capacity) entries.entries.iterator().apply {next();remove()}
    }
}
