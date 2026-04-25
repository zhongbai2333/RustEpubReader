package com.zhongbai233.epub.reader.ui.reader

import android.graphics.BitmapFactory
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
    blockIndex: Int?,
    block: ContentBlock,
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
    textSelection: TextSelectionState? = null,
    onSelectionChange: (TextSelectionState?) -> Unit = {},
    blockLayoutRegistry: MutableMap<Int, BlockLayoutInfo>? = null,
    highlights: List<HighlightDto> = emptyList(),
    cscBlockCorrections: List<CscBlockCorrection> = emptyList(),
    cscMode: String = "none",
    ttsCurrentBlock: Int = -1,
    onCscCorrectionClick: (CscBlockCorrection, androidx.compose.ui.geometry.Offset) -> Unit = { _, _ -> }
) {
    val viewConfiguration = LocalViewConfiguration.current
    val textMeasurer = rememberTextMeasurer()
    when (block) {
        is ContentBlock.Heading -> {
            val scale = when (block.level) {
                1 -> 2.0f
                2 -> 1.6f
                3 -> 1.3f
                else -> 1.2f
            }
            val annotated = buildSpanAnnotatedString(
                spans = block.spans,
                fontSize = fontSize * scale,
                textColor = textColor,
                linkColor = linkColor,
                fontFamily = fontFamily,
                overrideWeight = FontWeight.Bold,
                onLinkClick = onLinkClick
            )
            val headingStyle = TextStyle(
                fontFamily = fontFamily,
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
                    .run { if (idx != null && idx == ttsCurrentBlock) background(androidx.compose.ui.graphics.Color(0x4DFFEB3B), androidx.compose.foundation.shape.RoundedCornerShape(8.dp)) else this }
                    .padding(top = (fontSize * 1.2f).dp, bottom = (fontSize * 1.8f).dp)
                    .onGloballyPositioned { coordinates ->
                        val lr = layoutResult
                        if (idx != null && lr != null) {
                            blockLayoutRegistry?.set(idx, BlockLayoutInfo(annotated.text, lr, coordinates))
                        }
                    }
                    .drawWithContent {
                        drawContent()
                        val lr = layoutResult ?: return@drawWithContent
                        // 渲染已保存的高亮
                        if (idx != null) {
                            for (hl in highlights) {
                                if (idx in hl.startBlock..hl.endBlock) {
                                    val hlStart = if (idx == hl.startBlock) hl.startOffset else 0
                                    val hlEnd = if (idx == hl.endBlock) hl.endOffset else annotated.length
                                    if (hlStart < hlEnd && hlStart >= 0 && hlEnd <= annotated.length) {
                                        val hlPath = lr.multiParagraph.getPathForRange(hlStart, hlEnd)
                                        drawPath(hlPath, color = highlightColor(hl.color))
                                    }
                                }
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
                            val localStart = if (idx == selState.startBlock) selState.startChar else 0
                            val localEnd = if (idx == selState.endBlock) selState.endChar else annotated.length
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
                                        onLinkClick(annotations.first().item)
                                    } else {
                                        onTextTapped()
                                    }
                                } ?: onTextTapped()
                                down.consume()
                                up.consume()
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
                onLinkClick = onLinkClick
            )
            val baseStyle = TextStyle(
                fontFamily = fontFamily,
                textIndent = TextIndent(firstLine = (fontSize * textIndentChars).sp),
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
                    .run { if (idx != null && idx == ttsCurrentBlock) background(androidx.compose.ui.graphics.Color(0x4DFFEB3B), androidx.compose.foundation.shape.RoundedCornerShape(8.dp)) else this }
                    .padding(vertical = (fontSize * paraSpacing).dp)
                    .onGloballyPositioned { coordinates ->
                        val lr = layoutResult
                        if (idx != null && lr != null) {
                            blockLayoutRegistry?.set(idx, BlockLayoutInfo(annotated.text, lr, coordinates))
                        }
                    }
                    .drawWithContent {
                        drawContent()
                        val lr = layoutResult ?: return@drawWithContent
                        // 渲染已保存的高亮
                        if (idx != null) {
                            for (hl in highlights) {
                                if (idx in hl.startBlock..hl.endBlock) {
                                    val hlStart = if (idx == hl.startBlock) hl.startOffset else 0
                                    val hlEnd = if (idx == hl.endBlock) hl.endOffset else annotated.length
                                    if (hlStart < hlEnd && hlStart >= 0 && hlEnd <= annotated.length) {
                                        val hlPath = lr.multiParagraph.getPathForRange(hlStart, hlEnd)
                                        drawPath(hlPath, color = highlightColor(hl.color))
                                    }
                                }
                            }
                        }
                        // 渲染 CSC 纠错
                        if (cscBlockCorrections.isNotEmpty()) {
                            android.util.Log.d("CscRender", "block=$idx mode=$cscMode corrections=${cscBlockCorrections.size}")
                        }
                        for (csc in cscBlockCorrections) {
                            if (csc.status != CorrectionStatus.ACCEPTED && csc.status != CorrectionStatus.IGNORED) {
                                val cscStart = csc.localOffset
                                val cscEnd = (csc.localOffset + csc.original.length).coerceAtMost(annotated.length)
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
                            val localStart = if (idx == selState.startBlock) selState.startChar else 0
                            val localEnd = if (idx == selState.endBlock) selState.endChar else annotated.length
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
                                        onLinkClick(annotations.first().item)
                                    } else {
                                        onTextTapped()
                                    }
                                } ?: onTextTapped()
                                down.consume()
                                up.consume()
                            }
                        }
                    },
                onTextLayout = { layoutResult = it }
            )
        }

        is ContentBlock.Image -> {
            val bitmap by produceState<android.graphics.Bitmap?>(initialValue = null, block.data) {
                value = withContext(Dispatchers.IO) {
                    val bytes = android.util.Base64.decode(block.data, android.util.Base64.DEFAULT)
                    BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
                }
            }
            val bmp = bitmap
            if (bmp != null && bmp.width > 0 && bmp.height > 0) {
                val ratio = bmp.width.toFloat() / bmp.height.toFloat()
                Image(
                    bitmap = bmp.asImageBitmap(),
                    contentDescription = block.alt,
                    modifier = Modifier
                        .fillMaxWidth()
                        .aspectRatio(ratio)
                        .padding(vertical = (fontSize * 0.35f).dp),
                    contentScale = ContentScale.Fit
                )
            } else {
                Text(
                    text = block.alt ?: I18n.t("reader.image_load_failed"),
                    color = textColor.copy(alpha = 0.65f),
                    fontSize = (fontSize * 0.9f).sp,
                    modifier = Modifier.padding(vertical = (fontSize * 0.35f).dp)
                )
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
