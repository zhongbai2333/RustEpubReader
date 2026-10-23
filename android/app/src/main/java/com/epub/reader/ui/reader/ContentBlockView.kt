package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.foundation.layout.fillMaxHeight

import androidx.compose.foundation.border

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.layout.*
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalViewConfiguration
import androidx.compose.ui.text.PlatformTextStyle
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.text.style.TextIndent
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.zhongbai233.epub.reader.csc.CorrectionInfo
import com.zhongbai233.epub.reader.csc.CorrectionStatus
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.model.*
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

// ─── CSC 纠错 block 级数据 ───

data class CscBlockCorrection(
    val localOffset: Int,
    val globalCharOffset: Int,
    val original: String,
    val corrected: String,
    val confidence: Float,
    val status: CorrectionStatus,
    val blockIndex: Int = -1
)

/**
 * 将全局 CscCorrections 映射为 per-block corrections。
 * 与 ViewModel 中 runCscCheckOnCurrentChapter 逻辑一致：
 * 只取 Heading / Paragraph, 用 "\n" 连接。
 */
internal fun mapCscCorrectionsToBlocks(
    blocks: List<ContentBlock>,
    corrections: List<CorrectionInfo>
): Map<Int, List<CscBlockCorrection>> {
    if (corrections.isEmpty()) return emptyMap()
    // 计算每个 text block 在 fullText 中的 (startOffset, endOffset, blockIndex)
    data class BlockRange(val blockIndex: Int, val start: Int, val end: Int)
    val ranges = mutableListOf<BlockRange>()
    var cursor = 0
    blocks.forEachIndexed { index, block ->
        val text = when (block) {
            is ContentBlock.Paragraph -> block.spans.joinToString("") { it.text }
            is ContentBlock.Heading -> block.spans.joinToString("") { it.text }
            else -> null
        }
        if (text != null) {
            if (ranges.isNotEmpty()) cursor++ // "\n" 分隔符
            ranges.add(BlockRange(index, cursor, cursor + text.length))
            cursor += text.length
        }
    }
    // 分配每个 correction 到对应的 block
    val result = mutableMapOf<Int, MutableList<CscBlockCorrection>>()
    for (c in corrections) {
        for (r in ranges) {
            if (c.charOffset >= r.start && c.charOffset < r.end) {
                result.getOrPut(r.blockIndex) { mutableListOf() }.add(
                    CscBlockCorrection(
                        localOffset = c.charOffset - r.start,
                        globalCharOffset = c.charOffset,
                        original = c.original,
                        corrected = c.corrected,
                        confidence = c.confidence,
                        status = c.status,
                        blockIndex = r.blockIndex
                    )
                )
                break
            }
        }
    }
    return result
}

// ─── 内容块渲染 ───

