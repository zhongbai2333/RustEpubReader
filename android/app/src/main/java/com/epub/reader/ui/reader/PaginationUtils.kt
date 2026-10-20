/**
 * PaginationUtils.kt — 分页与排版计算工具
 *
 * 负责将章节内容按屏幕尺寸进行分页，估算文本块高度，处理 CJK 标点禁则等排版逻辑。
 */
package com.zhongbai233.epub.reader.ui.reader

import android.graphics.BitmapFactory
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.model.*
import kotlin.math.ceil
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.PlatformTextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextIndent
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.unit.sp
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.graphics.Color

// ─── CJK 标点禁则 ───

internal val NO_BREAK_BEFORE = charArrayOf(
    '，', '。', '！', '？', '；', '：', '、', '\u201D', '\'', '）', '》',
    '」', '』', '】', '〉', '〕', '〗', '〙', '〛', '，', '．',
    '！', '？', '）', '：', '；', '\u201D', '\'', '」', '〉', '》',
    '】', '〗', '〙', '〛', '.', ',', '!', '?', ';', ':', ')',
    ']', '}', '…', '‥', '％', '‰', '℃', 'ー', '〜', '～'
)

internal val NO_BREAK_AFTER = charArrayOf(
    '\u201C', '\'', '（', '《', '「', '『', '【', '〈', '〔', '〖',
    '〘', '〚', '（', '\u201C', '\'', '「', '〈', '《', '【', '〖',
    '〘', '〚', '(', '[', '{'
)

/** 分页缓存最大章节数 */
internal const val PAGINATION_CACHE_MAX_SIZE = 10

/**
 * 页面中的渲染块。长段落可能被拆成多个片段，但仍保留其在原章节中的索引和字符偏移。
 */
internal data class PaginatedBlock(
    val block: ContentBlock,
    val sourceBlockIndex: Int,
    val sourceCharOffset: Int = 0,
    val imageMaxHeight: Dp = Dp.Unspecified
)

/**
 * 手动将标题按行宽拆成多行（插入 \n），避免 Compose 自动 wrap 导致 lineHeight 不生效。
 */
internal fun nativeTitleLineBreaks(title: String): List<Int> {
    val iterator = android.icu.text.BreakIterator.getLineInstance(android.icu.util.ULocale("zh@lb=strict"))
    iterator.setText(title)
    val offsets = mutableListOf<Int>()
    var offset = iterator.first()
    while (offset != android.icu.text.BreakIterator.DONE) {
        offsets.add(offset)
        offset = iterator.next()
    }
    return offsets
}

internal fun breakTitleIntoLines(
    title: String, contentWidthPx: Float, titleFontSizeSp: Float, spToPx: Float,
    lineBreaks: (String) -> List<Int> = ::nativeTitleLineBreaks
): String {
    // 使用 1.1f 倍宽作为字符估算，防止极端字体时一行的字被挤到下一行
    val charWidth = titleFontSizeSp * spToPx * 1.1f
    val charsPerLine = (contentWidthPx / charWidth).toInt().coerceAtLeast(4)
    if (title.length <= charsPerLine) return title
    val boundaries = lineBreaks(title)
    val sb = StringBuilder()
    var start = 0
    while (start < title.length) {
        val limit = (start + charsPerLine).coerceAtMost(title.length)
        val end = if (limit == title.length) title.length else
            boundaries.lastOrNull { it > start && it <= limit }
                ?: boundaries.firstOrNull { it > start }
                ?: title.length
        if (sb.isNotEmpty() && sb.last() != '\n' && title[start] != '\n') sb.append('\n')
        sb.append(title, start, end)
        start = end
    }
    return sb.toString()
}

