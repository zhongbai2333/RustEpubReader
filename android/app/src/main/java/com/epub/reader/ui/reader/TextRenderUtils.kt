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
import com.zhongbai233.epub.reader.model.NativeStyle
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
    onLinkClick: ((String) -> Unit)? = null,
    native: NativeStyle = NativeStyle(),
    sourceCharOffset: Int = 0
, latinFamily: FontFamily? = null): AnnotatedString {
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
        if(latinFamily != null) {
            val value=toAnnotatedString().text;var start=-1;var i=0
            while(i<value.length){val cp=value.codePointAt(i);val latin=Character.UnicodeScript.of(cp)==Character.UnicodeScript.LATIN || (cp in 0x21..0x7e)
                if(latin && start<0)start=i
                if(!latin && start>=0){addStyle(SpanStyle(fontFamily=latinFamily),start,i);start=-1}
                i+=Character.charCount(cp)
            }
            if(start>=0)addStyle(SpanStyle(fontFamily=latinFamily),start,value.length)
        }
        for(item in native.inline) {
            val start=(item.start-sourceCharOffset).coerceIn(0,length);val end=(item.end-sourceCharOffset).coerceIn(0,length);if(end<=start)continue
            val family=when(item.family){"serif"->FontFamily.Serif;"sans-serif"->FontFamily.SansSerif;"monospace"->FontFamily.Monospace;else->null}
            val ink=if(item.hidden)Color.Transparent else item.color?.let {runCatching {Color(android.graphics.Color.parseColor(it))}.getOrNull()}?:Color.Unspecified
            addStyle(SpanStyle(fontFeatureSettings=if(item.smallCaps)"'smcp' 1" else "'smcp' 0",fontSize=if(item.hidden)0.01.sp else ((item.size?:1f)*fontSize).sp,color=ink,fontFamily=family,fontWeight=item.bold?.let {if(it)FontWeight.Bold else FontWeight.Normal},fontStyle=item.italic?.let {if(it)FontStyle.Italic else FontStyle.Normal},baselineShift=if(item.baseline!=0f)androidx.compose.ui.text.style.BaselineShift(item.baseline)else null,textDecoration=when{item.underline&&item.strike->androidx.compose.ui.text.style.TextDecoration.combine(listOf(androidx.compose.ui.text.style.TextDecoration.Underline,androidx.compose.ui.text.style.TextDecoration.LineThrough));item.underline->androidx.compose.ui.text.style.TextDecoration.Underline;item.strike->androidx.compose.ui.text.style.TextDecoration.LineThrough;else->null}),start,end)
            item.note?.let {href->val url="reader-note:$href";addLink(LinkAnnotation.Clickable(url,linkInteractionListener=onLinkClick?.let {callback->LinkInteractionListener {callback(url)}}),start,end);addStringAnnotation("URL",url,start,end)}
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

internal fun mixedScriptText(text:String,family:FontFamily,latinFamily:FontFamily?):AnnotatedString = buildAnnotatedString {
    append(text)
    if(latinFamily!=null){var start=-1;var i=0;while(i<text.length){val cp=text.codePointAt(i);val latin=Character.UnicodeScript.of(cp)==Character.UnicodeScript.LATIN || cp in 0x21..0x7e
        if(latin&&start<0)start=i
        if(!latin&&start>=0){addStyle(SpanStyle(fontFamily=latinFamily),start,i);start=-1};i+=Character.charCount(cp)
    };if(start>=0)addStyle(SpanStyle(fontFamily=latinFamily),start,text.length)}
}
