package com.zhongbai233.epub.reader.model

import kotlinx.serialization.Serializable

/** Local UTF-16 coordinates; conversion to Rust scalar offsets happens on save. */
@Serializable
data class ReaderLocation(val chapter: Int, val block: Int = 0, val charOffset: Int = 0)

@Serializable
data class ReaderReturnPoint(val location: ReaderLocation, val chapterTitle: String, val pageLabel: String)

fun recordReaderJump(history: List<ReaderReturnPoint>, origin: ReaderReturnPoint, target: ReaderLocation): List<ReaderReturnPoint> {
    if (origin.location == target || history.lastOrNull()?.location == origin.location) return history
    return (history + origin).takeLast(16)
}
