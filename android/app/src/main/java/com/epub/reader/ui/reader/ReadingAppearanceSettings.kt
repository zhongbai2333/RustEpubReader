package com.zhongbai233.epub.reader.ui.reader

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.ImageDecoder
import android.net.Uri
import android.os.Build
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.drag
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.DialogWindowProvider
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import coil.compose.AsyncImage
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.model.*
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import java.util.UUID
import kotlin.math.roundToInt

@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun ReadingAppearanceSettings(value: ReadingAppearance, activeDark: Boolean, sensorAvailable: Boolean, fontSize: Float, fontFamily: FontFamily, onChange: (ReadingAppearance) -> Unit, showLegacyMode: Boolean = true) {
    var editing by remember { mutableStateOf<ReadingStyle?>(null) }
    val context = LocalContext.current
    remember(context) { ReaderAppearanceImages.configure(context) }
    if(!showLegacyMode) Text(I18n.t("appearance.styles"), style = MaterialTheme.typography.labelLarge)
    ReadingAppearanceSample(value.style.variant(activeDark), fontSize, fontFamily)
    if(showLegacyMode) {
    Text(I18n.t("appearance.mode"), style = MaterialTheme.typography.labelLarge)
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        ReadingDisplayMode.entries.forEach { mode ->
            FilterChip(selected = value.mode == mode, onClick = { onChange(value.copy(mode = mode)) }, label = { Text(I18n.t("appearance.mode_${mode.name}")) }, modifier = Modifier.weight(1f))
        }
    }
    if(value.mode == ReadingDisplayMode.automatic) Text(I18n.t(if(sensorAvailable) "appearance.ambient" else "appearance.fallback"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    if(value.mode == ReadingDisplayMode.automatic) Text(I18n.t(if(activeDark) "appearance.current_dark" else "appearance.current_light"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
    if(showLegacyMode) Text(I18n.t("appearance.styles"), style = MaterialTheme.typography.labelLarge)
    val minimumCardWidth = (86 * LocalDensity.current.fontScale).dp
    FlowRow(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalArrangement = Arrangement.spacedBy(10.dp), maxItemsInEachRow = 3) {
        value.choices.forEach { style ->
            val variant = style.variant(activeDark)
            val creating = style.id == "custom" && value.custom == null
            val label = I18n.t("appearance.style_${style.id}")
            Column(Modifier.widthIn(min = minimumCardWidth).weight(1f).clip(RoundedCornerShape(10.dp)).border(if(value.selected == style.id) 2.dp else 1.dp, if(value.selected == style.id) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(10.dp)).clickable(role = Role.Button) {
                if(creating) editing = ReadingStyle.customTemplate else onChange(value.copy(selected = style.id))
            }.semantics { contentDescription = if(creating) I18n.t("appearance.create_custom") else label; selected = value.selected == style.id }) {
                Box(Modifier.fillMaxWidth().height(64.dp).background(if(creating) MaterialTheme.colorScheme.surfaceContainer else Color(variant.paper)), contentAlignment = Alignment.Center) {
                    Text(if(creating) "+" else "Aa", color = if(creating) MaterialTheme.colorScheme.primary else Color(variant.ink), fontFamily = if(creating) FontFamily.Default else FontFamily.Serif, fontSize = 24.sp)
                }
                Text((if(value.selected == style.id) "✓ " else "") + label, style = MaterialTheme.typography.labelMedium, modifier = Modifier.padding(8.dp))
            }
        }
    }
    TextButton(onClick = { editing = value.style }, modifier = Modifier.fillMaxWidth()) { Text(I18n.t("appearance.adjust")) }
    editing?.let { target -> ReadingStyleEditor(target, activeDark, fontSize, fontFamily, onDismiss = { editing = null }) { draft ->
        val saved = value.saved(draft)
        onChange(saved)
        // Only owned files with no remaining reference in any theme may be removed.
        (value.imageReferences - saved.imageReferences).forEach { ReaderAppearanceImages.remove(it) }
        editing = null
    } }

}

@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun ReadingStyleEditor(initial: ReadingStyle, initiallyDark: Boolean, fontSize: Float, fontFamily: FontFamily, onDismiss: () -> Unit, onSave: (ReadingStyle) -> Unit) {
    var draft by remember { mutableStateOf(initial) }
    var dark by remember { mutableStateOf(initiallyDark) }
    val variant = draft.variant(dark)
    var paperHex by remember(dark) { mutableStateOf("%06X".format(variant.paper and 0xFFFFFF)) }
    var inkHex by remember(dark) { mutableStateOf("%06X".format(variant.ink and 0xFFFFFF)) }
    var pickingInk by remember { mutableStateOf<Boolean?>(null) }
    fun valid(text: String) = text.length == 6 && text.all { it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F' }
    var token by remember { mutableStateOf(0) }
    var pendingSide by remember { mutableStateOf(false) }
    var pendingToken by remember { mutableStateOf(0) }
    var importing by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf(false) }
    var copied by remember { mutableStateOf(false) }
    var alive by remember { mutableStateOf(true) }
    val added = remember { mutableSetOf<String>() }
    var retained by remember { mutableStateOf<Set<String>>(emptySet()) }
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    fun import(uri: Uri?) {
        if(uri == null) return
        val request = pendingToken; val side = pendingSide
        importing = true; error = false
        scope.launch {
            var file: String? = null
            try {
                withContext(Dispatchers.IO) { file = ReaderAppearanceImages.import(context, uri) }
                val result = requireNotNull(file)
                if(alive && request == token && side == dark) {
                    added.add(result); draft = draft.replacing(side, draft.variant(side).copy(image = result)); copied = false
                } else ReaderAppearanceImages.remove(result)
            } catch(_: Exception) { file?.let { ReaderAppearanceImages.remove(it) }; if(alive) error = true }
            finally { importing = false }
        }
    }
    val photo = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { import(it) }
    val document = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { import(it) }
    fun begin() { token++; pendingToken = token; pendingSide = dark }
    DisposableEffect(Unit) { onDispose { alive = false; token++; added.filterNot { it in retained }.forEach { ReaderAppearanceImages.remove(it) } } }
    Dialog(onDismissRequest = onDismiss, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        BackHandler(onBack = onDismiss)
        val view = LocalView.current
        val lightBars = MaterialTheme.colorScheme.surface.luminance() > .45f
        DisposableEffect(view, lightBars) {
            (view.parent as? DialogWindowProvider)?.window?.let { window ->
                WindowCompat.getInsetsController(window, view).apply {
                    show(WindowInsetsCompat.Type.systemBars())
                    isAppearanceLightStatusBars = lightBars
                    isAppearanceLightNavigationBars = lightBars
                }
            }
            onDispose {}
        }
        Surface(Modifier.fillMaxSize()) {
            Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.statusBarsIgnoringVisibility.union(WindowInsets.navigationBarsIgnoringVisibility))) {
                Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    TextButton(onClick = onDismiss) { Text(I18n.t("common.cancel")) }
                    Text(if(initial.id == "custom") I18n.t("appearance.custom_title") else I18n.tf1("appearance.adjust_named", I18n.t("appearance.style_${initial.id}")), style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
                    TextButton(enabled = !importing && valid(paperHex) && valid(inkHex), onClick = { retained = setOfNotNull(draft.light.image, draft.dark.image); onSave(draft) }) { Text(I18n.t("common.save")) }
                }
                Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        listOf(false, true).forEach { side ->
                            FilterChip(selected = dark == side, onClick = { dark = side; token++; copied = false }, label = { Text(I18n.t(if(side) "appearance.edit_dark" else "appearance.edit_light")) }, modifier = Modifier.weight(1f))
                        }
                    }
                    ReadingAppearanceSample(variant, fontSize, fontFamily, dark)
                    OutlinedTextField(value = paperHex, onValueChange = { text -> paperHex = text.take(6).uppercase(); if(valid(paperHex)) draft = draft.replacing(dark, variant.copy(paper = (0xFF000000L or paperHex.toLong(16)).toInt())) }, label = { Text(I18n.t("settings.background_color")) }, prefix = { Text("#") }, trailingIcon = { AppearanceColorButton(variant.paper, I18n.t("settings.background_color")) { pickingInk = false } }, singleLine = true, isError = !valid(paperHex), modifier = Modifier.fillMaxWidth())
                    AppearanceColorSwatches(ReaderColorPalette.paper, variant.paper) { color ->
                        paperHex = "%06X".format(color and 0xFFFFFF); draft = draft.replacing(dark, draft.variant(dark).copy(paper = color))
                    }
                    OutlinedTextField(value = inkHex, onValueChange = { text -> inkHex = text.take(6).uppercase(); if(valid(inkHex)) draft = draft.replacing(dark, draft.variant(dark).copy(ink = (0xFF000000L or inkHex.toLong(16)).toInt())) }, label = { Text(I18n.t("settings.text_color")) }, prefix = { Text("#") }, trailingIcon = { AppearanceColorButton(variant.ink, I18n.t("settings.text_color")) { pickingInk = true } }, singleLine = true, isError = !valid(inkHex), modifier = Modifier.fillMaxWidth())
                    AppearanceColorSwatches(ReaderColorPalette.ink, variant.ink) { color ->
                        inkHex = "%06X".format(color and 0xFFFFFF); draft = draft.replacing(dark, draft.variant(dark).copy(ink = color))
                    }
                    val a = Color(variant.paper).luminance(); val b = Color(variant.ink).luminance()
                    if((maxOf(a,b)+.05f)/(minOf(a,b)+.05f) < 4.5f) Text(I18n.t("appearance.contrast"), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
                    HorizontalDivider()
                    Text(I18n.t("common.background_image"), style = MaterialTheme.typography.titleSmall)
                    if(variant.image != null) AsyncImage(model = variant.image, contentDescription = I18n.t("settings.image_selected"), contentScale = ContentScale.Crop, modifier = Modifier.size(64.dp).clip(RoundedCornerShape(10.dp))) else Text(I18n.t("settings.image_empty"), color = MaterialTheme.colorScheme.onSurfaceVariant)
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedButton(enabled = !importing, onClick = { begin(); photo.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) }) { Text(I18n.t("common.select_from_photos")) }
                        OutlinedButton(enabled = !importing, onClick = { begin(); document.launch(arrayOf("image/*")) }) { Text(I18n.t("common.select_from_files")) }
                    }
                    if(importing) LinearProgressIndicator(Modifier.fillMaxWidth())
                    if(error) Text(I18n.t("appearance.image_error"), color = MaterialTheme.colorScheme.error)
                    if(variant.image != null) {
                        Row(Modifier.fillMaxWidth()) { Text(I18n.t("appearance.strength"), modifier = Modifier.weight(1f)); Text("${(variant.strength * 100).roundToInt()}%") }
                        Slider(value = variant.strength, onValueChange = { draft = draft.replacing(dark, draft.variant(dark).copy(strength = it)) }, modifier = Modifier.semantics { contentDescription = I18n.t("appearance.strength") })
                        TextButton(onClick = { draft = draft.replacing(!dark, draft.variant(!dark).copy(image = variant.image)); copied = true }) { Text(I18n.t(if(dark) "appearance.copy_light" else "appearance.copy_dark")) }
                        TextButton(onClick = { token++; draft = draft.replacing(dark, draft.variant(dark).copy(image = null)); copied = false }, colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error)) { Text(I18n.t("common.remove_image")) }
                    }
                    if(copied) Text(I18n.t("appearance.copied"), style = MaterialTheme.typography.bodySmall)
                    HorizontalDivider()
                    TextButton(enabled = !importing, onClick = {
                        token++; copied = false; error = false
                        draft = ReadingStyle.presets.find { it.id == initial.id } ?: ReadingStyle.customTemplate
                        paperHex = "%06X".format(draft.variant(dark).paper and 0xFFFFFF)
                        inkHex = "%06X".format(draft.variant(dark).ink and 0xFFFFFF)
                    }) { Text(I18n.t(if(initial.id == "custom") "appearance.reset_custom" else "appearance.restore_preset")) }
                    Text(I18n.t("appearance.reset_note"), color = MaterialTheme.colorScheme.onSurfaceVariant, style = MaterialTheme.typography.bodySmall)
                    Text(I18n.t(if(initial.id == "custom") "appearance.custom_save_note" else "appearance.preset_save_note"), color = MaterialTheme.colorScheme.onSurfaceVariant, style = MaterialTheme.typography.bodySmall)
                    Spacer(Modifier.height(64.dp))
                }
            }
        }
    }
    pickingInk?.let { ink ->
        AppearanceColorPicker(if(ink) variant.ink else variant.paper,
            I18n.t(if(ink) "settings.text_color" else "settings.background_color"),
            if(ink) ReaderColorPalette.ink else ReaderColorPalette.paper,
            onDismiss = { pickingInk = null }, onChoose = { color ->
                if(ink) { inkHex = "%06X".format(color and 0xFFFFFF); draft = draft.replacing(dark, draft.variant(dark).copy(ink=color)) }
                else { paperHex = "%06X".format(color and 0xFFFFFF); draft = draft.replacing(dark, draft.variant(dark).copy(paper=color)) }
                pickingInk = null
            })
    }
}

