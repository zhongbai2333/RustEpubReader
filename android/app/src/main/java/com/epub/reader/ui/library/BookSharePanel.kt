package com.zhongbai233.epub.reader.ui.library

import android.app.Activity
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.automirrored.outlined.MenuBook
import androidx.compose.material.icons.outlined.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.style.TextOverflow
import android.graphics.BitmapFactory
import android.view.WindowManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.i18n.I18n
import kotlinx.coroutines.delay
import org.json.JSONArray
import org.json.JSONObject

@Composable
private fun WatchBookShare(model: BookShareModel) {
    val context = LocalContext.current
    LaunchedEffect(model) { while (true) { model.poll(); delay(500) } }
    DisposableEffect(model) { onDispose { model.close() } }
    DisposableEffect(model.activeLan) {
        val window = (context as? Activity)?.window
        if (model.activeLan) window?.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        onDispose { window?.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON) }
    }
}
@Composable
private fun BookShareProgress(model: BookShareModel) {
    val progress = model.state.optJSONObject("progress")
    if (model.phase != "idle" && model.phase != "error") Text(I18n.t("transfer.${model.phase}"), style = MaterialTheme.typography.titleSmall)
    val total = progress?.optLong("total") ?: 0L
    if (total > 0) {
        val done = progress?.optLong("done") ?: 0L
        LinearProgressIndicator(progress = { (done.toFloat() / total).coerceIn(0f, 1f) }, modifier = Modifier.fillMaxWidth())
        Text("${(done * 100 / total).coerceIn(0, 100)}%", style = MaterialTheme.typography.bodySmall)
    }
    val error = model.error ?: progress?.optString("error")?.takeIf { it.isNotBlank() && it != "null" }
    if (error != null) Text(I18n.tf1("transfer.failed", error), color = MaterialTheme.colorScheme.error)
    model.result?.let { (imported, skipped) -> Text(I18n.tf2("transfer.result", imported.toString(), skipped.toString())) }
}
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun BookShareSendSheet(model: BookShareModel?, books: List<com.zhongbai233.epub.reader.model.BookEntry>, covers: Map<String, ByteArray?>, token: String?, authorized: Boolean, onLogin: () -> Unit, exporting: Boolean, onExport: () -> Unit, onDismiss: () -> Unit) {
    var stage by remember(books.map { it.id }) { mutableIntStateOf(0) }
    var selected by remember { mutableStateOf(false) }
    val maxBody = (androidx.compose.ui.platform.LocalConfiguration.current.screenHeightDp - 250).coerceAtLeast(230).dp
    fun back() { if (stage == 1 || model?.busy == true) model?.cancel(); stage = 0 }
    ModalBottomSheet(sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true), onDismissRequest = { if (!exporting) onDismiss() }) {
        androidx.activity.compose.BackHandler(enabled = stage != 0 && !exporting) { back() }
        Column(Modifier.fillMaxWidth().padding(horizontal = 24.dp).padding(bottom = 20.dp)) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                if (stage != 0) IconButton(onClick = { back() }, enabled = !exporting) { Icon(Icons.AutoMirrored.Outlined.ArrowBack, I18n.t("settings.back")) }
                Text(I18n.t(when(stage) { 1 -> "share_ui.nearby"; 2 -> "transfer.github"; else -> "book.share_title" }), style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
                IconButton(onClick = onDismiss, enabled = !exporting) { Icon(Icons.Outlined.Close, I18n.t("settings.close")) }
            }
            Spacer(Modifier.height(8.dp))
            val cover = remember(books.firstOrNull()?.uri, covers[books.firstOrNull()?.uri]) { covers[books.firstOrNull()?.uri]?.let { BitmapFactory.decodeByteArray(it, 0, it.size) } }
            Row(Modifier.fillMaxWidth().clickable(enabled = books.size > 1) { selected = true }.padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                if (cover != null) Image(cover.asImageBitmap(), null, modifier = Modifier.size(40.dp, 56.dp).clip(MaterialTheme.shapes.extraSmall), contentScale = ContentScale.Crop)
                else Box(Modifier.size(40.dp, 56.dp).clip(MaterialTheme.shapes.extraSmall).background(MaterialTheme.colorScheme.surfaceContainerHighest), contentAlignment = Alignment.Center) { Icon(Icons.AutoMirrored.Outlined.MenuBook, null, tint = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.size(22.dp)) }
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(books.firstOrNull()?.title.orEmpty(), style = MaterialTheme.typography.titleMedium, maxLines = 2, overflow = TextOverflow.Ellipsis)
                    Text(I18n.tf1("manage.count", books.size.toString()), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                if (books.size > 1) Icon(Icons.Outlined.ChevronRight, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            Spacer(Modifier.height(12.dp))
            Column(Modifier.fillMaxWidth().heightIn(max = maxBody).verticalScroll(remember(stage) { androidx.compose.foundation.ScrollState(0) }), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                if (model != null) BookShareSender(model, books.map { it.id }, token, authorized, onLogin, stage, { stage = it }, exporting, onExport)
                else ShareActionRow("share_ui.export_file", "share_ui.export_hint", Icons.Outlined.FileDownload, !exporting, onExport)
            }
        }
    }
    if (selected) AlertDialog(onDismissRequest = { selected = false }, title = { Text(I18n.t("share_ui.selected")) }, text = { Column(Modifier.heightIn(max = 300.dp).verticalScroll(rememberScrollState())) { books.forEach { Text(it.title, Modifier.padding(vertical = 6.dp)) } } }, confirmButton = { TextButton(onClick = { selected = false }) { Text(I18n.t("settings.close")) } })
}

@Composable
private fun ShareActionRow(title: String, subtitle: String, icon: ImageVector, enabled: Boolean, onClick: () -> Unit) {
    ListItem(
        headlineContent = { Text(I18n.t(title), style = MaterialTheme.typography.bodyLarge) },
        supportingContent = { Text(I18n.t(subtitle), style = MaterialTheme.typography.bodySmall, maxLines = 2, overflow = TextOverflow.Ellipsis) },
        leadingContent = { Icon(icon, null, tint = MaterialTheme.colorScheme.onSurfaceVariant) },
        trailingContent = { Icon(Icons.Outlined.ChevronRight, null, tint = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.size(18.dp)) },
        modifier = Modifier.fillMaxWidth().clickable(enabled = enabled, onClick = onClick),
        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)
    )
}