@Composable
internal fun ContentBlockView(
    chapterIndex: Int,
    blockIndex: Int?,
    block: ContentBlock,
    sourceText: String? = null,
    sourceCharOffset: Int = 0,
    imageMaxHeight: Dp = Dp.Unspecified,
    fontSize: Float,
    textColor: Color,
    linkColor: Color,
    bgColor: Color = Color.White,
    fontFamily: FontFamily,
    onLinkClick: (String) -> Unit,
    onTextTapped: () -> Unit,
    lineSpacing: Float = 1.5f,
    paraSpacing: Float = 0.5f,
    textIndentChars: Int = 2,
    titleFontScale: Float = 1.5f,
    textSelection: TextSelectionState? = null,
    onSelectionChange: (TextSelectionState?) -> Unit = {},
    blockLayoutRegistry: MutableMap<Int, BlockLayoutInfo>? = null,
    highlights: List<HighlightDto> = emptyList(),
    cscBlockCorrections: List<CscBlockCorrection> = emptyList(),
    cscMode: String = "none",
    ttsCurrentBlock: Int = -1,
    onCscCorrectionClick: (CscBlockCorrection, androidx.compose.ui.geometry.Offset) -> Unit = { _, _ -> }
, latinFamily: FontFamily? = null) {
    val native=block.nativeStyle()
    if(native.hidden)return
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val font=native.font(fontSize);fun resolve(v:com.zhongbai233.epub.reader.model.NativeDimension?)=v?.resolve(font,maxWidth.value)?:0f
        val left=(resolve(native.insetLeft)+resolve(native.paddingLeft)).coerceAtMost(maxWidth.value*0.4f)
        val right=(resolve(native.insetRight)+resolve(native.paddingRight)).coerceAtMost(maxWidth.value*0.4f)
        androidx.compose.foundation.layout.Box(Modifier.padding(start=left.dp,end=right.dp,top=(resolve(native.marginTop)+resolve(native.paddingTop)).dp,bottom=(resolve(native.marginBottom)+resolve(native.paddingBottom)).dp)) {
            ContentBlockViewContent(chapterIndex=chapterIndex,blockIndex=blockIndex,block=block,sourceText=sourceText,sourceCharOffset=sourceCharOffset,imageMaxHeight=imageMaxHeight,fontSize=fontSize,textColor=textColor,linkColor=linkColor,bgColor=bgColor,fontFamily=fontFamily,onLinkClick=onLinkClick,onTextTapped=onTextTapped,lineSpacing=lineSpacing,paraSpacing=paraSpacing,textIndentChars=textIndentChars,titleFontScale=titleFontScale,textSelection=textSelection,onSelectionChange=onSelectionChange,blockLayoutRegistry=blockLayoutRegistry,highlights=highlights,cscBlockCorrections=cscBlockCorrections,cscMode=cscMode,ttsCurrentBlock=ttsCurrentBlock,onCscCorrectionClick=onCscCorrectionClick, latinFamily=latinFamily)
        }
    }
}