internal fun paginateContent(
    chapter: Chapter,
    fontSize: Float,
    availableHeight: Dp,
    contentWidth: Dp,
    density: androidx.compose.ui.unit.Density,
    showChapterTitle: Boolean,
    titleVPaddingDp: Dp = CHAPTER_TITLE_TOP_PADDING + CHAPTER_TITLE_BOTTOM_PADDING,
    lineSpacing: Float = 1.5f,
    paraSpacing: Float = 0.5f,
    textIndentChars: Int = 2,
    titleFontScale: Float = 1.5f,
    textMeasurer: TextMeasurer? = null,
    fontFamily: FontFamily = FontFamily.Default,
    titleLineBreaks: (String) -> List<Int> = ::nativeTitleLineBreaks
): List<List<PaginatedBlock>> {
    val contentWidthPx = with(density) { contentWidth.toPx() }
    // sp → px 需要乘 fontScale（处理系统字体缩放）
    val spToPx = density.fontScale * density.density
    val lineHeight = fontSize * lineSpacing * spToPx
    
    // 采用更精准的容错边距（不再一次性扣除40dp+整行高，那会导致严重底部留白）
    // 给系统布局误差保留 0.5 行高的弹性空间足矣，因为屏幕 padding 本身已避开页码
    val safetyMarginPx = lineHeight * 0.5f
    val maxHeightPx = with(density) { availableHeight.toPx() } - safetyMarginPx
    
    val pages = mutableListOf<List<PaginatedBlock>>()
    var currentPage = mutableListOf<PaginatedBlock>()
    var currentHeight = 0f
    var isFirstPage = true

    // 第一页有章节标题占的高度
    if (isFirstPage && showChapterTitle) {
        // 标题实际渲染 lineHeight = (fontSize * 2.2f).sp
        val titleLineHeightPx = fontSize * titleFontScale * 1.45f * spToPx
        // 使用 breakTitleIntoLines 保持分行逻辑一致
        val brokenTitle = breakTitleIntoLines(chapter.title, contentWidthPx, fontSize * titleFontScale, spToPx, titleLineBreaks)
        val titleLines = brokenTitle.count { it == '\n' } + 1
        // Match the actual title padding; retain the font-measurement safety buffer.
        val titlePaddingPx = with(density) { titleVPaddingDp.toPx() }
        currentHeight += (titleLines * titleLineHeightPx + titlePaddingPx) * 1.3f
    }

    val imagesByBlock = chapter.images.groupBy { it.block }
    val renderBlocks = chapter.blocks.flatMapIndexed { index, block ->
        val images = imagesByBlock[index].orEmpty()
        images.filter { !it.after }.map { PaginatedBlock(it.content(), index) } +
            PaginatedBlock(block, index) + images.filter { it.after }.map { PaginatedBlock(it.content(), index,
                when (block) { is ContentBlock.Paragraph -> block.spans.sumOf { it.text.length }; is ContentBlock.Heading -> block.spans.sumOf { it.text.length }; else -> 0 }) }
    }
    val imageLimitPx = (maxHeightPx - fontSize * 0.7f * density.density).coerceAtLeast(24f)
    for (rendered in renderBlocks) {
        val blockIndex = rendered.sourceBlockIndex
        var block = rendered.block
        var native=block.nativeStyle()
        if(native.hidden)continue
        var fontSize=native.font(fontSize)
        var lineHeight=fontSize*native.lineScale(lineSpacing)*spToPx
        var paraSpacing=native.after?:paraSpacing
        var textIndentChars=(native.indent?:textIndentChars.toFloat()).toInt()
        // Keep a leading illustration with its chapter heading, rather than
        // spending a whole page on the heading before a full-height image.
        val blockImageLimitPx = if (block is ContentBlock.Image && currentHeight > 0f &&
            currentHeight < maxHeightPx * 0.35f && currentPage.all { it.block is ContentBlock.Heading }) {
            (imageLimitPx - currentHeight).coerceAtLeast(24f)
        } else imageLimitPx
        var blockHeight = if (block is ContentBlock.Image) {
            estimateImageBlockHeight(block.data, contentWidthPx, density, fontSize, blockImageLimitPx,native)
        } else measuredBlockHeight(block, fontSize, lineHeight, contentWidthPx, density, paraSpacing, textIndentChars, titleFontScale, textMeasurer, fontFamily)

        if(block is ContentBlock.Heading && blockHeight>maxHeightPx) {
            val scale=if(native.size!=null||native.sizePx!=null)1f else (titleFontScale*when(block.level){1->1.3f;2->1.1f;3->0.9f;else->0.8f}).coerceAtLeast(1f)
            native=native.copy(size=null,sizePx=fontSize*scale,bold=true,indent=0f,indentLength=null,
                before=(native.before?:1.2f)/scale,after=(native.after?:1.8f)/scale)
            block=ContentBlock.Paragraph(block.spans.map {it.copy(style=when(it.style){InlineStyle.Italic,InlineStyle.BoldItalic->InlineStyle.BoldItalic;else->InlineStyle.Bold})},block.anchor_id,native)
            fontSize=native.font(fontSize);lineHeight=fontSize*native.lineScale(lineSpacing)*spToPx
            paraSpacing=native.after?:paraSpacing;textIndentChars=0
            blockHeight=measuredBlockHeight(block,fontSize,lineHeight,contentWidthPx,density,paraSpacing,0,titleFontScale,textMeasurer,fontFamily)
        }
        if (block is ContentBlock.Paragraph && (native.table!=null||native.list.isNotEmpty()) && blockHeight>maxHeightPx) {
            native=native.copy(table=null,list=emptyList());block=block.copy(nativeStyle=native)
        }
        if (block is ContentBlock.Paragraph && native.table==null && native.list.isEmpty() && currentHeight + blockHeight > maxHeightPx) {
            var remaining: ContentBlock.Paragraph = block
            var sourceOffset = 0
            // Shape a long paragraph once, then reuse native line ends. Re-layout
            // only each short visible fragment, avoiding quadratic full-tail work.
            val paragraphLayout=if(textMeasurer!=null)paragraphGeometry(block,fontSize,lineHeight,contentWidthPx,density,paraSpacing,textIndentChars,textMeasurer,fontFamily,0).first else null

            while (remaining.spans.any { it.text.isNotEmpty() }) {
                if (sourceOffset > 0) remaining=remaining.copy(nativeStyle=remaining.nativeStyle.copy(indent=0f,indentLength=null,before=0f,marginTop=null,paddingTop=null))
                var remainingHeight = maxHeightPx - currentHeight
                // 不在页尾硬塞不足两行的段落片段，避免产生孤行。
                if (currentPage.isNotEmpty() && remainingHeight < lineHeight * 2f) {
                    pages.add(currentPage.toList())
                    currentPage = mutableListOf()
                    currentHeight = 0f
                    remainingHeight = maxHeightPx
                }

                val splitOffset = findParagraphSplitOffset(
                    paragraph = remaining,
                    sourceCharOffset = sourceOffset,
                    maxHeightPx = remainingHeight,
                    fontSize = fontSize,
                    lineHeight = lineHeight,
                    contentWidthPx = contentWidthPx,
                    density = density,
                    paraSpacing = paraSpacing,
                    textIndentChars = textIndentChars,
                    textMeasurer = textMeasurer, fontFamily = fontFamily, paragraphLayout = paragraphLayout
                )
                val textLength = remaining.spans.sumOf { it.text.length }
                val safeOffset = splitOffset.coerceIn(1, textLength)
                val (head, tail) = splitParagraph(remaining, safeOffset)

                currentPage.add(PaginatedBlock(head, blockIndex, sourceOffset))
                currentHeight += measuredBlockHeight(head, fontSize, lineHeight, contentWidthPx, density, paraSpacing, textIndentChars, titleFontScale,textMeasurer,fontFamily,sourceOffset)
                sourceOffset += safeOffset
                remaining = tail

                if (remaining.spans.any { it.text.isNotEmpty() }) {
                    pages.add(currentPage.toList())
                    currentPage = mutableListOf()
                    currentHeight = 0f
                }
            }
            continue
        }

        if (currentHeight + blockHeight > maxHeightPx && currentPage.isNotEmpty()) {
            pages.add(currentPage.toList())
            currentPage = mutableListOf()
            currentHeight = 0f
            isFirstPage = false
        }

        currentPage.add(rendered.copy(block=block,imageMaxHeight = if (block is ContentBlock.Image) with(density) { blockImageLimitPx.toDp() } else Dp.Unspecified))
        currentHeight += blockHeight
    }

    if (currentPage.isNotEmpty()) {
        pages.add(currentPage.toList())
    }

    return pages.ifEmpty { listOf(emptyList()) }
}

