/**
 * TextRenderUtils.kt — 文本渲染工具函数
 *
 * 从 ReaderScreen.kt 中拆分出来的纯函数，负责:
 * - 构建 AnnotatedString（粗体、斜体、链接等内联样式）
 * - 规范化 EPUB 内部 href 链接
 */
package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.LinkInteractionListener
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.sp
import com.zhongbai233.epub.reader.model.InlineStyle
import com.zhongbai233.epub.reader.model.TextSpan
import java.net.URI

/**
 * 构建 AnnotatedString，支持粗体、斜体、链接等内联样式
 */
internal fun buildSpanAnnotatedString(
    spans: List<TextSpan>,
    fontSize: Float,
    textColor: Color,
    linkColor: Color,
    fontFamily: FontFamily,
    overrideWeight: FontWeight?,
    onLinkClick: ((String) -> Unit)? = null
): AnnotatedString {
    return buildAnnotatedString {
        for (span in spans) {
            val weight = when (span.style) {
                InlineStyle.Bold, InlineStyle.BoldItalic -> FontWeight.Bold
                else -> overrideWeight ?: FontWeight.Normal
            }
            val fontStyle = when (span.style) {
                InlineStyle.Italic, InlineStyle.BoldItalic -> FontStyle.Italic
                else -> FontStyle.Normal
            }
            val color = if (span.linkUrl != null) linkColor else textColor

            val start = length
            withStyle(
                SpanStyle(
                    fontSize = fontSize.sp,
                    fontFamily = fontFamily,
                    fontWeight = weight,
                    fontStyle = fontStyle,
                    color = color
                )
            ) {
                append(span.text)
            }
            val end = length
            val url = span.linkUrl
            if (url != null && end > start) {
                // Compose 1.7 LinkAnnotation
                addLink(
                    LinkAnnotation.Clickable(
                        tag = url,
                        linkInteractionListener = if (onLinkClick != null) LinkInteractionListener { onLinkClick(url) } else null
                    ),
                    start,
                    end
                )
                // 兼容 SelectionContainer 的手动捕获
                addStringAnnotation(
                    tag = "URL",
                    annotation = url,
                    start = start,
                    end = end
                )
            }
        }
    }
}

internal fun normalizeInternalHref(raw: String): String {
    val clean = raw.trim().substringBefore('#').trim()
    if (clean.isBlank()) return ""
    val withoutScheme = runCatching {
        val uri = URI(clean)
        if (uri.scheme != null) {
            (uri.path ?: "").trim('/').removePrefix("./")
        } else {
            clean
        }
    }.getOrDefault(clean)

    return withoutScheme
        .trim()
        .removePrefix("./")
        .trim('/')
}

internal fun extractAnchorFromHref(raw: String): String? {
    val hashPos = raw.indexOf('#')
    return if (hashPos >= 0) raw.substring(hashPos + 1).trim() else null
}
