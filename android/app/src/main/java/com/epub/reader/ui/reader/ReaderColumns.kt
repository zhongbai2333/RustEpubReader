package com.zhongbai233.epub.reader.ui.reader

internal fun readerColumnCount(widthDp: Float, forceSingleColumn: Boolean): Int =
    if (!forceSingleColumn && widthDp > 600f) 2 else 1