/** 找出在给定高度内可容纳的最大字符边界。 */
private fun findParagraphSplitOffset(
    paragraph: ContentBlock.Paragraph,
    sourceCharOffset: Int,
    maxHeightPx: Float,
    fontSize: Float,
    lineHeight: Float,
    contentWidthPx: Float,
    density: androidx.compose.ui.unit.Density,
    paraSpacing: Float,
    textIndentChars: Int,
    textMeasurer: TextMeasurer? = null,
    fontFamily: FontFamily = FontFamily.Default,
    paragraphLayout: androidx.compose.ui.text.TextLayoutResult? = null
): Int {
    val length = paragraph.spans.sumOf { it.text.length }
    if (length <= 1) return length

    if(textMeasurer != null) {
        val native=paragraph.nativeStyle
        val d=density.density
        fun resolve(v:NativeDimension?)=(v?.resolve(fontSize,contentWidthPx/d)?:0f)*d
        val vertical=resolve(native.marginTop)+resolve(native.paddingTop)+resolve(native.marginBottom)+resolve(native.paddingBottom)+fontSize*((native.before?:paraSpacing)+(native.after?:paraSpacing))*d
        val available=(maxHeightPx-vertical).coerceAtLeast(1f)
        val layout=paragraphLayout?:paragraphGeometry(paragraph,fontSize,lineHeight,contentWidthPx,density,paraSpacing,textIndentChars,textMeasurer,fontFamily,sourceCharOffset).first
        val base=if(paragraphLayout!=null)sourceCharOffset else 0
        val firstLine=layout.getLineForOffset(base.coerceAtMost(layout.layoutInput.text.length))
        val top=layout.getLineTop(firstLine)
        var end=base
        for(line in firstLine until layout.lineCount) {
            if(layout.getLineBottom(line)-top>available+0.1f)break
            end=layout.getLineEnd(line,visibleEnd=false)
        }
        var candidate=(end-base).coerceIn(1,length)
        // A line end shaped in the full paragraph can wrap differently once it
        // becomes the end of a fragment (for example before CJK punctuation).
        // Validate the actual short fragment, rather than overflowing the page
        // or reshaping the entire remaining chapter on every split.
        val source=paragraph.spans.joinToString(""){it.text}
        while(candidate>1) {
            val head=splitParagraph(paragraph,candidate).first
            val (shaped,padding)=paragraphGeometry(head,fontSize,lineHeight,contentWidthPx,density,paraSpacing,textIndentChars,textMeasurer,fontFamily,sourceCharOffset)
            if(shaped.size.height+ceil(padding)<=maxHeightPx+0.1f)break
            val previousLine=if(shaped.lineCount>1)shaped.getLineStart(shaped.lineCount-1)else candidate
            candidate=if(previousLine in 1 until candidate)previousLine else Character.offsetByCodePoints(source,candidate,-1)
            if(candidate>0 && candidate<source.length && Character.isLowSurrogate(source[candidate]) && Character.isHighSurrogate(source[candidate-1]))candidate--
        }
        return candidate.coerceAtLeast(Character.charCount(source.codePointAt(0)))
    }
    var low = 1
    var high = length
    var best = 1
    while (low <= high) {
        val mid = (low + high) ushr 1
        val candidate = splitParagraph(paragraph, mid).first
        val height = estimateBlockHeight(
            candidate, fontSize, lineHeight, contentWidthPx, density,
            paraSpacing, textIndentChars
        )
        if (height <= maxHeightPx) {
            best = mid
            low = mid + 1
        } else {
            high = mid - 1
        }
    }

    var boundary=adjustSplitForCjk(paragraph.spans.joinToString("") {it.text},best).coerceIn(1,length)
    for(ruby in paragraph.nativeStyle.ruby) {
        var end=ruby.end
        for(hidden in paragraph.nativeStyle.inline.filter {it.hidden}.sortedBy {it.start}) {if(hidden.start<=end&&hidden.end>end)end=hidden.end}
        val start=ruby.start-sourceCharOffset;val stop=end-sourceCharOffset
        if(start>=0&&boundary>start&&boundary<stop)boundary=if(start>0)start else stop.coerceAtMost(length)
    }
    return boundary.coerceIn(1,length)
}

