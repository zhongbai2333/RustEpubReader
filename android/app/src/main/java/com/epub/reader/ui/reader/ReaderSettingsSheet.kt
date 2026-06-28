package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.util.FontItem
import java.io.File

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun ReaderSettingsSheet(
    fontSize: Float,
    scrollMode: Boolean,
    isDarkMode: Boolean,
    bgColorIndex: Int,
    customBgColor: Color,
    fontColorIndex: Int,
    customFontColor: Color,
    fontFamilyName: String,
    pageAnimation: String,
    bgImageEnabled: Boolean,
    bgImageAlpha: Float,
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
    // TTS
    ttsVoiceName: String = "zh-CN-XiaoxiaoNeural",
    ttsRate: Int = 0,
    ttsVolume: Int = 0,
    onTtsVoiceNameChange: (String) -> Unit = {},
    onTtsRateChange: (Int) -> Unit = {},
    onTtsVolumeChange: (Int) -> Unit = {},
    // CSC
    cscMode: String = "none",
    cscThreshold: String = "standard",
    onCscModeChange: (String) -> Unit = {},
    onCscThresholdChange: (String) -> Unit = {},
    cscModelReady: Boolean = false,
    cscModelLoading: Boolean = false,
    onDownloadCscModel: () -> Unit = {}
) {
    val bgOptions = listOf(I18n.t("color.warm_white"), I18n.t("color.light_gray"), I18n.t("color.bean_green"), I18n.t("color.deep_night"), I18n.t("color.graphite"), I18n.t("settings.custom"))
    val fontColorOptions = listOf(I18n.t("color.auto"), I18n.t("color.ink_black"), I18n.t("color.dark_gray"), I18n.t("color.light_gray"), I18n.t("color.cream"), I18n.t("settings.custom"))
    val fontFamilyOptions = listOf("Sans", "Serif", "Monospace")
    val pageAnimationOptions = listOf("Slide", "Cover", "Realistic", "None")
    var fontDropdownExpanded by remember { mutableStateOf(false) }
    var fontSearchQuery by remember { mutableStateOf("") }

    val scrollState = rememberScrollState()
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(scrollState)
                .padding(horizontal = 16.dp, vertical = 8.dp)
        ) {
            Text(I18n.t("settings.title"), style = MaterialTheme.typography.titleLarge)
            Spacer(Modifier.height(12.dp))

            Text(I18n.tf1("settings.font_size", "${fontSize.toInt()}sp"))
            Slider(
                value = fontSize,
                onValueChange = onFontSizeChange,
                valueRange = 12f..40f
            )

            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(I18n.t("settings.reading_mode"), modifier = Modifier.width(72.dp))
                FilterChip(
                    selected = scrollMode,
                    onClick = { onScrollModeChange(true) },
                    label = { Text(I18n.t("settings.scroll")) }
                )
                Spacer(Modifier.width(8.dp))
                FilterChip(
                    selected = !scrollMode,
                    onClick = { onScrollModeChange(false) },
                    label = { Text(I18n.t("settings.paging")) }
                )
            }
            Spacer(Modifier.height(10.dp))

            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(I18n.t("settings.visual"), modifier = Modifier.width(72.dp))
                FilterChip(
                    selected = !isDarkMode,
                    onClick = { onDarkModeChange(false) },
                    label = { Text(I18n.t("toolbar.light_mode")) }
                )
                Spacer(Modifier.width(8.dp))
                FilterChip(
                    selected = isDarkMode,
                    onClick = { onDarkModeChange(true) },
                    label = { Text(I18n.t("toolbar.dark_mode")) }
                )
            }

            Spacer(Modifier.height(12.dp))
            Text(I18n.t("settings.bg_color"))
            Row(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.horizontalScroll(rememberScrollState())
            ) {
                bgOptions.forEachIndexed { index, name ->
                    FilterChip(
                        selected = bgColorIndex == index,
                        onClick = { onBgColorChange(index) },
                        label = { Text(name) }
                    )
                }
            }
            if (bgColorIndex == bgOptions.lastIndex) {
                Spacer(Modifier.height(8.dp))
                ColorEditorRow(
                    label = I18n.t("settings.custom_bg"),
                    color = customBgColor,
                    onColorChange = onCustomBgColorChange
                )
            }

            Spacer(Modifier.height(12.dp))
            Text(I18n.t("settings.font_color"))
            Row(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.horizontalScroll(rememberScrollState())
            ) {
                fontColorOptions.forEachIndexed { index, name ->
                    FilterChip(
                        selected = fontColorIndex == index,
                        onClick = { onFontColorChange(index) },
                        label = { Text(name) }
                    )
                }
            }
            if (fontColorIndex == fontColorOptions.lastIndex) {
                Spacer(Modifier.height(8.dp))
                ColorEditorRow(
                    label = I18n.t("settings.custom_font_color"),
                    color = customFontColor,
                    onColorChange = onCustomFontColorChange
                )
            }

            Spacer(Modifier.height(12.dp))
            Text(I18n.t("settings.font"))
            Box {
                OutlinedButton(
                    onClick = { fontDropdownExpanded = true },
                    modifier = Modifier.fillMaxWidth()
                ) {
                    Text(fontFamilyName, modifier = Modifier.weight(1f))
                    Icon(Icons.Default.ArrowDropDown, contentDescription = null)
                }
                DropdownMenu(
                    expanded = fontDropdownExpanded,
                    onDismissRequest = { fontDropdownExpanded = false; fontSearchQuery = "" },
                    modifier = Modifier.fillMaxWidth(0.9f)
                ) {
                    // 搜索框
                    OutlinedTextField(
                        value = fontSearchQuery,
                        onValueChange = { fontSearchQuery = it },
                        placeholder = { Text(I18n.t("settings.search_font")) },
                        singleLine = true,
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(horizontal = 8.dp, vertical = 4.dp)
                    )
                    val q = fontSearchQuery.lowercase()
                    // 内置字体
                    fontFamilyOptions.filter { q.isEmpty() || it.lowercase().contains(q) }.forEach { fam ->
                        DropdownMenuItem(
                            text = {
                                Column {
                                    Text(friendlyFontLabel(fam))
                                    Text(
                                        fontPreviewText(fam),
                                        style = MaterialTheme.typography.bodySmall,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                                        fontFamily = previewFontFamily(fam)
                                    )
                                }
                            },
                            onClick = {
                                onFontFamilyChange(fam)
                                fontDropdownExpanded = false
                                fontSearchQuery = ""
                            },
                            leadingIcon = if (fontFamilyName == fam) {
                                { Icon(Icons.Default.Check, contentDescription = null) }
                            } else null
                        )
                    }
                    // 系统字体
                    val filteredSystem = systemFonts.filter { q.isEmpty() || it.displayName.lowercase().contains(q) }
                    if (filteredSystem.isNotEmpty()) {
                        HorizontalDivider()
                        filteredSystem.forEach { item ->
                            DropdownMenuItem(
                                text = {
                                    Column {
                                        Text(item.displayName)
                                        Text(
                                            fontPreviewText(item.displayName),
                                            style = MaterialTheme.typography.bodySmall,
                                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                                            fontFamily = previewFontFamily(item.displayName, item.path)
                                        )
                                    }
                                },
                                onClick = {
                                    onFontFamilyChange(item.displayName)
                                    fontDropdownExpanded = false
                                    fontSearchQuery = ""
                                },
                                leadingIcon = if (fontFamilyName == item.displayName) {
                                    { Icon(Icons.Default.Check, contentDescription = null) }
                                } else null
                            )
                        }
                    }
                }
            }

            Spacer(Modifier.height(12.dp))
            Text(I18n.t("settings.page_animation"))
            Row(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.horizontalScroll(rememberScrollState())
            ) {
                pageAnimationOptions.forEach { mode ->
                    FilterChip(
                        selected = pageAnimation == mode,
                        onClick = { onPageAnimationChange(mode) },
                        label = {
                            Text(
                                when (mode) {
                                    "Slide" -> I18n.t("settings.slide")
                                    "Cover" -> I18n.t("settings.cover")
                                    "Realistic" -> I18n.t("settings.realistic")
                                    else -> I18n.t("settings.none")
                                }
                            )
                        }
                    )
                }
            }

            Spacer(Modifier.height(12.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(I18n.t("settings.bg_image"), modifier = Modifier.weight(1f))
                TextButton(onClick = onPickBackgroundImage) { Text(I18n.t("settings.pick_bg_image")) }
                if (bgImageEnabled) {
                    TextButton(onClick = onClearBackgroundImage) { Text(I18n.t("settings.clear_bg_image")) }
                }
            }

            if (bgImageEnabled) {
                Text("${I18n.t("settings.opacity")}: ${(bgImageAlpha * 100).toInt()}%")
                Slider(
                    value = bgImageAlpha,
                    onValueChange = onBgImageAlphaChange,
                    valueRange = 0f..1f
                )
            }

            // ─── 语言选择 ───
            Spacer(Modifier.height(12.dp))
            Text(I18n.t("settings.language"))
            Row(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.horizontalScroll(rememberScrollState())
            ) {
                I18n.availableLanguages.forEach { (code, label) ->
                    val selected = if (I18n.isAuto) code == "auto"
                                   else code == I18n.currentCode
                    FilterChip(
                        selected = selected,
                        onClick = { onLanguageChange(code) },
                        label = { Text(label) }
                    )
                }
            }

            Spacer(Modifier.height(12.dp))
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text("沉浸状态信息")
                    Text(
                        "在阅读界面左下角显示时间和电量",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
                Switch(
                    checked = showImmersiveStatus,
                    onCheckedChange = onShowImmersiveStatusChange
                )
            }

            // ─── 排版设置 ───
            Spacer(Modifier.height(16.dp))
            HorizontalDivider()
            Spacer(Modifier.height(12.dp))
            Text(I18n.t("settings.typography"), style = MaterialTheme.typography.titleMedium)
            Spacer(Modifier.height(8.dp))

            Text("${I18n.t("settings.line_spacing")}: ${"%.1f".format(lineSpacing)}")
            Slider(
                value = lineSpacing,
                onValueChange = onLineSpacingChange,
                valueRange = 1.0f..3.0f,
                steps = 19
            )

            Text("${I18n.t("settings.para_spacing")}: ${"%.1f".format(paraSpacing)}")
            Slider(
                value = paraSpacing,
                onValueChange = onParaSpacingChange,
                valueRange = 0.0f..2.0f,
                steps = 19
            )

            Text("${I18n.t("settings.text_indent")}: $textIndent ${I18n.t("settings.chars")}")
            Slider(
                value = textIndent.toFloat(),
                onValueChange = { onTextIndentChange(it.toInt()) },
                valueRange = 0f..4f,
                steps = 3
            )

            Text("标题字号倍率: ${"%.1f".format(titleFontScale)}x")
            Slider(
                value = titleFontScale,
                onValueChange = onTitleFontScaleChange,
                valueRange = 1.0f..2.5f,
                steps = 14
            )

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
                placeholder = { Text("https://api.example.com/translate") }
            )
            Spacer(Modifier.height(4.dp))
            var translateKeyVisible by remember { mutableStateOf(false) }
            OutlinedTextField(
                value = translateApiKey,
                onValueChange = onTranslateApiKeyChange,
                label = { Text(I18n.t("settings.translate_api_key")) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
                visualTransformation = if (translateKeyVisible)
                    androidx.compose.ui.text.input.VisualTransformation.None
                else
                    androidx.compose.ui.text.input.PasswordVisualTransformation(),
                trailingIcon = {
                    IconButton(onClick = { translateKeyVisible = !translateKeyVisible }) {
                        Icon(
                            if (translateKeyVisible) Icons.Default.VisibilityOff else Icons.Default.Visibility,
                            contentDescription = null
                        )
                    }
                },
                placeholder = { Text("sk-...") }
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
                placeholder = { Text("https://api.example.com/dictionary") }
            )
            Spacer(Modifier.height(4.dp))
            var dictKeyVisible by remember { mutableStateOf(false) }
            OutlinedTextField(
                value = dictionaryApiKey,
                onValueChange = onDictionaryApiKeyChange,
                label = { Text(I18n.t("settings.dictionary_api_key")) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
                visualTransformation = if (dictKeyVisible)
                    androidx.compose.ui.text.input.VisualTransformation.None
                else
                    androidx.compose.ui.text.input.PasswordVisualTransformation(),
                trailingIcon = {
                    IconButton(onClick = { dictKeyVisible = !dictKeyVisible }) {
                        Icon(
                            if (dictKeyVisible) Icons.Default.VisibilityOff else Icons.Default.Visibility,
                            contentDescription = null
                        )
                    }
                },
                placeholder = { Text("sk-...") }
            )

            // ─── TTS 设置 ───
            Spacer(Modifier.height(16.dp))
            HorizontalDivider()
            Spacer(Modifier.height(12.dp))
            Text(I18n.t("settings.tts_settings"), style = MaterialTheme.typography.titleMedium)
            Spacer(Modifier.height(8.dp))

            val voicePresets = listOf(
                "zh-CN-XiaoxiaoNeural" to "晓晓 (女)",
                "zh-CN-YunyangNeural" to "云扬 (男)",
                "zh-CN-XiaoyiNeural" to "晓依 (女)",
                "zh-CN-YunjianNeural" to "云健 (男)",
                "zh-CN-YunxiNeural" to "云希 (男)",
                "zh-CN-XiaochenNeural" to "晓辰 (女)",
                "zh-CN-XiaohanNeural" to "晓涵 (女)",
                "zh-CN-XiaomoNeural" to "晓墨 (女)",
                "zh-CN-XiaoruiNeural" to "晓睿 (女)",
                "zh-CN-XiaoshuangNeural" to "晓双 (女)",
                "en-US-AriaNeural" to "Aria (EN Female)",
                "en-US-GuyNeural" to "Guy (EN Male)",
                "ja-JP-NanamiNeural" to "Nanami (JP Female)"
            )
            val rateOptions = listOf(-50 to "-50%", -25 to "-25%", 0 to I18n.t("tts.rate_normal"), 25 to "+25%", 50 to "+50%", 100 to "+100%")
            val volumeOptions = listOf(-50 to "-50%", -25 to "-25%", 0 to I18n.t("tts.rate_normal"), 25 to "+25%", 50 to "+50%")

            var voiceDropdownExpanded by remember { mutableStateOf(false) }
            Text(I18n.t("settings.tts_voice"))
            Box {
                OutlinedButton(
                    onClick = { voiceDropdownExpanded = true },
                    modifier = Modifier.fillMaxWidth()
                ) {
                    Text(
                        voicePresets.firstOrNull { it.first == ttsVoiceName }?.second ?: ttsVoiceName,
                        modifier = Modifier.weight(1f)
                    )
                    Icon(Icons.Default.ArrowDropDown, contentDescription = null)
                }
                DropdownMenu(
                    expanded = voiceDropdownExpanded,
                    onDismissRequest = { voiceDropdownExpanded = false },
                    modifier = Modifier.fillMaxWidth(0.9f)
                ) {
                    voicePresets.forEach { (name, label) ->
                        DropdownMenuItem(
                            text = { Text(label) },
                            onClick = {
                                onTtsVoiceNameChange(name)
                                voiceDropdownExpanded = false
                            },
                            leadingIcon = if (ttsVoiceName == name) {
                                { Icon(Icons.Default.Check, contentDescription = null) }
                            } else null
                        )
                    }
                }
            }

            Spacer(Modifier.height(8.dp))
            Text(I18n.t("settings.tts_rate"))
            Row(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                modifier = Modifier.horizontalScroll(rememberScrollState())
            ) {
                rateOptions.forEach { (value, label) ->
                    FilterChip(
                        selected = ttsRate == value,
                        onClick = { onTtsRateChange(value) },
                        label = { Text(label) }
                    )
                }
            }

            Spacer(Modifier.height(8.dp))
            Text(I18n.t("settings.tts_volume"))
            Row(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                modifier = Modifier.horizontalScroll(rememberScrollState())
            ) {
                volumeOptions.forEach { (value, label) ->
                    FilterChip(
                        selected = ttsVolume == value,
                        onClick = { onTtsVolumeChange(value) },
                        label = { Text(label) }
                    )
                }
            }

            // ─── CSC 中文纠错设置 ───
            Spacer(Modifier.height(16.dp))
            HorizontalDivider()
            Spacer(Modifier.height(12.dp))
            Text(I18n.t("settings.csc_settings"), style = MaterialTheme.typography.titleMedium)
            Spacer(Modifier.height(8.dp))

            Text(I18n.t("settings.csc_mode"))
            Row(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.horizontalScroll(rememberScrollState())
            ) {
                listOf("none" to I18n.t("csc.mode_none"), "readonly" to I18n.t("csc.mode_readonly"), "readwrite" to I18n.t("csc.mode_readwrite")).forEach { (mode, label) ->
                    FilterChip(
                        selected = cscMode == mode,
                        onClick = { onCscModeChange(mode) },
                        label = { Text(label) }
                    )
                }
            }

            if (cscMode != "none") {
                Spacer(Modifier.height(8.dp))
                Text(I18n.t("settings.csc_threshold"))
                Row(
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    modifier = Modifier.horizontalScroll(rememberScrollState())
                ) {
                    listOf("conservative" to I18n.t("csc.conservative"), "standard" to I18n.t("csc.standard"), "aggressive" to I18n.t("csc.aggressive")).forEach { (th, label) ->
                        FilterChip(
                            selected = cscThreshold == th,
                            onClick = { onCscThresholdChange(th) },
                            label = { Text(label) }
                        )
                    }
                }

                // Model status
                Spacer(Modifier.height(8.dp))
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    if (cscModelReady) {
                        Icon(Icons.Default.CheckCircle, null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(18.dp))
                        Text(I18n.t("csc.model_ready"), style = MaterialTheme.typography.bodySmall)
                    } else if (cscModelLoading) {
                        CircularProgressIndicator(modifier = Modifier.size(18.dp), strokeWidth = 2.dp)
                        Text(I18n.t("csc.loading_model"), style = MaterialTheme.typography.bodySmall)
                    } else {
                        Icon(Icons.Default.Warning, null, tint = MaterialTheme.colorScheme.error, modifier = Modifier.size(18.dp))
                        Text(I18n.t("csc.model_not_downloaded"), style = MaterialTheme.typography.bodySmall)
                        TextButton(onClick = onDownloadCscModel) {
                            Text(I18n.t("csc.download_model"))
                        }
                    }
                }
            }

            Spacer(Modifier.height(20.dp))
        }
    }
}

