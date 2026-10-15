package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.runtime.*
import androidx.compose.ui.geometry.Rect

/** Only decoded images intersecting the actual reader viewport delay its reveal. */
internal class VisibleReaderImages {
    private data class Entry(val bounds: Rect? = null, val prepared: Boolean = false)
    private val entries = mutableStateMapOf<Any, Entry>()
    var viewport by mutableStateOf(Rect.Zero)
    val ready get() = viewport != Rect.Zero && entries.values.none {
        it.bounds?.overlaps(viewport) == true && !it.prepared
    }
    fun position(key: Any, bounds: Rect) { entries[key]=(entries[key]?:Entry()).copy(bounds=bounds) }
    fun prepared(key: Any, prepared: Boolean) { entries[key]=(entries[key]?:Entry()).copy(prepared=prepared) }
    fun remove(key: Any) { entries.remove(key) }
}
internal val LocalVisibleReaderImages = staticCompositionLocalOf<VisibleReaderImages?> { null }
