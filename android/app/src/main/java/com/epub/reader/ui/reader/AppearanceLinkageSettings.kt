package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.BorderStroke
import androidx.compose.material.icons.filled.SettingsBrightness
import androidx.compose.material.icons.filled.WebAsset
import androidx.compose.material.icons.filled.MenuBook
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Link
import androidx.compose.material.icons.filled.LinkOff
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.model.*

@Composable
internal fun AppearanceLinkageSettings(value: AppearanceLinkage, systemDark: Boolean, ambientDark: Boolean?, sensorAvailable: Boolean, onChange: (AppearanceLinkage) -> Unit, showTitle: Boolean = true) {
    var selected by remember { mutableIntStateOf(0) }
    fun t(key: String) = I18n.t("appearance.link_$key")
    @Composable fun node(index: Int, title: String, state: String, width: androidx.compose.ui.unit.Dp) {
        Surface(onClick={selected=index},modifier=Modifier.width(width).heightIn(min=112.dp)
            .semantics { this.selected=selected==index },shape=RoundedCornerShape(14.dp),
            color=if(selected==index) MaterialTheme.colorScheme.secondaryContainer else MaterialTheme.colorScheme.surfaceVariant.copy(alpha=0.35f),
            border=BorderStroke(1.dp,if(selected==index) MaterialTheme.colorScheme.primary.copy(alpha=0.30f) else MaterialTheme.colorScheme.outlineVariant.copy(alpha=0.65f))) {
            Column(Modifier.padding(horizontal=4.dp,vertical=12.dp),horizontalAlignment=Alignment.CenterHorizontally,verticalArrangement=Arrangement.spacedBy(7.dp)) {
                Icon(when(index) {0->Icons.Default.SettingsBrightness;1->Icons.Default.WebAsset;else->Icons.Default.MenuBook},null,Modifier.size(22.dp),tint=if(selected==index) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface)
                Text(t(title),style=MaterialTheme.typography.labelMedium,textAlign=androidx.compose.ui.text.style.TextAlign.Center)
                Text(state,style=MaterialTheme.typography.labelSmall,color=MaterialTheme.colorScheme.onSurfaceVariant,textAlign=androidx.compose.ui.text.style.TextAlign.Center)
            }
        }
    }
    @Composable fun connection(app: Boolean) {
        val linked=if(app) value.appLinked else value.readingLinked
        TextButton(onClick={
            onChange(if(app) value.toggleAppLink(systemDark,ambientDark) else value.toggleReadingLink(systemDark,ambientDark));selected=if(app) 1 else 2
        },modifier=Modifier.width(48.dp).heightIn(min=112.dp).semantics {
            contentDescription=t((if(linked) "unlink_" else "relink_")+(if(app) "app" else "reading"))
            stateDescription=t(if(app) {if(linked) "app_linked" else "app_independent"} else {if(linked) "reading_linked" else "reading_independent"})
        },contentPadding=PaddingValues(horizontal=0.dp,vertical=8.dp)) {
            Column(horizontalAlignment=Alignment.CenterHorizontally,verticalArrangement=Arrangement.spacedBy(7.dp)) {
                Icon(if(linked) Icons.Default.Link else Icons.Default.LinkOff,null,Modifier.size(20.dp))
                val action=t(if(linked) "unlink" else "relink")
                val label=if(action.length==4 && action.all {it.code in 0x4E00..0x9FFF}) action.chunked(2).joinToString("\n") else action
                Text(label,style=MaterialTheme.typography.labelSmall,textAlign=androidx.compose.ui.text.style.TextAlign.Center)
            }
        }
    }
    @Composable fun sourceChoices(source: AutomaticAppearanceSource, reading: Boolean, choose: (AutomaticAppearanceSource)->Unit) {
        Text(t(if(reading) "reading_basis" else "basis"),style=MaterialTheme.typography.labelLarge)
        Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.spacedBy(8.dp)) {
            AutomaticAppearanceSource.entries.forEach { candidate ->
                FilterChip(selected=source==candidate,onClick={choose(candidate)},label={Text(t(candidate.name))},modifier=Modifier.weight(1f).heightIn(min=44.dp))
            }
        }
        if(source==AutomaticAppearanceSource.ambient && (reading || value.appLinked)) Text(t(if(!sensorAvailable) "unavailable" else if(ambientDark==null) "waiting" else "ambient_hint"),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
    }
    Column(verticalArrangement=Arrangement.spacedBy(4.dp)) {
        if(showTitle) Text(t("title"),style=MaterialTheme.typography.titleMedium)
        val fontScale=LocalDensity.current.fontScale
        BoxWithConstraints(Modifier.fillMaxWidth()) {
            val minimumNodeWidth=64.dp*fontScale
            val width=maxOf(maxWidth,minimumNodeWidth*3+112.dp)
            val nodeWidth=(width-112.dp)/3
            Row(Modifier.horizontalScroll(rememberScrollState()).width(width),horizontalArrangement=Arrangement.spacedBy(4.dp),verticalAlignment=Alignment.CenterVertically) {
                node(0,"source",t(value.source.name),nodeWidth)
                connection(true)
                node(1,"app",I18n.t(if(value.appDark(systemDark,ambientDark)) "appearance.mode_dark" else "appearance.mode_light"),nodeWidth)
                connection(false)
                node(2,"reading",I18n.t(if(value.readingDark(systemDark,ambientDark)) "appearance.mode_dark" else "appearance.mode_light"),nodeWidth)
            }
        }
        Spacer(Modifier.height(12.dp))
        Text(t(when(selected) {0->"source";1->"app";else->"reading"}),style=MaterialTheme.typography.titleSmall)
        key(selected,value.readingLinked,value.readingMode) { when(selected) {
            0 -> sourceChoices(value.source,false) {onChange(value.copy(source=it))}
            1 -> if(value.appLinked) Text(t("follow_source")+" · "+t(value.source.name),style=MaterialTheme.typography.bodyMedium)
            else Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.spacedBy(8.dp)) {
                listOf(false,true).forEach {dark->FilterChip(selected=value.appManual==dark,onClick={onChange(value.setApp(dark))},label={Text(I18n.t(if(dark) "appearance.mode_dark" else "appearance.mode_light"))},modifier=Modifier.weight(1f).heightIn(min=44.dp))}
            }
            else -> if(value.readingLinked) Text(t("follow_app"),style=MaterialTheme.typography.bodyMedium)
            else {
                Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.spacedBy(8.dp)) {
                    ReadingDisplayMode.entries.forEach {mode->FilterChip(selected=value.readingMode==mode,onClick={onChange(value.copy(readingMode=mode))},label={Text(I18n.t("appearance.mode_${mode.name}"))},modifier=Modifier.weight(1f).heightIn(min=44.dp))}
                }
                if(value.readingMode==ReadingDisplayMode.automatic) sourceChoices(value.readingSource,true) {onChange(value.copy(readingSource=it))}
            }
        } }
    }
}
