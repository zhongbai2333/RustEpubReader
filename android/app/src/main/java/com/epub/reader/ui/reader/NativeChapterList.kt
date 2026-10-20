package com.zhongbai233.epub.reader.ui.reader

import com.zhongbai233.epub.reader.model.*

/** Styling one chapter must not force all lazy Rust chapters through the UI thread. */
internal class NativeChapterList(
    private val source: List<Chapter>,
    private val presentations: Map<Int, NativePresentation>,
) : AbstractList<Chapter>() {
    val publicationKey = ChapterListIdentity(source)
    private val styled = object : LinkedHashMap<Int, Chapter>(8, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<Int, Chapter>?): Boolean = size > 8
    }
    val residentCount: Int get() = synchronized(styled) { styled.size }
    override val size get() = source.size
    override fun get(index: Int): Chapter = synchronized(styled) { styled.getOrPut(index) {
        val chapter=source[index]
        val styles=presentations[index]?.styles ?: return@getOrPut chapter
        chapter.copy(blocks=chapter.blocks.mapIndexed { i,block ->
            val text=when(block) {
                is ContentBlock.Paragraph -> block.spans.joinToString(""){it.text}
                is ContentBlock.Heading -> block.spans.joinToString(""){it.text}
                else -> ""
            }
            val style=(styles.getOrNull(i)?:NativeStyle()).inUtf16(text)
            when(block) {
                is ContentBlock.Paragraph -> block.copy(nativeStyle=style)
                is ContentBlock.Heading -> block.copy(nativeStyle=style)
                is ContentBlock.Image -> block.copy(nativeStyle=style)
                else -> block
            }
        },images=chapter.images.map {it.copy(nativeStyle=styles.getOrNull(it.block)?:NativeStyle())})
    } }
}

/** Compose effects need identity, rather than AbstractList's eager structural equality. */
internal class ChapterListIdentity(private val source: List<Chapter>) {
    override fun equals(other: Any?) = other is ChapterListIdentity && source === other.source
    override fun hashCode() = System.identityHashCode(source)
}
internal fun publicationKey(chapters: List<Chapter>): ChapterListIdentity =
    (chapters as? NativeChapterList)?.publicationKey ?: ChapterListIdentity(chapters)
