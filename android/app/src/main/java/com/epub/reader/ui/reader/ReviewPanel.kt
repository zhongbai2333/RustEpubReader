package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.model.ContentBlock
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * 解析段评文本为结构化数据
 * 格式: "1. 【内容】 作者：xxx | 时间：xxx | 赞：52"
 */
private data class ReviewCard(
    val index: Int,
    val content: String,
    val author: String,
    val timestamp: String,
    val likes: Int
)

private fun parseReviewCard(text: String): ReviewCard? {
    val trimmed = text.trim()
    val dotPos = trimmed.indexOf('.')
    if (dotPos <= 0) return null
    val index = trimmed.substring(0, dotPos).trim().toIntOrNull() ?: return null
    val rest = trimmed.substring(dotPos + 1).trim()

    val parts = rest.split(Regex("[|｜]"))
    if (parts.size < 3) return null

    // Find likes — last part containing 赞:
    val likesPart = parts.last().trim()
    val likesStr = likesPart.removePrefix("赞：").removePrefix("赞:").trim()
    val likes = likesStr.toIntOrNull() ?: return null

    // Find timestamp — second-to-last part containing 时间:
    val timePart = parts[parts.size - 2].trim()
    val timestampRaw = timePart.removePrefix("时间：").removePrefix("时间:").trim()
    if (timestampRaw == timePart) return null  // neither prefix found

    // Find author and content — search all parts via regex to tolerate varied positions
    val authorRegex = Regex("""(.*)作者[:：]\s*(.+)""")
    var content = ""
    var author: String? = null
    for (part in parts) {
        val m = authorRegex.find(part.trim()) ?: continue
        content = m.groupValues[1].trim()
        author = m.groupValues[2].trim()
        break
    }
    if (author == null) return null

    return ReviewCard(index, content, author, timestampRaw, likes)
}

private fun formatTimestamp(s: String): String {
    val ts = s.toLongOrNull() ?: return s
    return try {
        val sdf = SimpleDateFormat("yyyy-MM-dd HH:mm", Locale.getDefault())
        sdf.format(Date(ts * 1000))
    } catch (_: Exception) {
        s
    }
}

