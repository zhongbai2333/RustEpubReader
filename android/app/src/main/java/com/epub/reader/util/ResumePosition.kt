package com.zhongbai233.epub.reader.util

/** The library is authoritative. Old preferences must not undo an intentional reread. */
fun resolveResumeChapter(libraryChapter: Int?, requestedChapter: Int, legacyChapter: Int = 0): Int =
    (libraryChapter ?: if (requestedChapter > 0) requestedChapter else legacyChapter).coerceAtLeast(0)
