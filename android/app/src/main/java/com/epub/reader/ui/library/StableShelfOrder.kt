package com.zhongbai233.epub.reader.ui.library

/** Metadata updates must not move an existing cover during an open/close cycle. */
class StableShelfOrder {
    private var ids=emptyList<String>()
    private var byTitle=false
    private var revision=0
    fun reset() { ids=emptyList() }
    fun resolve(ranked:List<String>, title:Boolean, nextRevision:Int = 0):List<String> {
        if(ranked.isEmpty()) return emptyList()
        if(revision != nextRevision) {reset();revision=nextRevision}
        val current=ranked.toSet()
        ids=if(ids.isEmpty() || title || byTitle!=title) ranked.toList()
            else ids.filter {it in current} + ranked.filter {it !in ids}
        byTitle=title
        return ids
    }
}