/**
 * 段评面板 — 底部弹层展示段评内容
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ReviewPanel(
    chapterTitle: String,
    blocks: List<ContentBlock>,
    anchorId: String? = null,
    fontSize: Float = 16f,
    onDismiss: () -> Unit
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)

    // 是否显示全部评论（当从锚点打开时，默认只显示当前段）
    var showAll by remember(anchorId) { mutableStateOf(anchorId.isNullOrBlank()) }

    // 根据 anchorId 和 showAll 筛选 blocks，同时去除重复的 "第X章 … - 段评" 标题
    val filteredBlocks = remember(blocks, anchorId, showAll) {
        val baseList = if (anchorId.isNullOrBlank() || showAll) {
            blocks
        } else {
            val result = mutableListOf<ContentBlock>()
            var inTargetSection = false
            for (block in blocks) {
                if (block is ContentBlock.Heading && block.anchorId == anchorId) {
                    inTargetSection = true
                    result.add(block)
                    continue
                }
                if (block is ContentBlock.Heading && !block.anchorId.isNullOrBlank() && block.anchorId != anchorId) {
                    inTargetSection = false
                    continue
                }
                if (inTargetSection) {
                    result.add(block)
                }
            }
            if (result.isEmpty()) blocks else result
        }
        // 去重：每个 "第X章 … - 段评" 标题只保留首次出现
        val seenTitles = mutableSetOf<String>()
        baseList.filter { block ->
            if (block is ContentBlock.Heading) {
                val text = block.spans.joinToString("") { it.text }
                if (text.endsWith(" - 段评")) seenTitles.add(text) else true
            } else true
        }
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        dragHandle = { BottomSheetDefaults.DragHandle() }
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .fillMaxHeight(0.80f)
                .padding(horizontal = 16.dp)
        ) {
            // Header
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Text(
                    I18n.t("review.panel_title"),
                    fontSize = 18.sp,
                    fontWeight = FontWeight.Bold
                )
                IconButton(onClick = onDismiss) {
                    Icon(
                        Icons.Default.Close,
                        contentDescription = I18n.t("dialog.close")
                    )
                }
            }

            Text(
                chapterTitle,
                fontSize = 13.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(bottom = 4.dp)
            )

            // 显示全部 / 只看当前段 切换按钮
            if (!anchorId.isNullOrBlank()) {
                TextButton(
                    onClick = { showAll = !showAll },
                    modifier = Modifier.padding(vertical = 2.dp)
                ) {
                    Text(
                        if (showAll) "🔍 只看当前段" else "📖 显示全部",
                        fontSize = 13.sp
                    )
                }
            }

            HorizontalDivider()
            Spacer(Modifier.height(8.dp))

            // Content
            LazyColumn {
                itemsIndexed(filteredBlocks, key = { index, _ -> index }) { _, block ->
                    when (block) {
                        is ContentBlock.Separator -> {
                            HorizontalDivider(
                                modifier = Modifier.padding(vertical = 6.dp),
                                color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.3f)
                            )
                        }
                        is ContentBlock.BlankLine -> {
                            Spacer(Modifier.height((fontSize * 0.4).dp))
                        }
                        is ContentBlock.Heading -> {
                            val text = block.spans.joinToString("") { it.text }
                            val size = when (block.level) {
                                1 -> 17.sp
                                2 -> 15.sp
                                else -> 13.sp
                            }
                            Text(
                                text = text,
                                fontSize = size,
                                fontWeight = FontWeight.Bold,
                                modifier = Modifier.padding(vertical = 6.dp),
                                color = MaterialTheme.colorScheme.onSurface
                            )
                        }
                        is ContentBlock.Paragraph -> {
                            val rawText = block.spans.joinToString("") { it.text }
                            if (rawText.isBlank()) return@itemsIndexed

                            val card = parseReviewCard(rawText)
                            if (card != null) {
                                // 结构化评论卡片
                                ReviewCardItem(card = card, fontSize = fontSize)
                            } else {
                                // 普通文本（如"回到正文"链接）
                                val formattedText = formatReviewTextFallback(rawText)
                                OutlinedCard(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .padding(vertical = 3.dp),
                                    colors = CardDefaults.cardColors(
                                        containerColor = MaterialTheme.colorScheme.surfaceContainerLow
                                    ),
                                    shape = MaterialTheme.shapes.medium,
                                    border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)
                                ) {
                                    Text(
                                        text = formattedText,
                                        fontSize = fontSize.sp,
                                        modifier = Modifier.padding(12.dp),
                                        color = MaterialTheme.colorScheme.onSurface
                                    )
                                }
                            }
                        }
                        is ContentBlock.Image -> {
                            if (!block.alt.isNullOrBlank()) {
                                Text(
                                    text = block.alt,
                                    fontSize = (fontSize * 0.9f).sp,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.65f),
                                    modifier = Modifier.padding(vertical = 4.dp)
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun ReviewCardItem(card: ReviewCard, fontSize: Float) {
    OutlinedCard(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerLow
        ),
        shape = MaterialTheme.shapes.medium,
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)
    ) {
        Column(modifier = Modifier.padding(12.dp)) {
            // 作者名
            Text(
                text = card.author,
                fontSize = (fontSize * 0.85f).sp,
                fontWeight = FontWeight.Bold,
                color = MaterialTheme.colorScheme.primary
            )
            Spacer(Modifier.height(4.dp))
            // 评论内容
            Text(
                text = card.content,
                fontSize = fontSize.sp,
                color = MaterialTheme.colorScheme.onSurface
            )
            Spacer(Modifier.height(6.dp))
            // 时间和赞数
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Text(
                    text = formatTimestamp(card.timestamp),
                    fontSize = (fontSize * 0.75f).sp,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
                Text(
                    text = "❤ ${card.likes}",
                    fontSize = (fontSize * 0.75f).sp,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }
        }
    }
}

/**
 * 兜底格式化：把文本中的 Unix 时间戳转换为可读日期（用于非结构化文本）
 */
private fun formatReviewTextFallback(text: String): String {
    val regex = """时间[:：]\s*(\d{10})""".toRegex()
    return regex.replace(text) { matchResult ->
        val ts = matchResult.groupValues[1].toLongOrNull()
        if (ts != null) {
            try {
                val sdf = SimpleDateFormat("yyyy-MM-dd HH:mm", Locale.getDefault())
                val dateStr = sdf.format(Date(ts * 1000))
                "时间：${dateStr}"
            } catch (_: Exception) {
                matchResult.value
            }
        } else {
            matchResult.value
        }
    }
}
