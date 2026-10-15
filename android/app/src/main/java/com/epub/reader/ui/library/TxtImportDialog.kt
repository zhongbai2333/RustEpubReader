package com.zhongbai233.epub.reader.ui.library

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.ui.theme.ReaderGeometry
import com.zhongbai233.epub.reader.viewmodel.TxtChapterPreview

@Composable
fun TxtImportDialog(
    show: Boolean,
    title: String,
    author: String,
    customRegex: String,
    useHeuristic: Boolean,
    previews: List<TxtChapterPreview>,
    converting: Boolean,
    error: String?,
    onTitleChange: (String) -> Unit,
    onAuthorChange: (String) -> Unit,
    onRegexChange: (String) -> Unit,
    onHeuristicChange: (Boolean) -> Unit,
    onRefreshPreview: () -> Unit,
    onConvert: () -> Unit,
    onDismiss: () -> Unit,
) {
    if (!show) return

    AlertDialog(
        onDismissRequest = { if (!converting) onDismiss() },
        confirmButton = {
            Button(
                onClick = onConvert,
                enabled = !converting,
                shape = ReaderGeometry.Shape,
                contentPadding = ReaderGeometry.ButtonPadding,
            ) {
                if (converting) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(16.dp),
                        strokeWidth = 2.dp,
                    )
                    Spacer(Modifier.width(8.dp))
                }
                Text(if (converting) I18n.t("txt.converting") else I18n.t("txt.convert"))
            }
        },
        dismissButton = {
            if (!converting) {
                TextButton(
                    onClick = onDismiss,
                    shape = ReaderGeometry.Shape,
                    contentPadding = ReaderGeometry.ButtonPadding,
                ) {
                    Text(I18n.t("error.ok"))
                }
            }
        },
        title = { Text(I18n.t("txt.import_title")) },
        text = {
            Column(modifier = Modifier.fillMaxWidth()) {
                // 书名
                OutlinedTextField(
                    value = title,
                    onValueChange = onTitleChange,
                    label = { Text(I18n.t("txt.book_title")) },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                    shape = ReaderGeometry.Shape,
                )
                Spacer(Modifier.height(8.dp))

                // 作者
                OutlinedTextField(
                    value = author,
                    onValueChange = onAuthorChange,
                    label = { Text(I18n.t("txt.author")) },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                    shape = ReaderGeometry.Shape,
                )
                Spacer(Modifier.height(12.dp))

                // 检测模式
                Text(I18n.t("txt.split_mode"), fontWeight = FontWeight.Medium)
                Spacer(Modifier.height(4.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Checkbox(
                        checked = useHeuristic,
                        onCheckedChange = {
                            onHeuristicChange(it)
                            onRefreshPreview()
                        },
                    )
                    Text(I18n.t("txt.mode_heuristic"), fontSize = 14.sp)
                }

                // 自定义正则
                OutlinedTextField(
                    value = customRegex,
                    onValueChange = onRegexChange,
                    label = { Text(I18n.t("txt.mode_regex")) },
                    placeholder = { Text(I18n.t("txt.regex_hint")) },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                    shape = ReaderGeometry.Shape,
                )
                Spacer(Modifier.height(12.dp))

                // 刷新按钮
                OutlinedButton(
                    onClick = onRefreshPreview,
                    modifier = Modifier.fillMaxWidth(),
                    shape = ReaderGeometry.Shape,
                    contentPadding = ReaderGeometry.ButtonPadding,
                ) {
                    Text(I18n.t("txt.preview"))
                }
                Spacer(Modifier.height(8.dp))

                // 预览列表
                HorizontalDivider()
                Spacer(Modifier.height(4.dp))
                Text(
                    I18n.count("txt.chapter_count", previews.size),
                    fontWeight = FontWeight.Medium,
                    fontSize = 14.sp,
                )
                Spacer(Modifier.height(4.dp))

                if (previews.isEmpty()) {
                    Text(
                        I18n.t("txt.no_chapters"),
                        fontSize = 13.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                } else {
                    LazyColumn(modifier = Modifier.heightIn(max = 250.dp)) {
                        itemsIndexed(previews) { i, ch ->
                            Row(
                                modifier = Modifier.fillMaxWidth().padding(vertical = 2.dp),
                                horizontalArrangement = Arrangement.SpaceBetween,
                            ) {
                                Text(
                                    "${i + 1}. ${ch.title}",
                                    fontSize = 13.sp,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                    modifier = Modifier.weight(1f),
                                )
                                Text(
                                    I18n.count("library.character_count", ch.charCount),
                                    fontSize = 12.sp,
                                    fontFamily = FontFamily.Monospace,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                        }
                    }
                }

                // 错误提示
                error?.let {
                    Spacer(Modifier.height(8.dp))
                    Text(it, color = MaterialTheme.colorScheme.error, fontSize = 13.sp)
                }
            }
        },
    )
}
