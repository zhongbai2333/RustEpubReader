package com.zhongbai233.epub.reader.ui.library

import androidx.compose.ui.geometry.Rect

/** Read-only diagnostic seam. Shipping builds never install an observer. */
internal interface LibraryRenderObserver {
    fun coverBounds(uri: String, bounds: Rect)
    fun coverRequested(uri: String)
}