private fun friendlyFontLabel(name: String): String = when (name) {
    "Sans" -> "系统无衬线"
    "Serif" -> "系统衬线"
    "Monospace" -> "等宽字体"
    else -> name
}

private fun fontPreviewText(name: String): String = when (name) {
    "Monospace" -> "Aa 01 中文"
    "Serif" -> "衬线示例 Aa"
    else -> "中文预览 Aa"
}

private fun previewFontFamily(name: String, path: String? = null): FontFamily? = when (name) {
    "Sans" -> FontFamily.SansSerif
    "Serif" -> FontFamily.Serif
    "Monospace" -> FontFamily.Monospace
    else -> path?.let { runCatching { FontFamily(Font(File(it))) }.getOrNull() }
}

@Composable
private fun ColorEditorRow(
    label: String,
    color: Color,
    onColorChange: (Color) -> Unit
) {
    fun Color.channelR(): Float = red
    fun Color.channelG(): Float = green
    fun Color.channelB(): Float = blue

    Text(label, style = MaterialTheme.typography.labelMedium)
    Spacer(Modifier.height(6.dp))
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp)
    ) {
        Box(
            modifier = Modifier
                .size(28.dp)
                .background(color, RoundedCornerShape(6.dp))
                .border(1.dp, MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(6.dp))
        )
        Column(modifier = Modifier.weight(1f)) {
            Slider(
                value = color.channelR(),
                onValueChange = { onColorChange(Color(it, color.channelG(), color.channelB(), 1f)) },
                valueRange = 0f..1f
            )
            Slider(
                value = color.channelG(),
                onValueChange = { onColorChange(Color(color.channelR(), it, color.channelB(), 1f)) },
                valueRange = 0f..1f
            )
            Slider(
                value = color.channelB(),
                onValueChange = { onColorChange(Color(color.channelR(), color.channelG(), it, 1f)) },
                valueRange = 0f..1f
            )
        }
    }
}

