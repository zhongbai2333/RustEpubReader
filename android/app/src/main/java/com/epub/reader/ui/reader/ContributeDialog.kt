package com.zhongbai233.epub.reader.ui.reader

import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.ui.theme.ReaderGeometry

@Composable
fun ContributeDialog(
    show: Boolean,
    sampleCount: Int,
    samples: String,
    githubUsername: String?,
    inProgress: Boolean,
    status: String,
    prUrl: String?,
    onPrepare: () -> Unit,
    onLogin: () -> Unit,
    onSubmit: () -> Unit,
    onDismiss: () -> Unit,
) {
    if (!show) return

    // Prepare samples when dialog opens
    LaunchedEffect(show) {
        if (show) onPrepare()
    }

    val context = LocalContext.current

    AlertDialog(
        onDismissRequest = { if (!inProgress) onDismiss() },
        confirmButton = {
            if (prUrl != null) {
                // Success state — show close + open PR
                Row {
                    TextButton(
                        onClick = {
                            context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(prUrl)))
                        },
                        shape = ReaderGeometry.Shape,
                        contentPadding = ReaderGeometry.ButtonPadding,
                    ) {
                        Text(I18n.t("csc.contribute_success").replace("{}", "PR"))
                    }
                    Spacer(Modifier.width(8.dp))
                    Button(
                        onClick = onDismiss,
                        shape = ReaderGeometry.Shape,
                        contentPadding = ReaderGeometry.ButtonPadding,
                    ) {
                        Text(I18n.t("csc.contribute_close"))
                    }
                }
            } else {
                Button(
                    onClick = onSubmit,
                    enabled = !inProgress && githubToken(githubUsername) && sampleCount > 0,
                    shape = ReaderGeometry.Shape,
                    contentPadding = ReaderGeometry.ButtonPadding,
                ) {
                    if (inProgress) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(16.dp),
                            strokeWidth = 2.dp,
                            color = MaterialTheme.colorScheme.onPrimary,
                        )
                        Spacer(Modifier.width(8.dp))
                        Text(I18n.t("csc.contribute_submitting"))
                    } else {
                        Text(I18n.t("csc.contribute_submit"))
                    }
                }
            }
        },
        dismissButton = {
            if (!inProgress && prUrl == null) {
                TextButton(
                    onClick = onDismiss,
                    shape = ReaderGeometry.Shape,
                    contentPadding = ReaderGeometry.ButtonPadding,
                ) {
                    Text(I18n.t("csc.contribute_later"))
                }
            }
        },
        title = { Text(I18n.t("csc.contribute_title")) },
        text = {
            Column(
                modifier = Modifier.fillMaxWidth().verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                // Description
                Text(
                    text = I18n.t("csc.contribute_desc"),
                    style = MaterialTheme.typography.bodyMedium,
                )

                // Sample count
                Text(
                    text =
                        I18n.t("csc.contribute_sample_count").replace("{}", sampleCount.toString()),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.primary,
                )

                // Preview
                if (samples.isNotBlank()) {
                    Text(
                        text = I18n.t("csc.contribute_preview"),
                        style = MaterialTheme.typography.labelMedium,
                    )
                    Surface(
                        color = MaterialTheme.colorScheme.surfaceVariant,
                        shape = MaterialTheme.shapes.small,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text(
                            text = samples.lines().take(5).joinToString("\n"),
                            modifier = Modifier.padding(8.dp),
                            style =
                                MaterialTheme.typography.bodySmall.copy(
                                    fontFamily = FontFamily.Monospace,
                                    fontSize = 11.sp,
                                ),
                            maxLines = 5,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }

                HorizontalDivider()

                // GitHub login section
                if (githubUsername != null) {
                    Text(
                        text = I18n.t("csc.contribute_logged_in").replace("{}", githubUsername),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.primary,
                    )
                } else {
                    Text(
                        I18n.t("csc.contribute_need_login"),
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
                Text(
                    I18n.t("account.contribution_hint"),
                    style = MaterialTheme.typography.bodySmall,
                )
                OutlinedButton(
                    onClick = onLogin,
                    enabled = !inProgress,
                    shape = ReaderGeometry.Shape,
                    contentPadding = ReaderGeometry.ButtonPadding,
                ) {
                    Text(I18n.t("account.manage"))
                }

                // Status / Error
                if (status.isNotBlank()) {
                    Text(
                        text = status,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.error,
                    )
                }

                // Success PR URL
                if (prUrl != null) {
                    Text(
                        text = I18n.t("csc.contribute_success").replace("{}", prUrl),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.primary,
                    )
                }
            }
        },
    )
}

private fun githubToken(username: String?): Boolean = username != null