@Composable
fun BookShareSender(model: BookShareModel, ids: List<String>, token: String?, authorized: Boolean, onLogin: () -> Unit, stage: Int, onStage: (Int) -> Unit, exporting: Boolean, onExport: () -> Unit) {
    WatchBookShare(model)
    var copied by remember(stage) { mutableStateOf(false) }
    var details by remember(stage) { mutableStateOf(false) }
    val clipboard = LocalClipboardManager.current
    val outgoing = model.state.optJSONObject("outgoing")
    val hasQr = model.phase in listOf("waiting", "published") && outgoing != null && outgoing.optString("kind") == if (stage == 1) "lan" else "github"
    fun local() { onStage(1); model.command("start_lan", JSONObject().put("ids", JSONArray(ids))) }
    if (stage == 0) {
        Surface(shape = MaterialTheme.shapes.medium, color = MaterialTheme.colorScheme.surfaceContainerLow) {
            Column {
                ShareActionRow("share_ui.nearby", "share_ui.nearby_hint", Icons.Outlined.Wifi, !model.busy && !exporting && ids.isNotEmpty(), ::local)
                HorizontalDivider(Modifier.padding(start = 56.dp), color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.5f))
                ShareActionRow("transfer.github", "share_ui.cloud_hint", Icons.Outlined.CloudUpload, !model.busy && !exporting && ids.isNotEmpty()) { onStage(2) }
                HorizontalDivider(Modifier.padding(start = 56.dp), color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.5f))
                ShareActionRow("share_ui.export_file", "share_ui.export_hint", Icons.Outlined.FileDownload, !model.busy && !exporting && ids.isNotEmpty(), onExport)
            }
        }
        if (exporting) LinearProgressIndicator(Modifier.fillMaxWidth())
        TextButton(onClick = { details = !details }) { Text(I18n.t("share_ui.details")); Icon(if (details) Icons.Outlined.ExpandLess else Icons.Outlined.ExpandMore, null, Modifier.size(18.dp)) }
        if (details) { Text(I18n.t("transfer.limits"), style = MaterialTheme.typography.bodySmall); Text(I18n.t("transfer.lan_hint"), style = MaterialTheme.typography.bodySmall) }
        BookShareHistory(model, token, authorized)
    } else if (stage == 2 && !hasQr) {
        Text(I18n.t("share_ui.public_hint"), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Button(onClick = { if (token == null || !authorized) onLogin() else model.command("publish", JSONObject().put("ids", JSONArray(ids)).put("token", token)) }, enabled = !model.busy && ids.isNotEmpty(), modifier = Modifier.fillMaxWidth()) { Text(I18n.t(if (token == null || !authorized) "transfer.login" else "share_ui.create_link")) }
        if (model.hasGitHubStatus) BookShareProgress(model)
        if (model.busy) TextButton(onClick = { model.cancel() }) { Text(I18n.t("transfer.cancel")) }
        TextButton(onClick = { details = !details }) { Text(I18n.t("share_ui.repository_details")); Icon(if(details) Icons.Outlined.ExpandLess else Icons.Outlined.ExpandMore, null, Modifier.size(18.dp)) }
        if (details) { Text(I18n.t("transfer.github_hint"), style = MaterialTheme.typography.bodySmall); Text(I18n.t("transfer.limits"), style = MaterialTheme.typography.bodySmall) }
        BookShareHistory(model, token, authorized)
    } else if (hasQr) {
        Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
            val encoded = outgoing!!.optString("qr")
            val bitmap = remember(encoded) { runCatching { android.util.Base64.decode(encoded, android.util.Base64.DEFAULT).let { BitmapFactory.decodeByteArray(it, 0, it.size) } }.getOrNull() }
            if (bitmap != null) Surface(color = androidx.compose.ui.graphics.Color.White, shape = MaterialTheme.shapes.medium) { Image(bitmap.asImageBitmap(), I18n.t("share_ui.qr_accessibility"), modifier = Modifier.padding(10.dp).size(240.dp)) }
            if (!outgoing.isNull("expires")) {
                val seconds = (outgoing.optLong("expires") - System.currentTimeMillis() / 1000).coerceAtLeast(0)
                Text(I18n.tf1("transfer.remaining", "%02d:%02d".format(seconds / 60, seconds % 60)), style = MaterialTheme.typography.labelLarge)
                Text(I18n.t("share_ui.keep_open"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            Text(I18n.t("transfer.credential_hint"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Button(onClick = { clipboard.setText(AnnotatedString(outgoing.optString("link"))); copied = true }, modifier = Modifier.fillMaxWidth()) { Icon(if (copied) Icons.Outlined.Check else Icons.Outlined.ContentCopy, null, Modifier.size(18.dp)); Spacer(Modifier.width(8.dp)); Text(I18n.t(if(copied) "transfer.copied" else "transfer.copy_link")) }
            if (model.activeLan) TextButton(onClick = { model.cancel(); onStage(0) }) { Text(I18n.t("share_ui.end")) }
            if (model.phase == "published") Text(I18n.t("transfer.published_hint"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    } else {
        BookShareProgress(model)
        if (model.busy) { LinearProgressIndicator(Modifier.fillMaxWidth()); TextButton(onClick = { model.cancel(); onStage(0) }) { Text(I18n.t("transfer.cancel")) } }
        else if(stage == 1) Button(onClick = ::local, modifier = Modifier.fillMaxWidth()) { Text(I18n.t("share_ui.retry")) }
    }
    LaunchedEffect(copied) { if (copied) { delay(2500); copied = false } }
}

@Composable
private fun BookShareHistory(model: BookShareModel, token: String?, authorized: Boolean) {
    val history = model.state.optJSONArray("history") ?: return
    if (history.length() == 0) return
    var expanded by remember { mutableStateOf(false) }
    var deleting by remember { mutableStateOf<String?>(null) }
    TextButton(onClick = { expanded = !expanded }) { Text(I18n.t("transfer.history")); Icon(if(expanded) Icons.Outlined.ExpandLess else Icons.Outlined.ExpandMore, null, Modifier.size(18.dp)) }
    if (expanded) for (index in history.length() - 1 downTo 0) {
        val item = history.getJSONObject(index)
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
            Column { Text(I18n.tf1("manage.count", item.optInt("count").toString())); Text(java.text.DateFormat.getDateTimeInstance(java.text.DateFormat.SHORT, java.text.DateFormat.SHORT).format(java.util.Date(item.optLong("created") * 1000)), style = MaterialTheme.typography.bodySmall); if(!item.optBoolean("ready",true) && !model.busy) Text(I18n.t("transfer.unfinished"), style = MaterialTheme.typography.bodySmall) }
            TextButton(onClick = { deleting = item.getString("id") }, enabled = !model.busy && token != null && authorized) { Text(I18n.t("transfer.delete")) }
        }
    }
    deleting?.let { id -> AlertDialog(onDismissRequest = { deleting = null }, title = { Text(I18n.t("transfer.delete")) }, text = { Text(I18n.t("transfer.delete_confirm")) }, confirmButton = { TextButton(onClick = { deleting = null; model.command("delete_share", JSONObject().put("id", id).put("token", token)) }) { Text(I18n.t("transfer.delete")) } }, dismissButton = { TextButton(onClick = { deleting = null }) { Text(I18n.t("settings.cancel")) } }) }
}
@Composable
fun BookShareReceiver(model: BookShareModel, onDismiss: () -> Unit) {
    WatchBookShare(model)
    val context = LocalContext.current
    val image = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri -> uri?.let { model.scanImage(context, it) } }
    val scanner = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        if (result.resultCode == Activity.RESULT_OK) {
            if (result.data?.getBooleanExtra(BookScanActivity.PICK_IMAGE, false) == true) image.launch("image/*")
            else result.data?.getStringExtra(BookScanActivity.CONTENTS)?.let { model.receive(it) }
        }
    }
    val progress = model.state.optJSONObject("progress")
    val total = progress?.optLong("total") ?: 0L
    val preview = model.state.optJSONObject("preview")?.optJSONArray("books")
    val books = preview?.let { array -> (0 until array.length()).map { i ->
        val book = array.getJSONObject(i); ReceivePreviewBook(book.getString("title"), book.optLong("size"))
    } }
    val error = model.error ?: progress?.optString("error")?.takeIf { it.isNotBlank() && it != "null" }
    ReceiveBookSheet(ReceiveSheetState(model.busy, model.phase,
        if (total > 0) ((progress?.optLong("done") ?: 0L).toFloat() / total).coerceIn(0f, 1f) else null,
        books, error, model.result), model.link, { model.link = it },
        scan = { scanner.launch(android.content.Intent(context, BookScanActivity::class.java)) },
        image = { image.launch("image/*") }, receive = { model.receive(model.link.trim()) },
        accept = { model.command("accept") }, cancel = { model.cancel() }, reset = { model.resetReceiver() },
        onDismiss = { model.cancel(); onDismiss() })
}