@Composable
private fun ContentBlockViewContent(
    chapterIndex: Int,
    blockIndex: Int?,
    block: ContentBlock,
    sourceText: String? = null,
    sourceCharOffset: Int = 0,
    imageMaxHeight: Dp = Dp.Unspecified,
    fontSize: Float,
    textColor: Color,
    linkColor: Color,
    bgColor: Color = Color.White,
    fontFamily: FontFamily,
    onLinkClick: (String) -> Unit,
    onTextTapped: () -> Unit,
    lineSpacing: Float = 1.5f,
    paraSpacing: Float = 0.5f,
    textIndentChars: Int = 2,
    titleFontScale: Float = 1.5f,
    textSelection: TextSelectionState? = null,
    onSelectionChange: (TextSelectionState?) -> Unit = {},
    blockLayoutRegistry: MutableMap<Int, BlockLayoutInfo>? = null,
    highlights: List<HighlightDto> = emptyList(),
    cscBlockCorrections: List<CscBlockCorrection> = emptyList(),
    cscMode: String = "none",
    ttsCurrentBlock: Int = -1,
    onCscCorrectionClick: (CscBlockCorrection, androidx.compose.ui.geometry.Offset) -> Unit = { _, _ -> }
, latinFamily: FontFamily? = null) {
    val fragmentText = remember(block) { readerSourceText(block) }
    val source = sourceText ?: fragmentText
    val fragmentLength = fragmentText.length
    val projectedHighlights = remember(highlights, chapterIndex, blockIndex, source, sourceCharOffset, fragmentLength) {
        if (blockIndex == null) emptyList() else highlights.mapNotNull { highlight ->
            highlightFragmentRange(highlight, chapterIndex, blockIndex, source, sourceCharOffset, fragmentLength)
                ?.let { it to highlight.color }
        }
    }
    val sourceNative=block.nativeStyle()
    val native=if((bgColor.red+bgColor.green+bgColor.blue)>1.5f)sourceNative else sourceNative.copy(inline=sourceNative.inline.map {it.copy(color=null)})
    val block=when(block){
        is ContentBlock.Paragraph->block.copy(spans=block.spans.map {span->span.copy(style=when{native.bold&&native.italic->InlineStyle.BoldItalic;native.bold->InlineStyle.Bold;native.italic->InlineStyle.Italic;else->span.style})})
        is ContentBlock.Heading->block.copy(spans=block.spans.map {span->span.copy(style=if(native.italic)InlineStyle.BoldItalic else span.style)})
        else->block
    }
    val textColor=if(native.color!=null && (bgColor.red+bgColor.green+bgColor.blue)>1.5f)runCatching {Color(android.graphics.Color.parseColor(native.color))}.getOrDefault(textColor) else textColor
    val fontFamily=when(native.family?.lowercase()){ "serif"->FontFamily.Serif;"sans-serif"->FontFamily.SansSerif;"monospace"->FontFamily.Monospace;else->fontFamily}
    if(native.hidden)return
    val fontSize=native.font(fontSize)
    val lineSpacing=native.lineScale(lineSpacing)
    val paraSpacing=native.after?:paraSpacing
    val textIndentChars=(native.indent?:textIndentChars.toFloat()).toInt()
    val nativeAlign=when(native.align){"center"->androidx.compose.ui.text.style.TextAlign.Center;"right"->androidx.compose.ui.text.style.TextAlign.Right;"justify"->androidx.compose.ui.text.style.TextAlign.Justify;else->androidx.compose.ui.text.style.TextAlign.Left}
    val fallbackLabel=when(block){is ContentBlock.Paragraph->block.spans.joinToString(""){it.text};is ContentBlock.Heading->block.spans.joinToString(""){it.text};else->""}
    if(native.action!=null){androidx.compose.material3.TextButton(onClick={onLinkClick("reader-event:${native.action}")}){Text(native.text?:fallbackLabel,fontSize=fontSize.sp)};return}
    if(native.text!=null){Text(native.text,fontSize=fontSize.sp,color=textColor,lineHeight=(fontSize*lineSpacing).sp,modifier=Modifier.padding(vertical=(fontSize*paraSpacing).dp));return}
    val viewConfiguration = LocalViewConfiguration.current
    val textMeasurer = rememberTextMeasurer()
    if(native.list.isNotEmpty()) {
        androidx.compose.foundation.layout.Column {for(item in native.list){androidx.compose.foundation.layout.Row(Modifier.fillMaxWidth().padding(start=(fontSize*1.2f*(item.depth-native.listDepth).coerceAtLeast(0)).dp,bottom=(fontSize*0.3f).dp)) {Text(item.marker,fontSize=fontSize.sp,color=textColor,modifier=Modifier.padding(end=8.dp));Text(mixedScriptText(item.text,fontFamily,latinFamily),fontSize=fontSize.sp,lineHeight=(fontSize*lineSpacing).sp,color=textColor,fontFamily=fontFamily)}}}
        return
    }
    native.table?.let {table->
        androidx.compose.foundation.layout.Row(Modifier.fillMaxWidth().height(androidx.compose.foundation.layout.IntrinsicSize.Min)) {
            for(cell in table.cells) {
                Text(mixedScriptText(cell.text,fontFamily,latinFamily),color=textColor,fontSize=fontSize.sp,fontFamily=fontFamily,fontWeight=if(cell.header)FontWeight.Bold else FontWeight.Normal,lineHeight=(fontSize*lineSpacing).sp,textAlign=when(cell.align){"right"->androidx.compose.ui.text.style.TextAlign.Right;"center"->androidx.compose.ui.text.style.TextAlign.Center;else->androidx.compose.ui.text.style.TextAlign.Left},modifier=Modifier.weight(cell.span.toFloat()).fillMaxHeight().border(0.5.dp,textColor.copy(alpha=0.25f)).run {if(cell.header)background(textColor.copy(alpha=0.06f))else this}.padding(6.dp))
            }
            val missing=table.columns-table.cells.sumOf {it.span};if(missing>0)Spacer(Modifier.weight(missing.toFloat()))
        }
        return
    }
    when (block) {
        is ContentBlock.Heading -> {
            val scale = if(native.size!=null||native.sizePx!=null)1f else when (block.level) {
                1 -> titleFontScale * 1.3f
                2 -> titleFontScale * 1.1f
                3 -> titleFontScale * 0.9f
                else -> titleFontScale * 0.8f
            }
                .coerceAtLeast(1.0f)
            val annotated = buildSpanAnnotatedString(
                spans = block.spans,
                fontSize = fontSize * scale,
                textColor = textColor,
                linkColor = linkColor,
                fontFamily = fontFamily,
                overrideWeight = FontWeight.Bold,
                onLinkClick = onLinkClick, native = native, sourceCharOffset = sourceCharOffset
            , latinFamily=latinFamily)
            val headingStyle = TextStyle(
                letterSpacing = 0.sp,
                fontSize = (fontSize * scale).sp,
                fontFamily = fontFamily,
                textAlign = if (native.align == null) androidx.compose.ui.text.style.TextAlign.Center else nativeAlign,
                lineHeight = (fontSize * scale * lineSpacing).sp,
                platformStyle = PlatformTextStyle(includeFontPadding = false),
                lineHeightStyle = LineHeightStyle(
                    alignment = LineHeightStyle.Alignment.Center,
                    trim = LineHeightStyle.Trim.None
                )
            )

            val idx = blockIndex
            DisposableEffect(idx) {
                onDispose {
                    if (idx != null) {
                        blockLayoutRegistry?.remove(idx)
                    }
                }
            }
            var layoutResult by remember { mutableStateOf<TextLayoutResult?>(null) }
            
            Text(
                text = annotated,
                style = headingStyle,
                modifier = Modifier
                    .fillMaxWidth()
                    .run { if (idx != null && idx == ttsCurrentBlock) background(androidx.compose.ui.graphics.Color(0x4DFFEB3B), androidx.compose.foundation.shape.RoundedCornerShape(8.dp)) else this }
                    .padding(top = (fontSize * (native.before?:1.2f)).dp, bottom = (fontSize * (native.after?:1.8f)).dp)
                    .onGloballyPositioned { coordinates ->
                        val lr = layoutResult
                        if (idx != null && lr != null) {
                            blockLayoutRegistry?.set(idx, BlockLayoutInfo(annotated.text, lr, coordinates, sourceCharOffset))
                        }
                    }
                    .drawWithContent {
                        drawContent()
                        val lr = layoutResult ?: return@drawWithContent
                        drawPublicationRuby(lr,native,sourceCharOffset,fontSize,textColor)
                        // Ranges are projected once from source scalars into this fragment's UTF-16 layout.
                        for ((range, color) in projectedHighlights) {
                            if (range.end <= annotated.length) {
                                val hlPath = lr.multiParagraph.getPathForRange(range.start, range.end)
                                drawPath(hlPath, color = highlightColor(color))
                            }
                        }
                        // 渲染 CSC 纠错
                        if (cscBlockCorrections.isNotEmpty()) {
                            android.util.Log.d("CscRender", "heading=$idx mode=$cscMode corrections=${cscBlockCorrections.size}")
                        }
                        for (csc in cscBlockCorrections) {
                            if (csc.status != CorrectionStatus.ACCEPTED && csc.status != CorrectionStatus.IGNORED) {
                                val cscStart = csc.localOffset
                                val cscEnd = (csc.localOffset + csc.original.length).coerceAtMost(annotated.length)
                                if (cscStart in 0 until annotated.length && cscStart < cscEnd) {
                                    android.util.Log.d("CscRender", "draw heading=$idx mode=$cscMode start=$cscStart end=$cscEnd orig=${csc.original} corr=${csc.corrected} status=${csc.status}")
                                    if (cscMode == "readonly") {
                                        drawCscRubyAnnotation(lr, cscStart, csc.original, csc.corrected, textMeasurer, fontSize * scale, bgColor)
                                    } else {
                                        drawCscWavyUnderline(lr, cscStart, cscEnd)
                                    }
                                }
                            }
                        }
                        // 渲染实时选区
                        val selState = textSelection
                        if (selState != null && idx != null && idx in selState.startBlock..selState.endBlock) {
                            val localStart = if (idx == selState.startBlock) selState.startChar - sourceCharOffset else 0
                            val localEnd = if (idx == selState.endBlock) selState.endChar - sourceCharOffset else annotated.length
                            if (localStart < localEnd && localStart >= 0 && localEnd <= annotated.length) {
                                val selPath = lr.multiParagraph.getPathForRange(localStart, localEnd)
                                drawPath(selPath, color = Color(0x4266D3FF))
                            }
                        }
                    }
                    .pointerInput(block, onLinkClick, cscMode, cscBlockCorrections) {
                        awaitEachGesture {
                            val down = awaitFirstDown(pass = PointerEventPass.Initial, requireUnconsumed = false)
                            val up = waitForUpOrCancellation(pass = PointerEventPass.Initial) ?: return@awaitEachGesture
                            val isQuickTap = (up.uptimeMillis - down.uptimeMillis) <= 220L
                            val isSmallMove = (up.position - down.position).getDistance() <= viewConfiguration.touchSlop
                            if (isQuickTap && isSmallMove) {
                                layoutResult?.let { result ->
                                    val offset = result.getOffsetForPosition(up.position)
                                    // CSC readwrite 模式：点击纠错区域弹出菜单
                                    if (cscMode == "readwrite") {
                                        val tappedCsc = cscBlockCorrections.firstOrNull { csc ->
                                            csc.status != CorrectionStatus.ACCEPTED && csc.status != CorrectionStatus.IGNORED &&
                                            offset >= csc.localOffset && offset < csc.localOffset + csc.original.length
                                        }
                                        if (tappedCsc != null) {
                                            onCscCorrectionClick(tappedCsc, up.position)
                                            down.consume()
                                            up.consume()
                                            return@awaitEachGesture
                                        }
                                    }
                                    val startOffset = (offset - 2).coerceAtLeast(0)
                                    val endOffset = (offset + 2).coerceAtMost(annotated.length)
                                    val annotations = annotated.getStringAnnotations(tag = "URL", start = startOffset, end = endOffset)
                                    if (annotations.isNotEmpty()) {
                                        onLinkClick(annotations.last().item)
                                        down.consume()
                                        up.consume()
                                    } else {
                                        // 普通文本点击交给页面级手势处理，避免正文块把整页点击吃掉
                                    }
                                }
                            }
                        }
                    },
                onTextLayout = { layoutResult = it }
            )
        }

        is ContentBlock.Paragraph -> {
            val annotated = buildSpanAnnotatedString(
                spans = block.spans,
                fontSize = fontSize,
                textColor = textColor,
                linkColor = linkColor,
                fontFamily = fontFamily,
                overrideWeight = null,
                onLinkClick = onLinkClick, native = native, sourceCharOffset = sourceCharOffset
            , latinFamily=latinFamily)
            val baseStyle = TextStyle(
                letterSpacing = 0.sp,
                fontSize = fontSize.sp,
                fontFamily = fontFamily,
                textAlign = nativeAlign,
                textIndent = TextIndent(firstLine = (native.indentLength?.resolve(fontSize,0f)?:fontSize * (native.indent?:textIndentChars.toFloat())).sp),
                lineHeight = (fontSize * lineSpacing).sp,
                platformStyle = PlatformTextStyle(includeFontPadding = false),
                lineHeightStyle = LineHeightStyle(
                    alignment = LineHeightStyle.Alignment.Center,
                    trim = LineHeightStyle.Trim.None
                )
            )
            
            val idx = blockIndex
            DisposableEffect(idx) {
                onDispose {
                    if (idx != null) {
                        blockLayoutRegistry?.remove(idx)
                    }
                }
            }
            var layoutResult by remember { mutableStateOf<TextLayoutResult?>(null) }
            
            Text(
                text = annotated,
                style = baseStyle,
                modifier = Modifier
                    .fillMaxWidth()
                    .run { if (idx != null && idx == ttsCurrentBlock) background(androidx.compose.ui.graphics.Color(0x4DFFEB3B), androidx.compose.foundation.shape.RoundedCornerShape(8.dp)) else this }
                    .padding(top=(fontSize*(native.before?:paraSpacing)).dp,bottom=(fontSize*paraSpacing).dp)
                    .onGloballyPositioned { coordinates ->
                        val lr = layoutResult
                        if (idx != null && lr != null) {
                            blockLayoutRegistry?.set(idx, BlockLayoutInfo(annotated.text, lr, coordinates, sourceCharOffset))
                        }
                    }
                    .drawWithContent {
                        drawContent()
                        val lr = layoutResult ?: return@drawWithContent
                        drawPublicationRuby(lr,native,sourceCharOffset,fontSize,textColor)
                        // Ranges are projected once from source scalars into this fragment's UTF-16 layout.
                        for ((range, color) in projectedHighlights) {
                            if (range.end <= annotated.length) {
                                val hlPath = lr.multiParagraph.getPathForRange(range.start, range.end)
                                drawPath(hlPath, color = highlightColor(color))
                            }
                        }
                        // 渲染 CSC 纠错
                        if (cscBlockCorrections.isNotEmpty()) {
                            android.util.Log.d("CscRender", "block=$idx mode=$cscMode corrections=${cscBlockCorrections.size}")
                        }
                        for (csc in cscBlockCorrections) {
                            if (csc.status != CorrectionStatus.ACCEPTED && csc.status != CorrectionStatus.IGNORED) {
                                val cscStart = csc.localOffset - sourceCharOffset
                                val cscEnd = (cscStart + csc.original.length).coerceAtMost(annotated.length)
                                if (cscStart in 0 until annotated.length && cscStart < cscEnd) {
                                    android.util.Log.d("CscRender", "draw block=$idx mode=$cscMode start=$cscStart end=$cscEnd orig=${csc.original} corr=${csc.corrected} status=${csc.status}")
                                    if (cscMode == "readonly") {
                                        drawCscRubyAnnotation(lr, cscStart, csc.original, csc.corrected, textMeasurer, fontSize, bgColor)
                                    } else {
                                        drawCscWavyUnderline(lr, cscStart, cscEnd)
                                    }
                                }
                            }
                        }
                        // 渲染实时选区
                        val selState = textSelection
                        if (selState != null && idx != null && idx in selState.startBlock..selState.endBlock) {
                            val localStart = if (idx == selState.startBlock) selState.startChar - sourceCharOffset else 0
                            val localEnd = if (idx == selState.endBlock) selState.endChar - sourceCharOffset else annotated.length
                            if (localStart < localEnd && localStart >= 0 && localEnd <= annotated.length) {
                                val selPath = lr.multiParagraph.getPathForRange(localStart, localEnd)
                                drawPath(selPath, color = Color(0x4266D3FF))
                            }
                        }
                    }
                    .pointerInput(block, onLinkClick, cscMode, cscBlockCorrections) {
                        awaitEachGesture {
                            val down = awaitFirstDown(pass = PointerEventPass.Initial, requireUnconsumed = false)
                            val up = waitForUpOrCancellation(pass = PointerEventPass.Initial) ?: return@awaitEachGesture
                            val isQuickTap = (up.uptimeMillis - down.uptimeMillis) <= 220L
                            val isSmallMove = (up.position - down.position).getDistance() <= viewConfiguration.touchSlop
                            if (isQuickTap && isSmallMove) {
                                layoutResult?.let { result ->
                                    val offset = result.getOffsetForPosition(up.position)
                                    // CSC readwrite 模式：点击纠错区域弹出菜单
                                    if (cscMode == "readwrite") {
                                        val tappedCsc = cscBlockCorrections.firstOrNull { csc ->
                                            csc.status != CorrectionStatus.ACCEPTED && csc.status != CorrectionStatus.IGNORED &&
                                            offset + sourceCharOffset >= csc.localOffset &&
                                                offset + sourceCharOffset < csc.localOffset + csc.original.length
                                        }
                                        if (tappedCsc != null) {
                                            onCscCorrectionClick(tappedCsc, up.position)
                                            down.consume()
                                            up.consume()
                                            return@awaitEachGesture
                                        }
                                    }
                                    val startOffset = (offset - 2).coerceAtLeast(0)
                                    val endOffset = (offset + 2).coerceAtMost(annotated.length)
                                    val annotations = annotated.getStringAnnotations(tag = "URL", start = startOffset, end = endOffset)
                                    if (annotations.isNotEmpty()) {
                                        onLinkClick(annotations.last().item)
                                        down.consume()
                                        up.consume()
                                    } else {
                                        // 普通文本点击交给页面级手势处理，避免正文块把整页点击吃掉
                                    }
                                }
                            }
                        }
                    },
                onTextLayout = { layoutResult = it }
            )
        }

        is ContentBlock.Image -> {
            var decodeFinished by remember(block.data) { mutableStateOf(false) }
            val bitmap by produceState(initialValue = ReaderImageCache.peek(block.data), block.data) {
                value = withContext(Dispatchers.IO) {
                    ReaderImageCache.load(block.data)
                }
                decodeFinished = true
            }
            val natural = remember(block.data) { ReaderImageCache.dimensions(block.data) }
            val visibleImages=LocalVisibleReaderImages.current
            val imageKey=remember(block.data) { Any() }
            val imagePrepared=bitmap!=null || decodeFinished
            SideEffect { visibleImages?.prepared(imageKey,imagePrepared) }
            DisposableEffect(visibleImages,imageKey) { onDispose { visibleImages?.remove(imageKey) } }
            if (natural != null) {
                BoxWithConstraints(Modifier.fillMaxWidth().padding(vertical = (fontSize * 0.35f).dp), contentAlignment = androidx.compose.ui.Alignment.Center) {
                    val boundHeight = if (imageMaxHeight == Dp.Unspecified) Float.POSITIVE_INFINITY else imageMaxHeight.value
                    val (width,height)=native.imageSize(natural.first.toFloat(),natural.second.toFloat(),fontSize,maxWidth.value,boundHeight)
                    val frame = Modifier.size(width.dp,height.dp).onGloballyPositioned { visibleImages?.position(imageKey,it.boundsInRoot()) }
                    if(bitmap != null) Image(bitmap=bitmap!!.asImageBitmap(),contentDescription=block.alt,modifier=frame,contentScale=ContentScale.Fit)
                    else Box(frame.background(textColor.copy(alpha=0.04f)),contentAlignment=androidx.compose.ui.Alignment.Center) {
                        if(decodeFinished)Text(block.alt?:I18n.t("reader.image_load_failed"),color=textColor.copy(alpha=0.65f),fontSize=(fontSize*0.9f).sp)
                    }
                }
            } else {
                Text(text=block.alt?:I18n.t("reader.image_load_failed"),color=textColor.copy(alpha=0.65f),fontSize=(fontSize*0.9f).sp,
                    modifier=Modifier.padding(vertical=(fontSize*0.35f).dp))
            }
        }

        is ContentBlock.Separator -> {
            HorizontalDivider(
                modifier = Modifier.padding(vertical = (fontSize * 0.5f).dp),
                color = textColor.copy(alpha = 0.2f)
            )
        }

        is ContentBlock.BlankLine -> {
            Spacer(Modifier.height((fontSize * 0.5f).dp))
        }
    }
}

