package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.ui.theme.ReaderGeometry

@Composable
fun TtsControlBar(
    playing: Boolean,
    paused: Boolean,
    status: String,
    currentBlockIndex: Int,
    onPlay: () -> Unit,
    onPause: () -> Unit,
    onResume: () -> Unit,
    onStop: () -> Unit,
    onClose: () -> Unit,
    onSettings: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val accent = MaterialTheme.colorScheme.primary
    val barBg = MaterialTheme.colorScheme.surfaceContainerHigh
    Surface(
        modifier = modifier.padding(horizontal = 16.dp, vertical = 8.dp),
        shape = ReaderGeometry.Shape,
        color = barBg,
        shadowElevation = 6.dp,
    ) {
        Row(
            modifier = Modifier.padding(horizontal = ReaderGeometry.CardInset, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ReaderGeometry.Gap),
        ) {
            if (!playing) {
                FilledIconButton(
                    onClick = onPlay,
                    modifier = Modifier.size(44.dp),
                    colors = IconButtonDefaults.filledIconButtonColors(containerColor = accent),
                ) {
                    Icon(
                        Icons.Default.PlayArrow,
                        contentDescription = I18n.t("tts.play"),
                        tint = MaterialTheme.colorScheme.onPrimary,
                        modifier = Modifier.size(24.dp),
                    )
                }
            } else {
                FilledIconButton(
                    onClick = { if (paused) onResume() else onPause() },
                    modifier = Modifier.size(44.dp),
                    colors = IconButtonDefaults.filledIconButtonColors(containerColor = accent),
                ) {
                    Icon(
                        if (paused) Icons.Default.PlayArrow else Icons.Default.Pause,
                        contentDescription = I18n.t(if (paused) "tts.resume" else "tts.pause"),
                        tint = MaterialTheme.colorScheme.onPrimary,
                        modifier = Modifier.size(24.dp),
                    )
                }
            }
            if (playing) {
                IconButton(onClick = onStop, modifier = Modifier.size(44.dp)) {
                    Icon(
                        Icons.Default.Stop,
                        contentDescription = I18n.t("tts.stop"),
                        modifier = Modifier.size(24.dp),
                    )
                }
            }
            Text(
                text =
                    when {
                        status.isNotEmpty() -> I18n.message(status)
                        playing && paused -> I18n.t("tts.paused")
                        playing -> I18n.t("tts.playing")
                        else -> I18n.t("tts.ready")
                    },
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.widthIn(max = 120.dp),
            )
            IconButton(onClick = onSettings, modifier = Modifier.size(40.dp)) {
                Icon(
                    Icons.Default.Settings,
                    contentDescription = I18n.t("settings.title"),
                    modifier = Modifier.size(24.dp),
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            IconButton(
                onClick = {
                    onStop()
                    onClose()
                },
                modifier = Modifier.size(40.dp),
            ) {
                Icon(
                    Icons.Default.Close,
                    contentDescription = I18n.t("common.close"),
                    modifier = Modifier.size(22.dp),
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}