/** 在保留内联样式和链接的前提下拆分段落。 */
private fun splitParagraph(
    paragraph: ContentBlock.Paragraph,
    offset: Int
): Pair<ContentBlock.Paragraph, ContentBlock.Paragraph> {
    val head = mutableListOf<TextSpan>()
    val tail = mutableListOf<TextSpan>()
    var consumed = 0
    for (span in paragraph.spans) {
        val local = (offset - consumed).coerceIn(0, span.text.length)
        if (local > 0) head.add(span.copy(text = span.text.substring(0, local)))
        if (local < span.text.length) tail.add(span.copy(text = span.text.substring(local)))
        consumed += span.text.length
    }
    return paragraph.copy(spans=head) to
        paragraph.copy(spans=tail)
}

private fun adjustSplitForCjk(text: String, proposed: Int): Int {
    var offset = proposed.coerceIn(1, text.length)
    while (offset > 1 && offset < text.length && NO_BREAK_BEFORE.contains(text[offset])) offset--
    while (offset > 1 && NO_BREAK_AFTER.contains(text[offset - 1])) offset--
    if(offset<text.length&&offset>0&&Character.isLowSurrogate(text[offset])&&Character.isHighSurrogate(text[offset-1]))offset--
    return offset.coerceAtLeast(Character.charCount(text.codePointAt(0)))
}

