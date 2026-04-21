package com.zhongbai233.epub.reader.ui.reader

import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.layout.*
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalViewConfiguration
import androidx.compose.ui.text.PlatformTextStyle
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.text.style.TextIndent
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.model.*
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

// ─── 内容块渲染 ───

@Composable
internal fun ContentBlockView(
    blockIndex: Int?,
    block: ContentBlock,
    fontSize: Float,
    textColor: Color,
    linkColor: Color,
    fontFamily: FontFamily,
    onLinkClick: (String) -> Unit,
    onTextTapped: () -> Unit,
    lineSpacing: Float = 1.5f,
    paraSpacing: Float = 0.5f,
    textIndentChars: Int = 2,
    textSelection: TextSelectionState? = null,
    onSelectionChange: (TextSelectionState?) -> Unit = {},
    blockLayoutRegistry: MutableMap<Int, BlockLayoutInfo>? = null,
    highlights: List<HighlightDto> = emptyList()
) {
    val viewConfiguration = LocalViewConfiguration.current
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
                    .pointerInput(annotated, onLinkClick) {
                        awaitEachGesture {
                            val down = awaitFirstDown(pass = PointerEventPass.Initial, requireUnconsumed = false)
                            val up = waitForUpOrCancellation(pass = PointerEventPass.Initial) ?: return@awaitEachGesture
                            val isQuickTap = (up.uptimeMillis - down.uptimeMillis) <= 220L
                            val isSmallMove = (up.position - down.position).getDistance() <= viewConfiguration.touchSlop
                            if (isQuickTap && isSmallMove) {
                                layoutResult?.let { result ->
                                    val url = result.findUrlAtPosition(up.position, annotated, extraPaddingPx = 30f)
                                    if (url != null) {
                                        onLinkClick(url)
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
                    .pointerInput(annotated, onLinkClick) {
                        awaitEachGesture {
                            val down = awaitFirstDown(pass = PointerEventPass.Initial, requireUnconsumed = false)
                            val up = waitForUpOrCancellation(pass = PointerEventPass.Initial) ?: return@awaitEachGesture
                            val isQuickTap = (up.uptimeMillis - down.uptimeMillis) <= 220L
                            val isSmallMove = (up.position - down.position).getDistance() <= viewConfiguration.touchSlop
                            if (isQuickTap && isSmallMove) {
                                layoutResult?.let { result ->
                                    val url = result.findUrlAtPosition(up.position, annotated, extraPaddingPx = 30f)
                                    if (url != null) {
                                        onLinkClick(url)
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

/**
 * 在 TextLayoutResult 中查找点击位置是否落在某个 URL 链接的扩展边界框内。
 * 先尝试精确字符匹配，失败后再检查链接边界框 ±[extraPaddingPx] 的扩展区域。
 */
private fun TextLayoutResult.findUrlAtPosition(
    position: androidx.compose.ui.geometry.Offset,
    annotated: AnnotatedString,
    extraPaddingPx: Float = 30f
): String? {
    // 先精确匹配
    val exactOffset = getOffsetForPosition(position)
    annotated.getStringAnnotations(tag = "URL", start = exactOffset, end = exactOffset)
        .firstOrNull()?.let { return it.item }

    // 再检查扩展边界框
    val all = annotated.getStringAnnotations(tag = "URL", start = 0, end = annotated.length)
    for (ann in all) {
        val startBox = getBoundingBox(ann.start)
        val endBox = getBoundingBox((ann.end - 1).coerceAtLeast(ann.start))
        val left = minOf(startBox.left, endBox.left) - extraPaddingPx
        val top = minOf(startBox.top, endBox.top) - extraPaddingPx
        val right = maxOf(startBox.right, endBox.right) + extraPaddingPx
        val bottom = maxOf(startBox.bottom, endBox.bottom) + extraPaddingPx
        if (position.x in left..right && position.y in top..bottom) {
            return ann.item
        }
    }
    return null
}
