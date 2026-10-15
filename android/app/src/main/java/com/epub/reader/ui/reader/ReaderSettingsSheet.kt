package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.ui.theme.ReaderGeometry
import com.zhongbai233.epub.reader.util.FontItem
import java.io.File
import coil.compose.AsyncImage
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.draw.clip

import androidx.compose.foundation.clickable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.contentDescription

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun ReaderSettingsSheet(
    fontSize: Float,
    scrollMode: Boolean,
    isDarkMode: Boolean,
    appearance: com.zhongbai233.epub.reader.model.ReadingAppearance? = null,
    appearanceDark: Boolean = false,
    appearanceLinkage: com.zhongbai233.epub.reader.model.AppearanceLinkage? = null,
    systemAppearanceDark: Boolean = false,
    ambientAppearanceDark: Boolean? = null,
    onLinkageChange: (com.zhongbai233.epub.reader.model.AppearanceLinkage) -> Unit = {},
    ambientSensorAvailable: Boolean = false,
    onAppearanceChange: (com.zhongbai233.epub.reader.model.ReadingAppearance) -> Unit = {},
    previewFontFamily: FontFamily = FontFamily.Default,
    bgColorIndex: Int,
    customBgColor: Color,
    fontColorIndex: Int,
    customFontColor: Color,
    fontFamilyName: String,
    pageAnimation: String,
    bgImageEnabled: Boolean,
    bgImageAlpha: Float,
    bgImageUri: String? = null,
    language: String,
    showImmersiveStatus: Boolean = false,
    systemFonts: List<FontItem> = emptyList(),
    onDismiss: () -> Unit,
    onFontSizeChange: (Float) -> Unit,
    onScrollModeChange: (Boolean) -> Unit,
    onDarkModeChange: (Boolean) -> Unit,
    onBgColorChange: (Int) -> Unit,
    onCustomBgColorChange: (Color) -> Unit,
    onFontColorChange: (Int) -> Unit,
    onCustomFontColorChange: (Color) -> Unit,
    onFontFamilyChange: (String) -> Unit,
    onPageAnimationChange: (String) -> Unit,
    onBgImageAlphaChange: (Float) -> Unit,
    onLanguageChange: (String) -> Unit,
    onShowImmersiveStatusChange: (Boolean) -> Unit = {},
    onPickBackgroundImage: () -> Unit,
    onClearBackgroundImage: () -> Unit,
    // 排版
    lineSpacing: Float = 1.5f,
    paraSpacing: Float = 0.5f,
    textIndent: Int = 2,
    titleFontScale: Float = 1.5f,
    onLineSpacingChange: (Float) -> Unit = {},
    onParaSpacingChange: (Float) -> Unit = {},
    onTextIndentChange: (Int) -> Unit = {},
    onTitleFontScaleChange: (Float) -> Unit = {},
    // API
    translateApiUrl: String = "",
    translateApiKey: String = "",
    dictionaryApiUrl: String = "",
    dictionaryApiKey: String = "",
    onTranslateApiUrlChange: (String) -> Unit = {},
    onTranslateApiKeyChange: (String) -> Unit = {},
    onDictionaryApiUrlChange: (String) -> Unit = {},
    onDictionaryApiKeyChange: (String) -> Unit = {},
    scriptsEnabled: Boolean = true,
    onScriptsEnabledChange: (Boolean) -> Unit = {},
    cssPolicy: String = "mixed",
    onCssPolicyChange: (String) -> Unit = {},
) {
    var more by rememberSaveable { mutableStateOf(false) }
    var fontPicker by remember { mutableStateOf(false) }
    var confirmReset by rememberSaveable { mutableStateOf(false) }
    val readingScroll = rememberScrollState()
    val moreScroll = rememberScrollState()
    val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val closeSheet = rememberReaderSheetDismiss(sheet, onDismiss)
    val themeNames = listOf("color.warm_white", "color.light_gray", "color.bean_green", "color.deep_night", "color.graphite").map { I18n.t(it) } + I18n.t("settings.custom")
    val papers = listOf(Color(0xFFF5F0E8), Color(0xFFF1F3F5), Color(0xFFE8F0E8), Color(0xFF1A1A1A), Color(0xFF24262B), customBgColor)
    val manualTypography = cssPolicy != "publisher"
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = sheet, sheetMaxWidth = 560.dp,
        containerColor = MaterialTheme.colorScheme.surfaceContainerLow) {
        Column(Modifier.fillMaxWidth().fillMaxHeight(0.92f)) {
            Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                if (more) IconButton(onClick = { more = false }) { Icon(Icons.AutoMirrored.Filled.ArrowBack, I18n.t("settings.back")) }
                Text(I18n.t(if (more) "settings.more" else "settings.appearance_title"), style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
                TextButton(onClick = closeSheet) { Text(I18n.t("settings.done")) }
            }
            Column(Modifier.weight(1f).verticalScroll(if (more) moreScroll else readingScroll).padding(horizontal = 16.dp).padding(bottom = 24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                if (more) {
            // ─── 语言选择 ───
            Spacer(Modifier.height(12.dp))
            Text(I18n.t("settings.language"))
            Row(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.horizontalScroll(rememberScrollState()),
            ) {
                I18n.availableLanguages.forEach { (code, label) ->
                    val selected = if (I18n.isAuto) code == "auto" else code == I18n.currentCode
                    FilterChip(
                        selected = selected,
                        onClick = { onLanguageChange(code) },
                        label = { Text(label) },
                    )
                }
            }

            Spacer(Modifier.height(12.dp))
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(I18n.t("settings.immersive_status_info"))
                    Text(
                        I18n.t("settings.immersive_status_info_desc"),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Switch(
                    checked = showImmersiveStatus,
                    onCheckedChange = onShowImmersiveStatusChange,
                )
            }

            // ─── API 设置 ───
            Spacer(Modifier.height(16.dp))
            HorizontalDivider()
            Spacer(Modifier.height(12.dp))
            Text(I18n.t("settings.api_settings"), style = MaterialTheme.typography.titleMedium)
            Spacer(Modifier.height(8.dp))

            Text(I18n.t("settings.translate_section"), style = MaterialTheme.typography.labelLarge)
            Spacer(Modifier.height(4.dp))
            OutlinedTextField(
                value = translateApiUrl,
                onValueChange = onTranslateApiUrlChange,
                label = { Text(I18n.t("settings.translate_api_url")) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
                placeholder = { Text("https://api.example.com/translate") },
                shape = ReaderGeometry.Shape,
            )
            Spacer(Modifier.height(4.dp))
            var translateKeyVisible by remember { mutableStateOf(false) }
            OutlinedTextField(
                value = translateApiKey,
                onValueChange = onTranslateApiKeyChange,
                label = { Text(I18n.t("settings.translate_api_key")) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
                visualTransformation =
                    if (translateKeyVisible)
                        androidx.compose.ui.text.input.VisualTransformation.None
                    else androidx.compose.ui.text.input.PasswordVisualTransformation(),
                trailingIcon = {
                    IconButton(onClick = { translateKeyVisible = !translateKeyVisible }) {
                        Icon(
                            if (translateKeyVisible) Icons.Default.VisibilityOff
                            else Icons.Default.Visibility,
                            contentDescription = null,
                        )
                    }
                },
                placeholder = { Text("sk-...") },
                shape = ReaderGeometry.Shape,
            )

            Spacer(Modifier.height(12.dp))
            Text(I18n.t("settings.dictionary_section"), style = MaterialTheme.typography.labelLarge)
            Spacer(Modifier.height(4.dp))
            OutlinedTextField(
                value = dictionaryApiUrl,
                onValueChange = onDictionaryApiUrlChange,
                label = { Text(I18n.t("settings.dictionary_api_url")) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
                placeholder = { Text("https://api.example.com/dictionary") },
                shape = ReaderGeometry.Shape,
            )
            Spacer(Modifier.height(4.dp))
            var dictKeyVisible by remember { mutableStateOf(false) }
            OutlinedTextField(
                value = dictionaryApiKey,
                onValueChange = onDictionaryApiKeyChange,
                label = { Text(I18n.t("settings.dictionary_api_key")) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
                visualTransformation =
                    if (dictKeyVisible) androidx.compose.ui.text.input.VisualTransformation.None
                    else androidx.compose.ui.text.input.PasswordVisualTransformation(),
                trailingIcon = {
                    IconButton(onClick = { dictKeyVisible = !dictKeyVisible }) {
                        Icon(
                            if (dictKeyVisible) Icons.Default.VisibilityOff
                            else Icons.Default.Visibility,
                            contentDescription = null,
                        )
                    }
                },
                placeholder = { Text("sk-...") },
                shape = ReaderGeometry.Shape,
            )

            Row(verticalAlignment=Alignment.CenterVertically){Text(I18n.t("settings.book_scripts"),modifier=Modifier.weight(1f));Switch(checked=scriptsEnabled,onCheckedChange=onScriptsEnabledChange)}
            Text(I18n.t("settings.book_scripts_desc"),style=MaterialTheme.typography.bodySmall)

            TextButton(onClick={confirmReset=true},modifier=Modifier.fillMaxWidth()) {Text(I18n.t("settings.reset_display"))}
            Text(I18n.t("settings.reset_display_scope"),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)

                } else {
                    if(appearanceLinkage != null) SettingsGroup("") {
                        AppearanceLinkageSettings(appearanceLinkage,systemAppearanceDark,ambientAppearanceDark,ambientSensorAvailable,onLinkageChange)
                    }
                    if(appearance != null) SettingsGroup("") {
                        ReadingAppearanceSettings(appearance, appearanceDark, ambientSensorAvailable, fontSize, previewFontFamily, onAppearanceChange, appearanceLinkage == null)
                    }
                    SettingsGroup(I18n.t("settings.page_turn_method")) {
                        Text(I18n.t("settings.reading_mode"))
                        SettingChoices(listOf(I18n.t("settings.scroll"), I18n.t("settings.paging")), if(scrollMode) 0 else 1) { onScrollModeChange(it == 0) }
                        Text(I18n.t("settings.page_animation"))
                        val animations = listOf("Realistic", "Slide", "Cover", "None")
                        SettingChoices(listOf("settings.realistic", "settings.slide", "settings.cover", "settings.none").map {I18n.t(it)}, animations.indexOf(pageAnimation)) { onPageAnimationChange(animations[it]) }
                    }
                    SettingsGroup(I18n.t("settings.font_group")) {
                        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.SpaceBetween) {
                            TextButton(onClick = { onFontSizeChange((fontSize - 1).coerceAtLeast(12f)) }, enabled = manualTypography && fontSize > 12, modifier = Modifier.size(48.dp).semantics { contentDescription = I18n.t("settings.font_smaller") }) { Text("A", style = MaterialTheme.typography.titleMedium) }
                            Text(fontSize.toInt().toString(), style = MaterialTheme.typography.titleLarge)
                            TextButton(onClick = { onFontSizeChange((fontSize + 1).coerceAtMost(40f)) }, enabled = manualTypography && fontSize < 40, modifier = Modifier.size(48.dp).semantics { contentDescription = I18n.t("settings.font_larger") }) { Text("A", style = MaterialTheme.typography.headlineMedium) }
                        }
                        Slider(value = fontSize.coerceIn(12f,40f), onValueChange = onFontSizeChange, valueRange = 12f..40f, enabled = manualTypography)
                        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                        Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).clickable(enabled = manualTypography) { fontPicker = true }, verticalAlignment = Alignment.CenterVertically) {
                            Text(I18n.t("settings.font").trim().trimEnd(':','：')); Spacer(Modifier.width(12.dp))
                            Text(friendlyFontLabel(fontFamilyName), modifier = Modifier.weight(1f).padding(end = 8.dp), textAlign = TextAlign.End, maxLines = 1, overflow = TextOverflow.Ellipsis, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            Icon(Icons.Default.ChevronRight, null)
                        }
                        if (!manualTypography) Text(I18n.t("settings.publisher_locked"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    SettingsGroup(I18n.t("settings.layout_style")) {
                        SettingChoices(listOf(I18n.t("settings.layout_native"), I18n.t("settings.layout_mixed"), I18n.t("settings.layout_classic")), listOf("none", "mixed", "publisher").indexOf(cssPolicy)) { onCssPolicyChange(listOf("none", "mixed", "publisher")[it]) }
                        Text(when(cssPolicy) { "none" -> I18n.t("settings.layout_native_desc"); "publisher" -> I18n.t("settings.layout_classic_desc_short"); else -> I18n.t("settings.layout_mixed_desc_short") }, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    SettingsGroup(I18n.t("settings.layout_group")) {
                        SettingValueSlider(I18n.t("settings.line_spacing"), "%.1f".format(I18n.locale, lineSpacing), lineSpacing, 1f..3f, manualTypography, onLineSpacingChange)
                        SettingValueSlider(I18n.t("settings.para_spacing"), "%.1f".format(I18n.locale, paraSpacing), paraSpacing, 0f..2f, manualTypography, onParaSpacingChange)
                        SettingValueSlider(I18n.t("settings.text_indent"), "$textIndent ${I18n.t("settings.chars")}", textIndent.toFloat(), 0f..4f, cssPolicy == "none") { onTextIndentChange(it.toInt()) }
                        SettingValueSlider(I18n.t("settings.heading_scale"), "%.1f".format(I18n.locale, titleFontScale), titleFontScale, 1f..2.5f, cssPolicy == "none", onTitleFontScaleChange)
                    }
                    SettingsGroup("") {
                        Row(Modifier.fillMaxWidth().heightIn(min=48.dp).clickable { more = true }, verticalAlignment = Alignment.CenterVertically) {
                            Column(Modifier.weight(1f)) { Text(I18n.t("settings.more")); Text(I18n.t("settings.more_summary"), style=MaterialTheme.typography.bodySmall, color=MaterialTheme.colorScheme.onSurfaceVariant) }
                            Icon(Icons.Default.ChevronRight, null)
                        }
                    }

                }
            }
        }
    }
    if(confirmReset) AlertDialog(
        onDismissRequest={confirmReset=false},
        title={Text(I18n.t("settings.reset_title"))},
        text={Text(I18n.t("settings.reset_message"))},
        confirmButton={TextButton(onClick={
                        onFontSizeChange(18f); onFontFamilyChange("Sans"); onCssPolicyChange("mixed"); onScrollModeChange(false); onPageAnimationChange("Realistic")
                        onAppearanceChange(com.zhongbai233.epub.reader.model.ReadingAppearance()); onBgColorChange(0); onFontColorChange(0); onLineSpacingChange(1.5f); onParaSpacingChange(0.5f); onTextIndentChange(2); onTitleFontScaleChange(1.5f); onClearBackgroundImage()
            confirmReset=false
        }) {Text(I18n.t("settings.reset_confirm"))}},
        dismissButton={TextButton(onClick={confirmReset=false}) {Text(I18n.t("common.cancel"))}},
    )
    if(fontPicker) ReaderSettingFontPicker(fontFamilyName, systemFonts, onDismiss = { fontPicker = false }, onSelect = onFontFamilyChange)
}

@Composable
private fun SettingsGroup(title: String, content: @Composable ColumnScope.() -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        if(title.isNotEmpty()) Text(title, modifier=Modifier.padding(start=4.dp), style=MaterialTheme.typography.labelLarge, color=MaterialTheme.colorScheme.onSurfaceVariant)
        Surface(shape=RoundedCornerShape(16.dp), color=MaterialTheme.colorScheme.surface) {
            Column(Modifier.fillMaxWidth().padding(12.dp), verticalArrangement=Arrangement.spacedBy(8.dp), content=content)
        }
    }
}
@Composable
private fun SettingChoices(labels: List<String>, selected: Int, onSelect: (Int)->Unit) {
    Row(horizontalArrangement=Arrangement.spacedBy(4.dp)) { labels.forEachIndexed { index, label ->
        OutlinedButton(onClick={onSelect(index)}, modifier=Modifier.weight(1f).heightIn(min=44.dp), shape=RoundedCornerShape(10.dp),
            contentPadding=PaddingValues(horizontal=6.dp), colors=ButtonDefaults.outlinedButtonColors(containerColor=if(selected==index) MaterialTheme.colorScheme.secondaryContainer else Color.Transparent),
            border=androidx.compose.foundation.BorderStroke(1.dp, if(selected==index) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outlineVariant)) { Text(label,maxLines=1) }
    } }
}
@Composable
private fun ThemeChoice(label: String, paper: Color, selected: Boolean, modifier: Modifier, onClick: ()->Unit) {
    Column(modifier.clickable(onClick=onClick),horizontalAlignment=Alignment.CenterHorizontally,verticalArrangement=Arrangement.spacedBy(6.dp)) {
        Box(Modifier.fillMaxWidth().height(66.dp).background(paper,RoundedCornerShape(10.dp)).border(if(selected) 2.dp else 1.dp,if(selected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outlineVariant,RoundedCornerShape(10.dp)),contentAlignment=Alignment.Center) {
            Text("Aa",style=MaterialTheme.typography.headlineSmall,fontFamily=FontFamily.Serif,color=if(paper.luminance()<0.45f) Color(0xFFE8E8E8) else Color(0xFF252525))
            if(selected) Icon(Icons.Default.CheckCircle,null,tint=MaterialTheme.colorScheme.primary,modifier=Modifier.align(Alignment.BottomEnd).padding(4.dp).size(18.dp))
        }
        Text(label,style=MaterialTheme.typography.labelMedium,maxLines=1,overflow=TextOverflow.Ellipsis)
    }
}
@Composable
private fun SettingValueSlider(title:String, label:String, value:Float, range:ClosedFloatingPointRange<Float>, enabled:Boolean, onChange:(Float)->Unit) {
    Column { Row(Modifier.fillMaxWidth()) { Text(title,style=MaterialTheme.typography.bodyMedium,modifier=Modifier.weight(1f)); Text(label,style=MaterialTheme.typography.bodyMedium,color=MaterialTheme.colorScheme.onSurfaceVariant) }
        Slider(value=value.coerceIn(range.start,range.endInclusive),onValueChange=onChange,valueRange=range,enabled=enabled)
    }
}
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ReaderSettingFontPicker(selected:String, fonts:List<FontItem>, onDismiss:()->Unit,onSelect:(String)->Unit) {
    var search by rememberSaveable {mutableStateOf("")}
    var downloads by rememberSaveable {mutableStateOf(false)}
    val catalog = remember(fonts) { (listOf(FontItem("Sans",""),FontItem("Serif",""),FontItem("Monospace",""))+fonts).distinctBy {it.displayName} }
    val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val closeSheet = rememberReaderSheetDismiss(sheet, onDismiss)
    ModalBottomSheet(onDismissRequest=onDismiss,sheetState=sheet,sheetMaxWidth=560.dp) {
        Column(Modifier.fillMaxWidth().fillMaxHeight(0.9f).padding(horizontal=16.dp)) {
            Row(verticalAlignment=Alignment.CenterVertically) {
                if(downloads) IconButton(onClick={downloads=false}) { Icon(Icons.AutoMirrored.Filled.ArrowBack, I18n.t("settings.back")) }
                Text(I18n.t(if(downloads) "fonts.library" else "settings.font"),style=MaterialTheme.typography.titleMedium,modifier=Modifier.weight(1f)); TextButton(onClick=closeSheet){Text(I18n.t("settings.done"))}
            }
            if(downloads) {
                FontLibrarySettings(selected, true, onSelect)
            } else {
            FontImportAction()
            TextButton(onClick={downloads=true}) { Text(I18n.t("fonts.library")); Spacer(Modifier.weight(1f));Icon(Icons.Default.ChevronRight,null) }
            OutlinedTextField(value=search,onValueChange={search=it},placeholder={Text(I18n.t("settings.search_font"))},singleLine=true,modifier=Modifier.fillMaxWidth(),shape=RoundedCornerShape(12.dp))
            val filtered = catalog.filter {it.displayName.contains(search,ignoreCase=true)||friendlyFontLabel(it.displayName).contains(search,ignoreCase=true)}
            if(filtered.isEmpty()) Text(I18n.t("settings.no_font_match"),modifier=Modifier.padding(16.dp))
            LazyColumn { items(filtered,key={it.displayName}) { item ->
                ListItem(headlineContent={Text(friendlyFontLabel(item.displayName),maxLines=1,overflow=TextOverflow.Ellipsis)},supportingContent={Text(fontPreviewText(item.displayName),fontFamily=previewFontFamily(item.displayName,item.path.takeIf {it.isNotEmpty()}))},trailingContent={if(selected==item.displayName)Icon(Icons.Default.Check,null)},modifier=Modifier.clickable{onSelect(item.displayName);closeSheet()})
            } }
            }
        }
    }
}

private fun friendlyFontLabel(name: String): String =
    when (name) {
        "Sans" -> I18n.t("settings.font_system_sans")
        "Serif" -> I18n.t("settings.font_system_serif")
        "Monospace" -> I18n.t("settings.font_monospace")
        else -> name
    }

private fun fontPreviewText(name: String): String =
    when (name) {
        "Monospace" -> "Aa 01 中文"
        "Serif" -> "衬线示例 Aa"
        else -> "中文预览 Aa"
    }

private fun previewFontFamily(name: String, path: String? = null): FontFamily? =
    when (name) {
        "Sans" -> FontFamily.SansSerif
        "Serif" -> FontFamily.Serif
        "Monospace" -> FontFamily.Monospace
        else -> path?.let { runCatching { FontFamily(Font(File(it))) }.getOrNull() }
    }

@Composable
private fun ColorEditorRow(
    label: String,
    color: Color,
    onColorChange: (Color) -> Unit,
) {
    fun Color.channelR(): Float = red
    fun Color.channelG(): Float = green
    fun Color.channelB(): Float = blue

    Text(label, style = MaterialTheme.typography.labelMedium)
    Spacer(Modifier.height(6.dp))
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Box(
            modifier =
                Modifier.size(28.dp)
                    .background(color, ReaderGeometry.Shape)
                    .border(1.dp, MaterialTheme.colorScheme.outlineVariant, ReaderGeometry.Shape)
        )
        Column(modifier = Modifier.weight(1f)) {
            Slider(
                value = color.channelR(),
                onValueChange = {
                    onColorChange(Color(it, color.channelG(), color.channelB(), 1f))
                },
                valueRange = 0f..1f,
            )
            Slider(
                value = color.channelG(),
                onValueChange = {
                    onColorChange(Color(color.channelR(), it, color.channelB(), 1f))
                },
                valueRange = 0f..1f,
            )
            Slider(
                value = color.channelB(),
                onValueChange = {
                    onColorChange(Color(color.channelR(), color.channelG(), it, 1f))
                },
                valueRange = 0f..1f,
            )
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun ReaderCorrectionSheet(
    cscMode: String, cscThreshold: String,
    onCscModeChange: (String) -> Unit, onCscThresholdChange: (String) -> Unit,
    cscModelReady: Boolean, cscModelLoading: Boolean, onDownloadCscModel: () -> Unit,
    contributionContent: @Composable () -> Unit,
    onDismiss: () -> Unit,
) {
    val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val closeSheet = rememberReaderSheetDismiss(sheet, onDismiss)
    ModalBottomSheet(onDismissRequest=onDismiss, sheetState=sheet, sheetMaxWidth=560.dp) {
        Column(Modifier.fillMaxWidth().fillMaxHeight(.92f)) {
            Row(Modifier.fillMaxWidth().padding(horizontal=16.dp),verticalAlignment=Alignment.CenterVertically) {
                Text(I18n.t("settings.csc_settings"),style=MaterialTheme.typography.titleMedium,modifier=Modifier.weight(1f))
                TextButton(onClick=closeSheet) {Text(I18n.t("settings.done"))}
            }
            Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(16.dp),verticalArrangement=Arrangement.spacedBy(16.dp)) {
                SettingsGroup(I18n.t("settings.csc_settings")) {
                    Text(I18n.t("settings.csc_mode"))
                    val modes=listOf("none","readonly","readwrite")
                    SettingChoices(listOf("csc.mode_none","csc.mode_readonly","csc.mode_readwrite").map {I18n.t(it)},modes.indexOf(cscMode)) {onCscModeChange(modes[it])}
                    if(cscMode!="none") {
                        Text(I18n.t("settings.csc_threshold"))
                        val thresholds=listOf("conservative","standard","aggressive")
                        SettingChoices(listOf("csc.conservative","csc.standard","csc.aggressive").map {I18n.t(it)},thresholds.indexOf(cscThreshold)) {onCscThresholdChange(thresholds[it])}
                        Row(verticalAlignment=Alignment.CenterVertically,horizontalArrangement=Arrangement.spacedBy(8.dp)) {
                            if(cscModelLoading) CircularProgressIndicator(Modifier.size(18.dp),strokeWidth=2.dp)
                            Text(I18n.t(if(cscModelReady) "csc.model_ready" else if(cscModelLoading) "csc.loading_model" else "csc.model_not_downloaded"),style=MaterialTheme.typography.bodySmall)
                        }
                        if(!cscModelReady && !cscModelLoading) TextButton(onClick=onDownloadCscModel) {Text(I18n.t("csc.download_model"))}
                    }
                }
                SettingsGroup(I18n.t("account.contribution")) {contributionContent()}
            }
        }
    }
}