private fun paragraphGeometry(block:ContentBlock.Paragraph,font:Float,lineHeight:Float,width:Float,density:androidx.compose.ui.unit.Density,
    para:Float,indent:Int,measurer:TextMeasurer,family:FontFamily,sourceOffset:Int):Pair<androidx.compose.ui.text.TextLayoutResult,Float> {
    val native=block.nativeStyle
    val d=density.density
    fun resolve(v:NativeDimension?)=(v?.resolve(font,width/d)?:0f)*d
    val left=(resolve(native.insetLeft)+resolve(native.paddingLeft)).coerceAtMost(width*0.4f)
    val right=(resolve(native.insetRight)+resolve(native.paddingRight)).coerceAtMost(width*0.4f)
    val vertical=resolve(native.marginTop)+resolve(native.paddingTop)+resolve(native.marginBottom)+resolve(native.paddingBottom)+
        font*((native.before?:para)+(native.after?:para))*d
    val annotated=buildSpanAnnotatedString(block.spans,font,Color.Black,Color.Blue,family,null,native=native,sourceCharOffset=sourceOffset)
    val style=TextStyle(fontFamily=family,fontSize=font.sp,lineHeight=(lineHeight/d/density.fontScale).sp,
        textIndent=TextIndent(firstLine=(native.indentLength?.resolve(font,0f)?:font*(native.indent?:indent.toFloat())).sp),
        platformStyle=PlatformTextStyle(includeFontPadding=false),lineHeightStyle=LineHeightStyle(LineHeightStyle.Alignment.Center,LineHeightStyle.Trim.None))
    val layout=measurer.measure(annotated,style=style,constraints=Constraints(maxWidth=(width-left-right).toInt().coerceAtLeast(1)),skipCache=annotated.length>4096)
    return layout to vertical
}