private fun highlightColor(name: String): Color = when (name) {
    "Yellow" -> Color(0x59FFF176)
    "Green"  -> Color(0x59A5D6A7)
    "Blue"   -> Color(0x5990CAF9)
    "Pink"   -> Color(0x59F48FB1)
    else     -> Color(0x59FFF176)
}

private fun DrawScope.drawCscWavyUnderline(
    layoutResult: TextLayoutResult,
    startOffset: Int,
    endOffset: Int,
    color: Color = Color.Red,
    waveAmplitude: Float = 2.5f,
    waveLength: Float = 8f,
    strokeWidth: Float = 1.5f
) {
    for (i in startOffset until endOffset) {
        val rect: Rect = layoutResult.getBoundingBox(i)
        if (i == startOffset || rect.left <= layoutResult.getBoundingBox(i - 1).left) {
            // Start of a new visual line segment
            val segEnd = (i until endOffset).lastOrNull { j ->
                val r = layoutResult.getBoundingBox(j)
                r.top == rect.top
            } ?: i
            val lineLeft = rect.left
            val lineRight = layoutResult.getBoundingBox(segEnd).right
            val baselineY = rect.bottom - 1f

            val path = Path()
            var x = lineLeft
            path.moveTo(x, baselineY)
            var up = true
            while (x < lineRight) {
                val nextX = (x + waveLength / 2).coerceAtMost(lineRight)
                val peakY = if (up) baselineY - waveAmplitude else baselineY + waveAmplitude
                path.quadraticTo((x + nextX) / 2, peakY, nextX, baselineY)
                x = nextX
                up = !up
            }
            drawPath(path, color = color, style = Stroke(width = strokeWidth))
        }
    }
}

