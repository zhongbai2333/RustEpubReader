package com.zhongbai233.epub.reader.ui.reader

import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.model.ContentBlock
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * 段评面板 — 底部弹层展示段评内容
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ReviewPanel(
    chapterTitle: String,
    blocks: List<ContentBlock>,
    fontSize: Float = 16f,
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
                modifier = Modifier.padding(bottom = 8.dp)
            )

            HorizontalDivider()
            Spacer(Modifier.height(8.dp))

            // Content
            LazyColumn {
                itemsIndexed(blocks, key = { index, _ -> index }) { _, block ->
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
                                Text(
                                    text = text,
                                    fontSize = fontSize.sp,
                                    modifier = Modifier.padding(vertical = 4.dp)
                                )
                            }
                        }
                        is ContentBlock.Image -> {
                            val bitmap by produceState<android.graphics.Bitmap?>(
                                initialValue = null,
                                block.data
                            ) {
                                value = withContext(Dispatchers.IO) {
                                    val bytes = android.util.Base64.decode(
                                        block.data,
                                        android.util.Base64.DEFAULT
                                    )
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
                    }
                }
            }
        }
    }
}
