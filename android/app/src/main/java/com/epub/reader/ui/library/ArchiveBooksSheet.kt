package com.zhongbai233.epub.reader.ui.library

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.i18n.I18n
import org.json.JSONObject
import kotlinx.coroutines.launch

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ArchiveBooksSheet(books: List<JSONObject>, action:(String,String)->Unit, dismiss:()->Unit) {
    var deleting by remember { mutableStateOf<JSONObject?>(null) }
    val sheet=rememberModalBottomSheetState(skipPartiallyExpanded=true)
    val scope=rememberCoroutineScope()
    ModalBottomSheet(onDismissRequest=dismiss,sheetState=sheet) {
        Column(Modifier.fillMaxWidth().padding(horizontal=24.dp)) {
            Text(I18n.t("archive.title"),style=MaterialTheme.typography.titleLarge)
            Spacer(Modifier.height(12.dp))
            if (books.isEmpty()) Text(I18n.t("archive.empty"),color=MaterialTheme.colorScheme.onSurfaceVariant)
            LazyColumn(Modifier.weight(1f,false)) {
                items(books,key={it.getJSONObject("book").getString("hash")}) { entry ->
                    val book=entry.getJSONObject("book")
                    Column(Modifier.fillMaxWidth().padding(vertical=12.dp)) {
                        Text(book.getString("title"),style=MaterialTheme.typography.titleMedium)
                        Text(I18n.t(if(entry.optBoolean("local_available")) "archive.local" else "archive.cloud"),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                        Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.SpaceBetween) {
                            TextButton(onClick={action(book.getString("hash"),"restore")}) { Text(I18n.t("archive.restore")) }
                            TextButton(onClick={deleting=entry}) { Text(I18n.t("archive.delete"),color=MaterialTheme.colorScheme.error) }
                        }
                    }
                    HorizontalDivider()
                }
            }
            TextButton(onClick={scope.launch {sheet.hide();dismiss()}},modifier=Modifier.fillMaxWidth()) { Text(I18n.t("manage.done")) }
            Spacer(Modifier.height(12.dp))
        }
    }
    deleting?.let { entry -> AlertDialog(onDismissRequest={deleting=null},title={Text(I18n.t("archive.delete_confirm"))},text={Text(entry.getJSONObject("book").getString("title")+"\n\n"+I18n.t("archive.delete_note"))},confirmButton={TextButton(onClick={action(entry.getJSONObject("book").getString("hash"),"delete");deleting=null}) {Text(I18n.t("archive.delete"),color=MaterialTheme.colorScheme.error)}},dismissButton={TextButton(onClick={deleting=null}) {Text(I18n.t("settings.cancel"))}}) }
}

@Composable
fun SyncMeterContent(meter:JSONObject?,compact:Boolean=false) {
    if(meter==null || meter.optString("phase").isBlank()) return
    val label=I18n.t(when(meter.optString("phase")) {
        "progress"->"cloud.phase_progress"
        "uploading"->"cloud.phase_uploading"
        "downloading"->"cloud.phase_downloading"
        "deleting"->"cloud.phase_deleting"
        "saving"->"cloud.phase_saving"
        else->"cloud.phase_preparing"
    })
    val completed=meter.optInt("completed"); val total=meter.optInt("total")
    Column(Modifier.widthIn(max=if(compact) 160.dp else 600.dp),verticalArrangement=Arrangement.spacedBy(4.dp)) {
        Text(label,style=MaterialTheme.typography.labelSmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
        if(total>0) {
            LinearProgressIndicator(progress={completed.toFloat()/total},modifier=Modifier.fillMaxWidth())
            Text(I18n.tf("cloud.processed",completed.toString(),total.toString()),style=MaterialTheme.typography.labelSmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
        }
        if(!compact && meter.optString("title").isNotBlank()) Text(meter.optString("title"),style=MaterialTheme.typography.bodySmall,maxLines=2)
    }
}