private fun DrawScope.drawCscRubyAnnotation(
    layoutResult: TextLayoutResult,
    startOffset: Int,
    originalText: String,
    correctedText: String,
    textMeasurer: androidx.compose.ui.text.TextMeasurer,
    fontSize: Float,
    bgColor: Color
) {
    // 计算原文完整区域
    val endOffset = (startOffset + originalText.length - 1).coerceAtMost(layoutResult.layoutInput.text.length - 1)
    val firstRect = layoutResult.getBoundingBox(startOffset)
    val lastRect = layoutResult.getBoundingBox(endOffset)
    val fullRect = Rect(firstRect.left, firstRect.top, lastRect.right, lastRect.bottom)

    // 用背景色覆盖原文
    drawRect(bgColor, topLeft = Offset(fullRect.left, fullRect.top), size = androidx.compose.ui.geometry.Size(fullRect.width, fullRect.height))

    // 绘制纠正后的文本（替换原文位置）
    val correctedMeasured = textMeasurer.measure(
        text = correctedText,
        style = TextStyle(fontSize = fontSize.sp, color = Color(0xFF1976D2))
    )
    val correctedX = fullRect.left + (fullRect.width - correctedMeasured.size.width) / 2f
    val correctedY = fullRect.top + (fullRect.height - correctedMeasured.size.height) / 2f
    drawText(correctedMeasured, topLeft = Offset(correctedX, correctedY))

    // 原文作为小字 Ruby 标注在上方（居中，灰色）
    val rubyFontSize = (fontSize * 0.45f).sp
    val rubyMeasured = textMeasurer.measure(
        text = originalText,
        style = TextStyle(fontSize = rubyFontSize, color = Color.Gray)
    )
    val rubyX = fullRect.left + (fullRect.width - rubyMeasured.size.width) / 2f
    val rubyY = fullRect.top - rubyMeasured.size.height.toFloat() * 0.35f
    drawText(rubyMeasured, topLeft = Offset(rubyX, rubyY))
}

