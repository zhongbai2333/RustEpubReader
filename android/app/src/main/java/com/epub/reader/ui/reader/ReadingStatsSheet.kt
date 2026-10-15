package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.zhongbai233.epub.reader.i18n.I18n
import kotlinx.serialization.Serializable
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.ensureActive

@Serializable data class ActivityDay(val date:String, val reading:Long, val listening:Long)
@Serializable data class ActivityBook(val hash:String, val title:String, val reading:Long, val listening:Long, val undated:Long)
@Serializable data class ActivityReport(val today:Long, val week:Long, val total:Long, val reading:Long, val listening:Long,
    val undated:Long, val active_days:Int, val streak:Int, val days:List<ActivityDay>, val books:List<ActivityBook>, val devices:Int)
private fun duration(seconds:Long):String {
    val hour=I18n.t("stats.unit_hours");val minute=I18n.t("stats.unit_minutes");val second=I18n.t("stats.unit_seconds")
    return when {
    seconds>=3600 -> "${seconds/3600}$hour ${seconds%3600/60}$minute"
    seconds>=60 -> "${seconds/60}$minute"
    else -> "$seconds$second"
}
}
@OptIn(ExperimentalMaterial3Api::class)
@Composable fun ReadingStatsSheet(currentBookAvailable:Boolean, initialCurrent:Boolean, load:suspend (Boolean)->ActivityReport,
    onDismiss:()->Unit, onSyncSettings:()->Unit) {
    var current by remember {mutableStateOf(initialCurrent && currentBookAvailable)}
    var range by remember {mutableIntStateOf(7)}
    var report by remember {mutableStateOf<ActivityReport?>(null)}
    var failure by remember {mutableStateOf<String?>(null)}
    var selected by remember {mutableStateOf<ActivityDay?>(null)}
    LaunchedEffect(current) {
        report=null;selected=null;failure=null
        while(isActive) {
            try {val value=load(current);ensureActive();report=value;failure=null}
            catch(e:kotlinx.coroutines.CancellationException) {throw e}
            catch(e:Exception) {failure=e.message}
            delay(15000)
        }
    }
    val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val closeSheet = rememberReaderSheetDismiss(sheet, onDismiss)
    ModalBottomSheet(onDismissRequest=onDismiss, sheetState=sheet) {
        Column(Modifier.fillMaxWidth().fillMaxHeight(.92f).padding(horizontal=20.dp)) {
            Row(Modifier.fillMaxWidth(),verticalAlignment=Alignment.CenterVertically) {
                Text(I18n.t("stats.title"),style=MaterialTheme.typography.titleLarge,modifier=Modifier.weight(1f))
                IconButton(onClick=closeSheet) {Icon(Icons.Default.Close,I18n.t("settings.close"))}
            }
            Row(horizontalArrangement=Arrangement.spacedBy(8.dp)) {
                FilterChip(selected=!current,onClick={current=false},label={Text(I18n.t("stats.all_books"))})
                if(currentBookAvailable) FilterChip(selected=current,onClick={current=true},label={Text(I18n.t("stats.current_book"))})
            }
            failure?.let {Text(I18n.t("common.error_activity_load"),color=MaterialTheme.colorScheme.error)}
            val value=report
            if(value==null && failure==null) {CircularProgressIndicator(Modifier.padding(20.dp))}
            if(value!=null) LazyColumn(Modifier.weight(1f),verticalArrangement=Arrangement.spacedBy(14.dp),contentPadding=PaddingValues(bottom=24.dp)) {
                item {
                    Text(I18n.t("stats.today"),color=MaterialTheme.colorScheme.onSurfaceVariant)
                    Text(duration(value.today),fontSize=34.sp,fontWeight=FontWeight.Bold)
                    Text("${I18n.t("stats.week")} ${duration(value.week)} · ${I18n.t("stats.total")} ${duration(value.total)}",style=MaterialTheme.typography.bodyMedium)
                }
                if(value.total==0L) item {Text(I18n.t("stats.empty"),fontWeight=FontWeight.SemiBold);Text(I18n.t("stats.empty_hint"))}
                else {
                    item {
                        Row(verticalAlignment=Alignment.CenterVertically) {
                            Text(I18n.t("stats.trend"),modifier=Modifier.weight(1f),fontWeight=FontWeight.SemiBold)
                            FilterChip(selected=range==7,onClick={range=7;selected=null},label={Text(I18n.t("stats.seven_days"))})
                            Spacer(Modifier.width(6.dp))
                            FilterChip(selected=range==30,onClick={range=30;selected=null},label={Text(I18n.t("stats.thirty_days"))})
                        }
                        val days=value.days.takeLast(range)
                        val peak=((days.maxOfOrNull {it.reading+it.listening} ?: 0).coerceAtLeast(60)+59)/60*60
                        Row(Modifier.fillMaxWidth()) {
                            Column(Modifier.width(58.dp).height(140.dp).padding(top=20.dp),verticalArrangement=Arrangement.SpaceBetween) {
                                Text(duration(peak),style=MaterialTheme.typography.labelSmall)
                                Text(if(peak%120==60L && peak>60) "${duration(peak/2/60*60)} ${duration(30)}" else duration(peak/2),style=MaterialTheme.typography.labelSmall)
                                Text("0",style=MaterialTheme.typography.labelSmall)
                            }
                            Column(Modifier.weight(1f)) {
                        Row(Modifier.fillMaxWidth().height(140.dp),horizontalArrangement=Arrangement.spacedBy(if(range==7) 8.dp else 2.dp),verticalAlignment=Alignment.Bottom) {
                            days.forEach {day -> Column(Modifier.weight(1f).fillMaxHeight().semantics {contentDescription="${day.date} · ${I18n.t("stats.reading")} ${duration(day.reading)} · ${I18n.t("stats.listening")} ${duration(day.listening)}"}.clickable {selected=day},verticalArrangement=Arrangement.Bottom) {
                                if(day.listening>0) Box(Modifier.fillMaxWidth().height((120f*day.listening/peak).dp).background(Color(0xFFCC8E42),RoundedCornerShape(3.dp)))
                                Box(Modifier.fillMaxWidth().height((120f*day.reading/peak).coerceAtLeast(1f).dp).background(MaterialTheme.colorScheme.primary.copy(alpha=if(day.reading>0) 1f else .18f),RoundedCornerShape(3.dp)))
                            } }
                        }
                        Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.SpaceBetween) {
                            if(range==7) days.forEach {day -> Text(day.date.takeLast(5),modifier=Modifier.weight(1f),fontSize=9.sp,textAlign=androidx.compose.ui.text.style.TextAlign.Center)}
                            else {
                                Text(days.first().date.takeLast(5),style=MaterialTheme.typography.labelSmall)
                                Text(days[days.size/2].date.takeLast(5),style=MaterialTheme.typography.labelSmall)
                                Text(days.last().date.takeLast(5),style=MaterialTheme.typography.labelSmall)
                            }
                        }
                            }
                        }
                        selected?.let {Text("${it.date} · ${I18n.t("stats.reading")} ${duration(it.reading)} · ${I18n.t("stats.listening")} ${duration(it.listening)}")}
                        Row(horizontalArrangement=Arrangement.spacedBy(12.dp)) {
                            Text("● ${I18n.t("stats.reading")} ${duration(value.reading)}",color=MaterialTheme.colorScheme.primary)
                            Text("● ${I18n.t("stats.listening")} ${duration(value.listening)}",color=Color(0xFFCC8E42))
                        }
                        Text("${I18n.t("stats.active_days")} ${value.active_days} · ${I18n.t("stats.streak")} ${value.streak}",style=MaterialTheme.typography.bodySmall)
                    }
                    item {Text(I18n.t("stats.books"),fontWeight=FontWeight.SemiBold)}
                    items(value.books,key={it.hash}) {book ->
                        Row(Modifier.fillMaxWidth(),verticalAlignment=Alignment.CenterVertically) {Text(book.title,modifier=Modifier.weight(1f));Spacer(Modifier.width(12.dp));Text(duration(book.reading+book.listening+book.undated))}
                        HorizontalDivider(Modifier.padding(top=10.dp))
                    }
                }
                item {var expanded by remember {mutableStateOf(false)}
                    TextButton(onClick={expanded=!expanded}) {Text(I18n.t("stats.rules"))}
                    if(expanded) {Text(I18n.t("stats.timer_hint"));Text(I18n.t("stats.sync_hint"));Text(I18n.t("stats.date_hint"))}
                    TextButton(onClick=onSyncSettings) {Text(I18n.t("stats.sync_settings"))}
                }
            }
        }
    }
}
