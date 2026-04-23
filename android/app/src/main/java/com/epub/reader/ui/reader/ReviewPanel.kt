package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.model.ContentBlock

/** Parsed review card data */
private data class ReviewCard(
    val index: Int,
    val content: String,
    val author: String,
    val timestamp: String,
    val likes: Int
)

/** Format Unix timestamp to readable date string */
private fun formatTimestamp(raw: String): String {
    val ts = raw.toLongOrNull() ?: return raw
    return try {
        val sdf = java.text.SimpleDateFormat("yyyy-MM-dd HH:mm", java.util.Locale.getDefault())
        sdf.format(java.util.Date(ts * 1000))
    } catch (_: Exception) {
        raw
    }
}

/**
 * Try to parse a review paragraph into structured card data.
 * Expected format: "1. 【内容】 作者：xxx | 时间：xxx | 赞：52"
 */
private fun parseReviewCard(text: String): ReviewCard? {
    val trimmed = text.trim()

    // Extract index: "1. ..."
    val dotPos = trimmed.indexOf('.')
    if (dotPos <= 0) return null
    val index = trimmed.substring(0, dotPos).trim().toIntOrNull() ?: return null
    val rest = trimmed.substring(dotPos + 1).trim()

    // Parse from the end to avoid | inside review content causing offset
    // Step 1: find "赞：xx" at the end
    val likesDelims = listOf(" | 赞：", " | 赞:", " ｜ 赞：", " ｜ 赞:", "|赞：", "|赞:", "｜赞：", "｜赞:")
    var likesPart: String? = null
    var beforeLikes: String? = null
    for (delim in likesDelims) {
        val pos = rest.lastIndexOf(delim)
        if (pos >= 0) {
            likesPart = rest.substring(pos + delim.length).trim()
            beforeLikes = rest.substring(0, pos)
            break
        }
    }
    if (likesPart == null) return null
    val likes = likesPart.toIntOrNull() ?: return null

    // Step 2: find "时间：xxx" before likes
    val timeDelims = listOf(" | 时间：", " | 时间:", " ｜ 时间：", " ｜ 时间:", "|时间：", "|时间:", "｜时间：", "｜时间:")
    var timestampRaw: String? = null
    var contentAuthorPart: String? = null
    for (delim in timeDelims) {
        val pos = beforeLikes!!.lastIndexOf(delim)
        if (pos >= 0) {
            timestampRaw = beforeLikes.substring(pos + delim.length).trim()
            contentAuthorPart = beforeLikes.substring(0, pos)
            break
        }
    }
    if (timestampRaw == null) return null
    val timestamp = formatTimestamp(timestampRaw)

    // Step 3: split content and author from the remaining part
    val authorFull = "作者："
    val authorAscii = "作者:"
    val authorPos = contentAuthorPart!!.lastIndexOf(authorFull).takeIf { it >= 0 }
        ?: contentAuthorPart.lastIndexOf(authorAscii).takeIf { it >= 0 }
        ?: return null
    val authorMarkerLen = if (contentAuthorPart.contains(authorFull)) authorFull.length else authorAscii.length
    val content = contentAuthorPart.substring(0, authorPos).trim()
    val author = contentAuthorPart.substring(authorPos + authorMarkerLen).trim()

    return ReviewCard(index, content, author, timestamp, likes)
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
    showAll: Boolean = false,
    onShowAllChanged: ((Boolean) -> Unit)? = null,
    onDismiss: () -> Unit
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)

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

            // Show-all toggle (only when opened from anchor link)
            if (!anchorId.isNullOrBlank() && onShowAllChanged != null) {
                TextButton(
                    onClick = { onShowAllChanged(!showAll) },
                    modifier = Modifier.padding(vertical = 2.dp)
                ) {
                    Text(
                        text = if (showAll) I18n.t("review.show_current_only") else I18n.t("review.show_all"),
                        fontSize = 13.sp
                    )
                }
            }

            HorizontalDivider()
            Spacer(Modifier.height(8.dp))

            // Group-filter: anchor-matching block (Heading or Paragraph with anchor_id)
            // + subsequent blocks until next group start (Heading or block with anchor_id)
            val filteredBlocks = remember(blocks, anchorId, showAll) {
                if (anchorId.isNullOrBlank() || showAll) {
                    blocks
                } else {
                    val result = mutableListOf<ContentBlock>()
                    var inGroup = false
                    for (block in blocks) {
                        val blockAnchor = when (block) {
                            is ContentBlock.Heading -> block.anchor_id
                            is ContentBlock.Paragraph -> block.anchor_id
                            else -> null
                        }
                        val isGroupStart = when (block) {
                            is ContentBlock.Heading -> true
                            is ContentBlock.Paragraph -> block.anchor_id != null
                            else -> false
                        }
                        when {
                            blockAnchor == anchorId -> {
                                inGroup = true
                                result.add(block)
                            }
                            inGroup && isGroupStart -> break
                            inGroup -> result.add(block)
                        }
                    }
                    // Defensive fallback: if anchor is not found, show all instead of blank
                    result.ifEmpty { blocks }
                }
            }

            // Content
            LazyColumn {
                items(filteredBlocks) { block ->
                    when (block) {
                        is ContentBlock.Heading -> {
                            val text = block.spans.joinToString("") { it.text }
                            val size = when (block.level) {
                                1 -> 18.sp
                                2 -> 16.sp
                                else -> 14.sp
                            }
                            Text(
                                text = text,
                                fontSize = size,
                                fontWeight = FontWeight.Bold,
                                modifier = Modifier.padding(vertical = 6.dp)
                            )
                        }
                        is ContentBlock.Paragraph -> {
                            val text = block.spans.joinToString("") { it.text }
                            if (text.isNotBlank()) {
                                val card = parseReviewCard(text)
                                if (card != null) {
                                    ReviewCardItem(card = card, fontSize = fontSize)
                                } else {
                                    Text(
                                        text = text,
                                        fontSize = fontSize.sp,
                                        modifier = Modifier.padding(vertical = 4.dp)
                                    )
                                }
                            }
                        }
                        is ContentBlock.Separator -> {
                            HorizontalDivider(
                                modifier = Modifier.padding(vertical = 8.dp),
                                color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f)
                            )
                        }
                        is ContentBlock.BlankLine -> {
                            Spacer(Modifier.height((fontSize * 0.5).dp))
                        }
                        else -> {}
                    }
                }
            }
        }
    }
}

@Composable
private fun ReviewCardItem(card: ReviewCard, fontSize: Float) {
    Card(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 6.dp)
            .border(
                width = 1.dp,
                color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.5f),
                shape = RoundedCornerShape(12.dp)
            ),
        shape = RoundedCornerShape(12.dp),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.6f)
        )
    ) {
        Column(
            modifier = Modifier.padding(12.dp)
        ) {
            // Content
            Text(
                text = card.content,
                fontSize = fontSize.sp,
                fontWeight = FontWeight.Medium,
                modifier = Modifier.padding(bottom = 8.dp)
            )
            // Author / Time / Likes
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Text(
                    text = card.author,
                    fontSize = 13.sp,
                    color = MaterialTheme.colorScheme.primary,
                    fontWeight = FontWeight.SemiBold
                )
                Text(
                    text = "${card.timestamp} · 赞 ${card.likes}",
                    fontSize = 12.sp,
                    color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f)
                )
            }
        }
    }
}