private fun androidx.compose.ui.graphics.drawscope.DrawScope.drawPublicationRuby(layout:TextLayoutResult,native:com.zhongbai233.epub.reader.model.NativeStyle,base:Int,font:Float,color:Color) {
    for(ruby in native.ruby) {
        val start=ruby.start-base;val end=ruby.end-base
        if(start<0||end>layout.layoutInput.text.length||end<=start)continue
        val first=layout.getBoundingBox(start);val last=layout.getBoundingBox(end-1)
        val width=if(first.top==last.top)last.right-first.left else first.width*(end-start)
        val baseScale=native.inline.lastOrNull {it.start<=ruby.start&&it.end>=ruby.end}?.size?:1f
        val basePx=font.sp.toPx()*baseScale
        with(drawContext.canvas.nativeCanvas) {val paint=android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG);paint.textSize=basePx*0.5f;paint.color=color.toArgb();val measured=paint.measureText(ruby.text);val extent=width+basePx;if(measured>extent&&extent>0)paint.textSize*=extent/measured;val baseline=layout.getLineBaseline(layout.getLineForOffset(start))-basePx-paint.fontMetrics.descent-1f;drawText(ruby.text,first.left+(width-paint.measureText(ruby.text))*0.5f,baseline,paint)}
    }
}
