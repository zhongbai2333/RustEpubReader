package com.zhongbai233.epub.reader.ui.reader

/** Virtual boundary pages belong to the adjacent chapter, except during a chapter handoff. */
internal fun renderedHighlightChapter(
    currentChapter: Int,
    isTransitioning: Boolean,
    isLeadingVirtual: Boolean,
    isTrailingVirtual: Boolean,
): Int = when {
    isTransitioning -> currentChapter
    isLeadingVirtual -> currentChapter - 1
    isTrailingVirtual -> currentChapter + 1
    else -> currentChapter
}