internal fun measuredBlockHeight(block:ContentBlock,font:Float,lineHeight:Float,width:Float,density:androidx.compose.ui.unit.Density,
    para:Float,indent:Int,titleScale:Float,measurer:TextMeasurer?,family:FontFamily,sourceOffset:Int=0):Float {
    if(measurer==null)return estimateBlockHeight(block,font,lineHeight,width,density,para,indent,titleScale)
    if(block is ContentBlock.Paragraph&&block.nativeStyle.table==null&&block.nativeStyle.list.isEmpty()) {
        val (layout,vertical)=paragraphGeometry(block,font,lineHeight,width,density,para,indent,measurer,family,sourceOffset)
        return layout.size.height+ceil(vertical)
    }
    if(block is ContentBlock.Heading) {
        val native=block.nativeStyle
        val scale=if(native.size!=null||native.sizePx!=null)1f else (titleScale*when(block.level){1->1.3f;2->1.1f;3->0.9f;else->0.8f}).coerceAtLeast(1f)
        val paragraph=ContentBlock.Paragraph(block.spans.map {it.copy(style=when(it.style){InlineStyle.Italic,InlineStyle.BoldItalic->InlineStyle.BoldItalic;else->InlineStyle.Bold})},block.anchor_id,
            native.copy(align=native.align?:"center",indent=0f,indentLength=null,before=(native.before?:1.2f)/scale,after=(native.after?:1.8f)/scale))
        val (layout,vertical)=paragraphGeometry(paragraph,font*scale,lineHeight*scale,width,density,para,0,measurer,family,sourceOffset)
        return layout.size.height+ceil(vertical)
    }
    val native=block.nativeStyle()
    val d=density.density
    fun resolve(v:NativeDimension?)=(v?.resolve(font,width/d)?:0f)*d
    val inner=(width-(resolve(native.insetLeft)+resolve(native.paddingLeft)).coerceAtMost(width*0.4f)-
        (resolve(native.insetRight)+resolve(native.paddingRight)).coerceAtMost(width*0.4f)).coerceAtLeast(1f)
    val vertical=resolve(native.marginTop)+resolve(native.marginBottom)+resolve(native.paddingTop)+resolve(native.paddingBottom)
    fun textHeight(text:String,extent:Float,bold:Boolean=false)=measurer.measure(androidx.compose.ui.text.AnnotatedString(text),
        style=TextStyle(fontFamily=family,fontSize=font.sp,fontWeight=if(bold)FontWeight.Bold else FontWeight.Normal,
            letterSpacing=0.sp,lineHeight=(lineHeight/d/density.fontScale).sp,platformStyle=PlatformTextStyle(includeFontPadding=false),
            lineHeightStyle=LineHeightStyle(LineHeightStyle.Alignment.Center,LineHeightStyle.Trim.None)),
        constraints=Constraints(maxWidth=extent.toInt().coerceAtLeast(1))).size.height.toFloat()
    native.table?.let {table->
        return (table.cells.maxOfOrNull {cell->textHeight(cell.text,inner/table.columns.coerceAtLeast(1)*cell.span-12*d,cell.header)}?:lineHeight)+12*d+ceil(vertical)
    }
    if(native.list.isNotEmpty())return native.list.sumOf {item->
        (textHeight(item.text,inner-font*1.2f*(item.depth-native.listDepth).coerceAtLeast(0)*d-32*d)+font*0.3f*d).toDouble()
    }.toFloat()+ceil(vertical)
    // Grid/list/heading estimates remain conservative; oversized grids use full source paragraphs.
    return estimateBlockHeight(block,font,lineHeight,width,density,para,indent,titleScale)
}