@Composable
internal fun ReadingAppearanceSample(variant: ReadingAppearanceVariant, fontSize: Float, fontFamily: FontFamily, editingDark: Boolean? = null) {
    Box(Modifier.fillMaxWidth().background(Color(variant.paper)).clip(RoundedCornerShape(4.dp))) {
        if(variant.image != null) AsyncImage(model = variant.image, contentDescription = null, contentScale = ContentScale.Crop, alpha = variant.strength, modifier = Modifier.matchParentSize())
        Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            if(editingDark != null) Text(I18n.t("appearance.preview") + " · " + I18n.t(if(editingDark) "appearance.mode_dark" else "appearance.mode_light"), style = MaterialTheme.typography.labelSmall, color = Color(variant.ink))
            Text(I18n.t("appearance.sample"), fontSize = fontSize.sp, lineHeight = (fontSize * 1.65f).sp, fontFamily = fontFamily, color = Color(variant.ink))
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun AppearanceColorSwatches(colors: List<Int>, selected: Int, onChange: (Int) -> Unit) {
    FlowRow(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        colors.forEach { color ->
            val hex = "#" + "%06X".format(color and 0xFFFFFF)
            IconButton(onClick = { onChange(color) }, modifier = Modifier.size(44.dp).semantics { contentDescription = I18n.t("appearance.hex") + " " + hex; this.selected = selected == color }) {
                Box(Modifier.size(28.dp).background(Color(color), RoundedCornerShape(6.dp)).border(if(selected == color) 3.dp else 1.dp, if(selected == color) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface.copy(alpha = .55f), RoundedCornerShape(6.dp)))
            }
        }
    }
}

internal object ReaderAppearanceImages {
    private var directory: File? = null
    fun configure(context: Context) { directory = File(context.filesDir, "reader-appearance") }
    fun import(context: Context, uri: Uri): String {
        val dir = File(context.filesDir, "reader-appearance").also { it.mkdirs() }; directory = dir
        val bitmap = if(Build.VERSION.SDK_INT >= 28) ImageDecoder.decodeBitmap(ImageDecoder.createSource(context.contentResolver, uri)) { decoder, info, _ ->
            val scale = minOf(1.0, 2048.0 / maxOf(info.size.width, info.size.height))
            decoder.setTargetSize(maxOf(1, (info.size.width * scale).toInt()), maxOf(1, (info.size.height * scale).toInt()))
            decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
        } else {
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            context.contentResolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, bounds) }
            require(bounds.outWidth > 0 && bounds.outHeight > 0)
            val options = BitmapFactory.Options().apply { while(maxOf(bounds.outWidth, bounds.outHeight) / inSampleSize > 2048) inSampleSize *= 2 }
            requireNotNull(context.contentResolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, options) })
        }
        val file = File(dir, "background-${UUID.randomUUID()}.png")
        try { file.outputStream().use { check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)) }; return Uri.fromFile(file).toString() }
        catch(error: Exception) { file.delete(); throw error }
        finally { bitmap.recycle() }
    }
    fun remove(uri: String) {
        val dir = directory ?: return
        val parsed = Uri.parse(uri)
        if(parsed.scheme != "file") return
        val file = parsed.path?.let { File(it) } ?: return
        if(file.name.startsWith("background-") && file.canonicalFile.parentFile == dir.canonicalFile) file.delete()
    }
}


