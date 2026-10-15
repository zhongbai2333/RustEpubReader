package com.zhongbai233.epub.reader.ui.reader

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.unit.dp
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.layout.ContentScale
import coil.compose.AsyncImage
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.util.FontLibrary
import com.zhongbai233.epub.reader.util.DownloadableFont

@Composable
internal fun FontImportAction() {
    val context = LocalContext.current
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        if (uris.isNotEmpty()) FontLibrary.startImport(context, uris)
    }
    TextButton(onClick = { picker.launch(arrayOf("font/*", "application/x-font-ttf", "application/x-font-opentype", "application/octet-stream")) }, enabled = FontLibrary.loading["import"] != true) { Text(I18n.t("fonts.import")) }
    if (FontLibrary.loading["import"] == true) Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) { CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp); Text(I18n.t("fonts.importing")) }
    if (FontLibrary.failed["import"] == true) Text(I18n.t("fonts.invalid"), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
}

@Composable
internal fun FontLibrarySettings(selected: String, enabled: Boolean, onSelect: (String) -> Unit) {
    val context = LocalContext.current
    val links = LocalUriHandler.current
    val revision = FontLibrary.revision
    var language by rememberSaveable { mutableStateOf("zh") }
    var license by remember { mutableStateOf<DownloadableFont?>(null) }
    val external = remember { FontLibrary.external(context) }
    Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(I18n.t("fonts.description"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(I18n.t("fonts.preview_note"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        FontImportAction()
        SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
            listOf("zh", "en").forEachIndexed { index, code ->
                SegmentedButton(selected = language == code, onClick = { language = code }, shape = SegmentedButtonDefaults.itemShape(index, 2)) { Text(I18n.t(if(code == "zh") "fonts.chinese" else "fonts.english")) }
            }
        }
        if(language == "en") Text(I18n.t("fonts.latin_fallback"), style = MaterialTheme.typography.bodySmall)
        Text(I18n.t("fonts.direct"), style = MaterialTheme.typography.labelLarge)
        FontLibrary.catalog(context).filter { it.language == language }.forEach { font ->
            val installed = remember(font.id, revision) { FontLibrary.installed(context, font) }
            HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
            Column(Modifier.fillMaxWidth().padding(vertical = 4.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(if (I18n.currentCode == "zh_cn") font.nameZh else font.nameEn, style = MaterialTheme.typography.titleSmall)
                Text(if (I18n.currentCode == "zh_cn") font.descriptionZh else font.descriptionEn, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                AsyncImage(model = "file:///android_asset/fonts/${font.preview}", contentDescription = font.previewText,
                    colorFilter = ColorFilter.tint(MaterialTheme.colorScheme.onSurface), contentScale = ContentScale.Fit,
                    modifier = Modifier.widthIn(max = 320.dp).fillMaxWidth().aspectRatio(1200f / 216f))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(if (font.bytes < 1_000_000) String.format(I18n.locale, "%.0f KB", font.bytes / 1_000.0) else String.format(I18n.locale, "%.1f MB", font.bytes / 1_000_000.0), style = MaterialTheme.typography.labelSmall, modifier = Modifier.weight(1f))
                    if (installed) {
                        if (selected == font.nameZh) Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                            Icon(Icons.Default.Check, null, modifier = Modifier.size(16.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
                            Text(I18n.t("fonts.selected"), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        } else TextButton(onClick = { onSelect(font.nameZh) }, enabled = enabled) { Text(I18n.t("fonts.use")) }
                    } else if (FontLibrary.loading[font.id] == true) {
                        CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
                        Text(I18n.t("fonts.downloading"), modifier = Modifier.padding(start = 8.dp), style = MaterialTheme.typography.bodySmall)
                    } else {
                        TextButton(onClick = { FontLibrary.start(context, font) }) { Text(I18n.t(if (FontLibrary.failed[font.id] == true) "fonts.retry" else "fonts.download")) }
                    }
                }
                if (FontLibrary.failed[font.id] == true) Text(I18n.t("fonts.failed"), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
                TextButton(onClick = { license = font }, contentPadding = PaddingValues(0.dp)) { Text(I18n.t("fonts.details") + " · " + font.license, style = MaterialTheme.typography.labelSmall) }
            }
        }
        Text(I18n.t("fonts.open_note"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        val official = external.filter { it.language == language }
        if (official.isNotEmpty()) {
            Text(I18n.t("fonts.external"), style = MaterialTheme.typography.labelLarge, modifier = Modifier.padding(top = 12.dp))
            Text(I18n.t("fonts.personal_note"), style = MaterialTheme.typography.bodySmall)
            official.forEach { font ->
                HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                Text(if (I18n.currentCode == "zh_cn") font.nameZh else font.nameEn, style = MaterialTheme.typography.titleSmall)
                Text(if (I18n.currentCode == "zh_cn") font.descriptionZh else font.descriptionEn, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                Row {
                    TextButton(onClick = { links.openUri(font.projectUrl) }) { Text(I18n.t("fonts.official_preview")) }
                    TextButton(onClick = { links.openUri(font.licenseUrl) }) { Text(I18n.t("fonts.official_terms")) }
                }
                Text(I18n.t("fonts.checked") + " · " + font.checkedAt, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            Text(I18n.t("fonts.import_note"), style = MaterialTheme.typography.bodySmall)
        }
        Spacer(Modifier.height(24.dp))
    }
    license?.let { font ->
        val text = remember(font.id) { context.assets.open("fonts/${font.licenseFile}").bufferedReader().use { it.readText() } }
        AlertDialog(onDismissRequest = { license = null }, title = { Text(I18n.t("fonts.details")) },
            text = { Column(Modifier.heightIn(max = 430.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(I18n.t("fonts.open_note"))
                TextButton(onClick = { links.openUri(font.projectUrl) }) { Text(I18n.t("fonts.official")) }
                TextButton(onClick = { links.openUri(font.licenseUrl) }) { Text(I18n.t("fonts.official_terms")) }
                Text(text, style = MaterialTheme.typography.bodySmall)
            } }, confirmButton = { TextButton(onClick = { license = null }) { Text(I18n.t("settings.done")) } })
    }
}
