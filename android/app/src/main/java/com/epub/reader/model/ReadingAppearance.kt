package com.zhongbai233.epub.reader.model

import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

@Serializable
enum class ReadingDisplayMode { light, dark, automatic;
    fun isDark(automaticDark: Boolean) = this == dark || (this == automatic && automaticDark)
}

@Serializable
data class ReadingAppearanceVariant(
    val paper: Int, val ink: Int, val image: String? = null, val strength: Float = .18f,
)

@Serializable
data class ReadingStyle(val id: String, val light: ReadingAppearanceVariant, val dark: ReadingAppearanceVariant) {
    fun variant(isDark: Boolean) = if (isDark) dark else light
    fun replacing(isDark: Boolean, value: ReadingAppearanceVariant) = if (isDark) copy(dark = value) else copy(light = value)
    companion object {
        val customTemplate = ReadingStyle("custom",
            ReadingAppearanceVariant(0xFFFFFFFF.toInt(), 0xFF1A1A1A.toInt()),
            ReadingAppearanceVariant(0xFF1A1A1A.toInt(), 0xFFE8E8E8.toInt(), strength = .12f))
        val presets = listOf(
            ReadingStyle("paper", ReadingAppearanceVariant(0xFFF5F0E8.toInt(), 0xFF1A1A1A.toInt()), ReadingAppearanceVariant(0xFF1A1A1A.toInt(), 0xFFE8E8E8.toInt(), strength = .12f)),
            ReadingStyle("gray", ReadingAppearanceVariant(0xFFF1F3F5.toInt(), 0xFF1A1A1A.toInt()), ReadingAppearanceVariant(0xFF24262B.toInt(), 0xFFE6E6E6.toInt(), strength = .12f)),
            ReadingStyle("green", ReadingAppearanceVariant(0xFFE8F0E8.toInt(), 0xFF1A1A1A.toInt()), ReadingAppearanceVariant(0xFF1D2823.toInt(), 0xFFDCE6DE.toInt(), strength = .12f)),
            ReadingStyle("light", ReadingAppearanceVariant(0xFFFFFFFF.toInt(), 0xFF1A1A1A.toInt()), ReadingAppearanceVariant(0xFF171719.toInt(), 0xFFDCDCDC.toInt(), strength = .12f)),
            ReadingStyle("warm", ReadingAppearanceVariant(0xFFEEDDC4.toInt(), 0xFF1A1A1A.toInt()), ReadingAppearanceVariant(0xFF29241E.toInt(), 0xFFEADFCC.toInt(), strength = .12f)),
        )
    }
}

@Serializable
data class ReadingAppearance(
    val mode: ReadingDisplayMode = ReadingDisplayMode.light,
    val selected: String = "paper",
    val custom: ReadingStyle? = null,
    val overrides: Map<String, ReadingStyle> = emptyMap(),
) {
    val choices: List<ReadingStyle> get() = ReadingStyle.presets.map { overrides[it.id] ?: it } + (custom ?: ReadingStyle.customTemplate)
    val style: ReadingStyle get() = choices.find { it.id == selected } ?: choices.first()
    val imageReferences: Set<String> get() = (overrides.values + listOfNotNull(custom))
        .flatMap { listOfNotNull(it.light.image, it.dark.image) }.filter { it.isNotBlank() }.toSet()
    fun saved(draft: ReadingStyle): ReadingAppearance = if (ReadingStyle.presets.any { it.id == draft.id })
        copy(overrides = overrides + (draft.id to draft), selected = draft.id).normalized()
    else copy(custom = draft.copy(id = "custom"), selected = "custom").normalized()
    fun encoded(): String = codec.encodeToString(this)
    fun normalized(): ReadingAppearance {
        fun normalize(style: ReadingStyle, id: String) = style.copy(id = id,
            light = style.light.copy(strength = style.light.strength.takeIf { it.isFinite() }?.coerceIn(0f, 1f) ?: .18f),
            dark = style.dark.copy(strength = style.dark.strength.takeIf { it.isFinite() }?.coerceIn(0f, 1f) ?: .12f))
        val known = ReadingStyle.presets.map { it.id }.toSet()
        return copy(
            selected = selected.takeIf { it in known || (it == "custom" && custom != null) } ?: "paper",
            custom = custom?.let { normalize(it, "custom") },
            overrides = overrides.filterKeys { it in known }.mapValues { (id, style) -> normalize(style, id) })
    }
    companion object {
        const val storageKey = "reader.appearance.v1"
        private val codec = Json { ignoreUnknownKeys = true; encodeDefaults = true }
        fun decode(raw: String?): ReadingAppearance? = raw?.let { runCatching { codec.decodeFromString<ReadingAppearance>(it).normalized() }.getOrNull() }
        fun migrate(bgIndex: Int, customBg: Int, inkIndex: Int, customInk: Int, image: String?, strength: Float): ReadingAppearance {
            val papers = listOf(0xFFF5F0E8.toInt(), 0xFFF1F3F5.toInt(), 0xFFE8F0E8.toInt(), 0xFF1A1A1A.toInt(), 0xFF24262B.toInt())
            val paper = papers.getOrNull(bgIndex) ?: customBg
            val dark = ((paper shr 16 and 255) * .299 + (paper shr 8 and 255) * .587 + (paper and 255) * .114) < 255 * .45
            val inks = listOf(if(dark) 0xFFE8E8E8.toInt() else 0xFF1A1A1A.toInt(), 0xFF1A1A1A.toInt(), 0xFF2D2D2D.toInt(), 0xFFE6E6E6.toInt(), 0xFFF1EAD8.toInt())
            val selected = when(bgIndex) { 1,4 -> "gray"; 2 -> "green"; else -> "paper" }
            val value = ReadingAppearance(if(dark) ReadingDisplayMode.dark else ReadingDisplayMode.light, selected)
            if (bgIndex != 5 && inkIndex == 0 && image.isNullOrBlank()) return value
            var style = value.style.replacing(dark, ReadingAppearanceVariant(paper, inks.getOrNull(inkIndex) ?: customInk))
            style = style.copy(light = style.light.copy(image = image, strength = strength), dark = style.dark.copy(image = image, strength = strength))
            return value.saved(style.copy(id = "custom")).normalized()
        }
    }
}

// Separate enter/exit thresholds and a dwell time prevent flicker near a boundary.
class AmbientReadingSwitch(initialDark: Boolean) {
    var dark = initialDark; private set
    private var candidate: Boolean? = null
    private var since = 0L
    fun sample(lux: Float, nowMillis: Long): Boolean {
        if (!lux.isFinite() || lux < 0f) return dark
        val target = when { lux < 15f -> true; lux > 40f -> false; else -> dark }
        if (target == dark) { candidate = null; return dark }
        if (candidate != target) { candidate = target; since = nowMillis }
        if (nowMillis - since >= 3000) { dark = target; candidate = null }
        return dark
    }
}