private object ReaderColorPalette {
    val paper = listOf(0xFFF7F2E8,0xFFFFFFFF,0xFFF1F3F5,0xFFE2EADC,0xFFEEDDC4,0xFFE7EFF7,0xFFF3E6EA,0xFFE8E2F0,0xFF1A1A1C,0xFF24262B,0xFF1D2823,0xFF29241E).map { it.toInt() }
    val ink = listOf(0xFF1A1A1A,0xFF2D2D2D,0xFF5B4636,0xFF314C3B,0xFF284B63,0xFF604A70,0xFF7C3E45,0xFF7A5726,0xFFDCDCDC,0xFFE6E6E6,0xFFF1EAD8,0xFFFFFFFF).map { it.toInt() }
}
@Composable private fun AppearanceColorButton(color: Int, label: String, open: () -> Unit) {
    IconButton(onClick=open, modifier=Modifier.size(48.dp).semantics { contentDescription=I18n.t("settings.custom")+" · "+label }) {
        Box(Modifier.size(30.dp).background(Color(color),RoundedCornerShape(8.dp)).border(1.dp,MaterialTheme.colorScheme.outline,RoundedCornerShape(8.dp)))
    }
}
@OptIn(ExperimentalMaterial3Api::class)
@Composable private fun AppearanceColorPicker(initial: Int, title: String, presets: List<Int>, onDismiss: () -> Unit, onChoose: (Int) -> Unit) {
    val initialHsv = remember(initial) { FloatArray(3).also { android.graphics.Color.colorToHSV(initial,it) } }
    var hue by remember(initial) { mutableFloatStateOf(initialHsv[0]) }
    var saturation by remember(initial) { mutableFloatStateOf(initialHsv[1]) }
    var brightness by remember(initial) { mutableFloatStateOf(initialHsv[2]) }
    val picked = Color.hsv(hue,saturation,brightness).toArgb()
    var hex by remember { mutableStateOf("%06X".format(picked and 0xFFFFFF)) }
    fun apply(color: Int) {
        val hsv=FloatArray(3); android.graphics.Color.colorToHSV(color,hsv)
        hue=hsv[0]; saturation=hsv[1]; brightness=hsv[2]
        hex="%06X".format(color and 0xFFFFFF)
    }
    LaunchedEffect(picked) { hex="%06X".format(picked and 0xFFFFFF) }
    AlertDialog(onDismissRequest=onDismiss, title={Text(title)},
        text={ Column(Modifier.verticalScroll(rememberScrollState()),verticalArrangement=Arrangement.spacedBy(12.dp)) {
            Canvas(Modifier.fillMaxWidth().aspectRatio(1.7f).clip(RoundedCornerShape(8.dp))
                .background(Color.hsv(hue,1f,1f))
                .semantics { contentDescription=title+" #"+"%06X".format(picked and 0xFFFFFF) }
                .pointerInput(Unit) {
                    fun pick(position: Offset) {
                        saturation=(position.x/size.width.coerceAtLeast(1)).coerceIn(0f,1f)
                        brightness=1f-(position.y/size.height.coerceAtLeast(1)).coerceIn(0f,1f)
                    }
                    awaitEachGesture { val down=awaitFirstDown();pick(down.position);down.consume();drag(down.id) { pick(it.position);it.consume() } }
                }) {
                drawRect(Brush.horizontalGradient(listOf(Color.White,Color.Transparent)))
                drawRect(Brush.verticalGradient(listOf(Color.Transparent,Color.Black)))
                val point=Offset(saturation*size.width,(1f-brightness)*size.height)
                drawCircle(Color.Black,8.dp.toPx(),point,style=Stroke(3.dp.toPx()))
                drawCircle(Color.White,8.dp.toPx(),point,style=Stroke(1.5.dp.toPx()))
            }
            Slider(value=hue,onValueChange={hue=it},valueRange=0f..360f,
                modifier=Modifier.semantics {contentDescription=title},
                thumb={Box(Modifier.size(24.dp).background(Color.hsv(hue,1f,1f),CircleShape).border(3.dp,MaterialTheme.colorScheme.surface,CircleShape).border(1.dp,MaterialTheme.colorScheme.outline,CircleShape))},
                track={Box(Modifier.fillMaxWidth().height(12.dp).background(Brush.horizontalGradient(listOf(Color.Red,Color.Yellow,Color.Green,Color.Cyan,Color.Blue,Color.Magenta,Color.Red)),RoundedCornerShape(6.dp)))})
            OutlinedTextField(value=hex,onValueChange={raw ->
                hex=raw.removePrefix("#").take(6).uppercase()
                if(hex.length==6 && hex.all { it in '0'..'9' || it in 'A'..'F' }) apply((0xFF000000L or hex.toLong(16)).toInt())
            },singleLine=true,isError=hex.length!=6 || hex.any {it !in '0'..'9' && it !in 'A'..'F'},label={Text(I18n.t("appearance.hex"))},prefix={Text("#")},
                trailingIcon={Box(Modifier.size(28.dp).background(Color(picked),RoundedCornerShape(6.dp)).border(1.dp,MaterialTheme.colorScheme.outline,RoundedCornerShape(6.dp)))},
                modifier=Modifier.fillMaxWidth())
            AppearanceColorSwatches(presets,picked,::apply)
        } },
        confirmButton={TextButton(enabled=hex.length==6 && hex.all {it in '0'..'9' || it in 'A'..'F'},onClick={onChoose(picked)}) {Text(I18n.t("common.save"))}},
        dismissButton={TextButton(onClick=onDismiss) {Text(I18n.t("common.cancel"))}})
}
