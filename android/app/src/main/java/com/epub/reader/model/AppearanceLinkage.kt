package com.zhongbai233.epub.reader.model

import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

@Serializable
enum class AutomaticAppearanceSource { system, ambient;
    fun resolve(systemDark: Boolean, ambientDark: Boolean?) = if(this == ambient) ambientDark ?: systemDark else systemDark
}

@Serializable
data class AppearanceLinkage(
    val version: Int = 1,
    val source: AutomaticAppearanceSource = AutomaticAppearanceSource.system,
    val appLinked: Boolean = true,
    val readingLinked: Boolean = true,
    val appManual: Boolean = false,
    val readingMode: ReadingDisplayMode = ReadingDisplayMode.automatic,
    val readingSource: AutomaticAppearanceSource = AutomaticAppearanceSource.system,
) {
    fun appDark(system: Boolean, ambient: Boolean?) = if(appLinked) source.resolve(system, ambient) else appManual
    fun readingDark(system: Boolean, ambient: Boolean?) = if(readingLinked) appDark(system,ambient) else readingMode.isDark(readingSource.resolve(system,ambient))
    val usesAmbient: Boolean get() = (appLinked && source == AutomaticAppearanceSource.ambient) ||
        (!readingLinked && readingMode == ReadingDisplayMode.automatic && readingSource == AutomaticAppearanceSource.ambient)
    fun toggleAppLink(system: Boolean, ambient: Boolean?) = copy(appLinked = !appLinked, appManual = if(appLinked) appDark(system,ambient) else appManual)
    fun toggleReadingLink(system: Boolean, ambient: Boolean?) = copy(readingLinked = !readingLinked,
        readingMode = if(readingLinked) {if(readingDark(system,ambient)) ReadingDisplayMode.dark else ReadingDisplayMode.light} else readingMode)
    fun setApp(dark: Boolean) = copy(appLinked = false, appManual = dark)
    fun toggleReading(system: Boolean, ambient: Boolean?): AppearanceLinkage {
        val dark = !readingDark(system,ambient)
        return if(readingLinked) setApp(dark) else copy(readingMode = if(dark) ReadingDisplayMode.dark else ReadingDisplayMode.light)
    }
    fun encoded() = codec.encodeToString(this)
    companion object {
        fun firstRun() = AppearanceLinkage(source=AutomaticAppearanceSource.ambient, readingSource=AutomaticAppearanceSource.ambient)
        const val storageKey = "appearance.linkage.v1"
        private val codec = Json { ignoreUnknownKeys = true; encodeDefaults = true }
        fun decode(raw: String?): AppearanceLinkage? = raw?.let {runCatching {codec.decodeFromString<AppearanceLinkage>(it).takeIf {v->v.version==1}}.getOrNull()}
        fun migrate(app: Boolean, reading: ReadingDisplayMode): AppearanceLinkage {
            val dark = reading == ReadingDisplayMode.dark
            return AppearanceLinkage(appLinked=false, appManual=app, readingLinked=reading != ReadingDisplayMode.automatic && app==dark,
                readingMode=reading, readingSource=AutomaticAppearanceSource.ambient)
        }
    }
}