internal fun estimateBlockHeight(
    block: ContentBlock,
    fontSize: Float,
    lineHeight: Float,
    contentWidthPx: Float,
    density: androidx.compose.ui.unit.Density,
    paraSpacing: Float = 0.5f,
    textIndentChars: Int = 2,
    titleFontScale: Float = 1.5f
): Float {
    val native=block.nativeStyle();if(native.hidden)return 0f
    val spToPx=density.fontScale*density.density
    fun resolve(v:NativeDimension?)=(v?.resolve(fontSize,contentWidthPx/density.density)?:0f)*density.density
    val left=(resolve(native.insetLeft)+resolve(native.paddingLeft)).coerceAtMost(contentWidthPx*0.4f)
    val right=(resolve(native.insetRight)+resolve(native.paddingRight)).coerceAtMost(contentWidthPx*0.4f)
    val width=(contentWidthPx-left-right).coerceAtLeast(32f*density.density)
    val vertical=resolve(native.marginTop)+resolve(native.marginBottom)+resolve(native.paddingTop)+resolve(native.paddingBottom)
    val scale=native.inline.filter {!it.hidden}.mapNotNull {it.size}.fold(1f,::maxOf)
    fun lines(text:String,font:Float,extent:Float,indent:Float=0f):Float=text.split('\n').mapIndexed {index,line->val total=line.sumOf {if(it.code>255)1.05 else 0.6}.toFloat()*font*spToPx+(if(index==0)indent else 0f);ceil(total/extent).coerceAtLeast(1f)}.sum()
    val height=if(native.list.isNotEmpty())native.list.sumOf {item->(lines(item.text,fontSize,(width-fontSize*1.2f*(item.depth-native.listDepth).coerceAtLeast(0)*density.density-32f*density.density).coerceAtLeast(32f*density.density))*lineHeight+fontSize*0.3f*density.density).toDouble()}.toFloat() else native.table?.let {table->val column=width/table.columns.coerceAtLeast(1);table.cells.maxOfOrNull {cell->lines(cell.text,fontSize,(column*cell.span-12f*density.density).coerceAtLeast(16f*density.density))*lineHeight}?.plus(12f*density.density)?:lineHeight}?:when(block) {
        is ContentBlock.Heading->{val headingScale=if(native.size!=null||native.sizePx!=null)1f else (titleFontScale*when(block.level){1->1.3f;2->1.1f;3->0.9f;else->0.8f}).coerceAtLeast(1f);lines(block.spans.joinToString(""){it.text},fontSize*headingScale*scale,width)*lineHeight*headingScale+fontSize*((native.before?:1.2f)+(native.after?:1.8f))*density.density}
        is ContentBlock.Paragraph->{val indent=native.indentLength?.resolve(fontSize,width/density.density)?.times(density.density)?:fontSize*spToPx*textIndentChars;lines(block.spans.joinToString(""){it.text},fontSize*scale,width,indent)*lineHeight+fontSize*((native.before?:paraSpacing)+(native.after?:paraSpacing))*density.density}
        is ContentBlock.Image->estimateImageBlockHeight(block.data,width,density,fontSize,native=native)
        is ContentBlock.Separator->lineHeight+fontSize*density.density
        is ContentBlock.BlankLine->fontSize*0.5f*density.density
    }
    return height+vertical
}

internal fun estimateImageBlockHeight(
    data: String,
    contentWidthPx: Float,
    density: androidx.compose.ui.unit.Density,
    fontSize: Float,
    maxHeightPx: Float = Float.POSITIVE_INFINITY,
    native: NativeStyle = NativeStyle()
): Float {
    val (width,naturalHeight)=ReaderImageCache.dimensions(data)?:return fontSize*1.8f*density.density
    val (_,height)=native.imageSize(width.toFloat(),naturalHeight.toFloat(),fontSize,contentWidthPx/density.density,maxHeightPx/density.density)
    return (height+fontSize*0.7f)*density.density
}

internal fun shouldRenderChapterTitle(chapter: Chapter): Boolean {
    val first = chapter.blocks.firstOrNull() as? ContentBlock.Heading ?: return true
    val headingText = first.spans.joinToString("") { it.text }.filterNot { it.isWhitespace() }
    val chapterText = chapter.title.filterNot { it.isWhitespace() }
    if (headingText.isBlank() || chapterText.isBlank()) return true
    // A parser fallback is navigation metadata; the EPUB's own heading is the visible title.
    // Keep every content block and its index intact for bookmarks and synchronized positions.
    if (Regex("^第\\s*\\d+\\s*章$").matches(chapterText)) return false
    return headingText != chapterText
}
