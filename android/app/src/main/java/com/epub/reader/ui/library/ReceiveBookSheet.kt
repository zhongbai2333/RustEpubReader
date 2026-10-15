package com.zhongbai233.epub.reader.ui.library

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.MenuBook
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.ui.reader.rememberReaderSheetDismiss

internal data class ReceivePreviewBook(val title: String, val size: Long)
internal data class ReceiveSheetState(
    val busy: Boolean = false,
    val phase: String = "idle",
    val fraction: Float? = null,
    val preview: List<ReceivePreviewBook>? = null,
    val error: String? = null,
    val result: Pair<Int, Int>? = null,
)

/** Also rendered with explicit fixtures by the debug-only UI check activity. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun ReceiveBookSheet(
    state: ReceiveSheetState, link: String, changeLink: (String) -> Unit,
    scan: () -> Unit, image: () -> Unit, receive: () -> Unit, accept: () -> Unit,
    cancel: () -> Unit, reset: () -> Unit, onDismiss: () -> Unit,
) {
    val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val close = rememberReaderSheetDismiss(sheet, onDismiss)
    var editingLink by rememberSaveable { mutableStateOf(link.isNotBlank()) }
    val clipboard = LocalClipboardManager.current
    val context = LocalContext.current
    val maxHeight = (LocalConfiguration.current.screenHeightDp * 0.86f).dp
    val success = state.result != null
    val focus = remember { FocusRequester() }
    val keyboard = LocalSoftwareKeyboardController.current
    LaunchedEffect(state.error, link) { if (state.error != null && link.isNotBlank()) editingLink = true }
    LaunchedEffect(editingLink, state.busy, state.preview != null, success) {
        if (editingLink && !state.busy && state.preview == null && !success) { focus.requestFocus(); keyboard?.show() }
    }
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = sheet, sheetMaxWidth = 560.dp,
        contentWindowInsets = { WindowInsets.safeDrawing.only(WindowInsetsSides.Bottom) }) {
        BackHandler(enabled = editingLink && !state.busy && state.preview == null && !success) { editingLink = false }
        Column(Modifier.fillMaxWidth().heightIn(max = maxHeight).imePadding().padding(horizontal = 20.dp).padding(bottom = 16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                if (editingLink && !state.busy && state.preview == null && !success) {
                    IconButton(onClick = { editingLink = false; keyboard?.hide() }) { Icon(Icons.AutoMirrored.Outlined.ArrowBack, I18n.t("settings.back")) }
                }
                Text(I18n.t("transfer.receive"), style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
            }
            if (state.preview != null && !state.busy && !success) {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(I18n.t("receive.preview_title"), style = MaterialTheme.typography.titleMedium)
                    Text(I18n.t("receive.ready_hint"), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Text(I18n.count("ui.library_count", state.preview.size), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
                }
                Surface(shape = MaterialTheme.shapes.medium, color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = Modifier.weight(1f, fill = false)) {
                    LazyColumn(Modifier.fillMaxWidth().heightIn(max = 300.dp)) {
                        itemsIndexed(state.preview) { index, book ->
                            ListItem(headlineContent = { Text(book.title, maxLines = 2, overflow = TextOverflow.Ellipsis) },
                                supportingContent = { Text(android.text.format.Formatter.formatShortFileSize(context, book.size)) },
                                leadingContent = { Icon(Icons.AutoMirrored.Outlined.MenuBook, null, tint = MaterialTheme.colorScheme.onSurfaceVariant) },
                                colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow))
                            if (index < state.preview.lastIndex) HorizontalDivider(Modifier.padding(start = 56.dp), color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.45f))
                        }
                    }
                }
            } else {
                Column(Modifier.weight(1f, fill = false).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    when {
                        success -> Column(
                            Modifier.fillMaxWidth().padding(vertical = 8.dp),
                            horizontalAlignment = Alignment.CenterHorizontally,
                            verticalArrangement = Arrangement.spacedBy(12.dp),
                        ) {
                            Icon(Icons.Outlined.CheckCircle, null, modifier = Modifier.size(36.dp), tint = MaterialTheme.colorScheme.primary)
                            Text(I18n.t("transfer.imported"), style = MaterialTheme.typography.titleMedium, textAlign = TextAlign.Center)
                            Text(I18n.tf2("transfer.result", state.result!!.first.toString(), state.result.second.toString()),
                                style = MaterialTheme.typography.bodyMedium, textAlign = TextAlign.Center)
                        }
                        state.busy -> {
                            Row(Modifier.fillMaxWidth().padding(vertical = 20.dp).semantics { liveRegion = LiveRegionMode.Polite },
                                horizontalArrangement = Arrangement.spacedBy(16.dp), verticalAlignment = Alignment.CenterVertically) {
                                CircularProgressIndicator(Modifier.size(24.dp), strokeWidth = 2.dp)
                                Text(I18n.t("transfer.${state.phase}"), style = MaterialTheme.typography.bodyLarge)
                            }
                            state.fraction?.let { fraction ->
                                LinearProgressIndicator(progress = { fraction }, modifier = Modifier.fillMaxWidth())
                                Text("${(fraction * 100).toInt()}%", style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                        }
                        else -> {
                            Text(I18n.t(if (editingLink) "receive.link_hint" else "receive.intro"), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            if (!editingLink) Surface(shape = MaterialTheme.shapes.medium, color = MaterialTheme.colorScheme.surfaceContainerLow) {
                                Column {
                                    ReceiveAction(Icons.Outlined.QrCodeScanner, "transfer.scan", "receive.scan_hint", scan)
                                    HorizontalDivider(Modifier.padding(start = 56.dp), color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.45f))
                                    ReceiveAction(Icons.Outlined.Image, "transfer.image", "receive.image_hint", image)
                                    HorizontalDivider(Modifier.padding(start = 56.dp), color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.45f))
                                    ReceiveAction(Icons.Outlined.Link, "transfer.link", "receive.link_hint") { editingLink = !editingLink }
                                }
                            }
                            if (editingLink) {
                                OutlinedTextField(link, changeLink, modifier = Modifier.fillMaxWidth().focusRequester(focus),
                                    label = { Text(I18n.t("receive.link_title")) }, placeholder = { Text(I18n.t("receive.link_placeholder")) },
                                    minLines = 1, maxLines = 3, shape = MaterialTheme.shapes.medium)
                                TextButton(onClick = { clipboard.getText()?.text?.let { changeLink(it.trim()) } }) {
                                    Icon(Icons.Outlined.ContentPaste, null, Modifier.size(18.dp)); Spacer(Modifier.width(8.dp)); Text(I18n.t("receive.paste"))
                                }
                            }
                            Text(I18n.t("receive.no_login"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                    }
                    state.error?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium,
                        modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }) }
                }
            }
            when {
                success -> Button(close, Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text(I18n.t("manage.done")) }
                state.busy -> OutlinedButton(cancel, Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text(I18n.t("transfer.cancel")) }
                state.preview != null -> {
                    Button(accept, Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text(I18n.t("transfer.import")) }
                    TextButton(onClick = { reset(); editingLink = false }, modifier = Modifier.fillMaxWidth()) { Text(I18n.t("receive.choose_again")) }
                }
                editingLink -> Button(receive, Modifier.fillMaxWidth().heightIn(min = 48.dp), enabled = link.isNotBlank()) { Text(I18n.t("transfer.connect")) }
            }
        }
    }
}

@Composable
private fun ReceiveAction(icon: ImageVector, title: String, hint: String, onClick: () -> Unit) {
    ListItem(headlineContent = { Text(I18n.t(title), color = MaterialTheme.colorScheme.onSurface) },
        supportingContent = { Text(I18n.t(hint), style = MaterialTheme.typography.bodySmall) },
        leadingContent = { Icon(icon, null, tint = MaterialTheme.colorScheme.primary) },
        trailingContent = { Icon(Icons.Outlined.ChevronRight, null, modifier = Modifier.size(18.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant) },
        modifier = Modifier.fillMaxWidth().clickable(onClick = onClick),
        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow))
}
