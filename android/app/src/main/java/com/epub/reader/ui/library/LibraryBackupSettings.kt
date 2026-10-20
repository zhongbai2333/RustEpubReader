package com.zhongbai233.epub.reader.ui.library

import android.provider.OpenableColumns
import androidx.compose.ui.text.style.TextOverflow
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.clickable
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Save
import androidx.compose.material.icons.filled.FolderOpen
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.ui.Alignment
import androidx.compose.ui.semantics.Role
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.viewmodel.ReaderViewModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.File
import java.util.UUID

/** Native file providers stream packages; book bytes never travel through JNI JSON. */
@Composable
@OptIn(ExperimentalLayoutApi::class)
internal fun LibraryBackupSettings(vm: ReaderViewModel, modifier: Modifier = Modifier, onDone: () -> Unit = {}) {
    val context=LocalContext.current
    @Suppress("UNUSED_VARIABLE") val languageVersion=I18n.version
    val scope=rememberCoroutineScope()
    var selected by rememberSaveable { mutableStateOf<String?>(null) }
    var selectedName by rememberSaveable { mutableStateOf<String?>(null) }
    var exported by rememberSaveable { mutableStateOf<String?>(null) }
    var summary by rememberSaveable { mutableStateOf<String?>(null) }
    var message by rememberSaveable { mutableStateOf<String?>(null) }
    var operation by rememberSaveable { mutableStateOf("inspect") }
    var detailsExpanded by rememberSaveable { mutableStateOf(false) }
    var contentsExpanded by rememberSaveable { mutableStateOf(false) }
    var failure by rememberSaveable { mutableStateOf<String?>(null) }
    var preparing by remember {mutableStateOf(false)}
    var progress by remember {mutableStateOf<JSONObject?>(null)}
    val busy=vm.backupBusy || preparing
    val available=vm.backupAvailable && !preparing
    LaunchedEffect(vm.backupBusy) {
        while(vm.backupBusy) {progress=vm.backupProgress();delay(200)}
        progress=null
    }
    val save=rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/zip")) {uri ->
        val path=exported
        if(uri!=null && path!=null) scope.launch {
            preparing=true;failure=null;operation="save"
            try {
                vm.backupFileOperation { withContext(Dispatchers.IO) {
                    context.contentResolver.openOutputStream(uri,"wt")?.use {output -> File(path).inputStream().use {it.copyTo(output)}}
                        ?: error("Cannot open selected backup destination")
                } }
                message="backup.saved"
            } catch(error:Exception) {failure=error.message}
            finally {preparing=false}
        }
    }
    val choose=rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) {uri ->
        if(uri!=null) scope.launch {
            preparing=true;failure=null;message=null;summary=null;selected=null;selectedName=null;operation="inspect";detailsExpanded=false
            val folder=File(context.cacheDir,"library-backups/${UUID.randomUUID()}")
            val path=File(folder,"selected.readerbackup")
            try {
                var displayName: String? = null
                vm.backupFileOperation { withContext(Dispatchers.IO) {
                    displayName = context.contentResolver.query(uri,arrayOf(OpenableColumns.DISPLAY_NAME),null,null,null)?.use { cursor ->
                        if(cursor.moveToFirst()) cursor.getString(0)?.take(512) else null
                    }
                    check(folder.mkdirs()) {"Cannot prepare backup file"}
                    context.contentResolver.openInputStream(uri)?.use {input ->
                        path.outputStream().use {output ->
                            val buffer=ByteArray(64*1024);var total=0L
                            while(true) {val count=input.read(buffer);if(count<0) break;total+=count
                                check(total<=65L*1024*1024*1024) {"Backup exceeds package size limit"}
                                output.write(buffer,0,count)
                            }
                        }
                    } ?: error("Cannot read selected backup")
                } }
                preparing=false
                val result=vm.backupOperation("inspect",path.absolutePath)
                selected=path.absolutePath;selectedName=displayName;summary=result.toString()
            } catch(error:Exception) {failure=error.message;withContext(Dispatchers.IO) {folder.deleteRecursively()}}
            finally {preparing=false}
        }
    }
    fun create() {
        if(!available) return
        scope.launch {
            preparing=true;failure=null;message=null;summary=null;selected=null;operation="export";detailsExpanded=false
            val folder=File(context.cacheDir,"library-backups/${UUID.randomUUID()}")
            try {
                withContext(Dispatchers.IO) {check(folder.mkdirs()) {"Cannot prepare backup file"}}
                val path=File(folder,"RustEpubReader.readerbackup")
                preparing=false
                vm.backupOperation("export",path.absolutePath)
                exported=path.absolutePath;message="backup.created"
                save.launch(path.name)
            } catch(error:Exception) {failure=error.message}
            finally {preparing=false}
        }
    }
    fun home() {summary=null;selected=null;selectedName=null;message=null;failure=null;detailsExpanded=false}
    fun merge() {
        val path=selected ?: return
        val result=summary?.let(::JSONObject) ?: return
        scope.launch {
            failure=null;message=null;operation="restore";detailsExpanded=false
            try {vm.backupOperation("restore",path,result.getString("manifest_sha256"));summary=null;selected=null;selectedName=null;message="backup.done"}
            catch(error:Exception) {failure=error.message}
        }
    }
    val confirming=summary!=null && failure==null && !busy
    val finished=message!=null && failure==null && !busy
    val scroll=rememberScrollState()
    LaunchedEffect(confirming,finished,failure) {scroll.scrollTo(0)}
    Column(modifier.fillMaxWidth().padding(horizontal=20.dp,vertical=16.dp)) {
        Column(Modifier.weight(1f).fillMaxWidth().verticalScroll(scroll),verticalArrangement=Arrangement.spacedBy(16.dp)) {
            when {
                busy -> Column(Modifier.fillMaxWidth().semantics {liveRegion=LiveRegionMode.Polite},verticalArrangement=Arrangement.spacedBy(12.dp)) {
                    val phase=progress?.optString("phase") ?: "preparing"
                    val key=when(phase) {"collecting"->"backup.collecting";"packing"->"backup.packing";"verifying","inspect"->"backup.verifying";"merging","restore"->"backup.merging";"publishing"->"backup.publishing";else->"backup.preparing"}
                    Text(I18n.t(key),style=MaterialTheme.typography.titleMedium)
                    val total=progress?.optInt("total") ?: 0
                    val done=progress?.optInt("completed") ?: 0
                    if(total>0) {
                        LinearProgressIndicator(progress={(done.toFloat()/total).coerceIn(0f,1f)},modifier=Modifier.fillMaxWidth())
                        Text(I18n.tf2("cloud.processed",done.toString(),total.toString()),style=MaterialTheme.typography.bodyMedium)
                    } else LinearProgressIndicator(Modifier.fillMaxWidth())
                }
                failure!=null -> {
                    val title=when(operation) {"export","save"->"backup.error_create";"restore"->"backup.error_restore";else->"backup.error_inspect"}
                    Text(I18n.t(title),style=MaterialTheme.typography.titleMedium,color=MaterialTheme.colorScheme.error)
                    Text(I18n.t(backupFailureHint(failure!!)),style=MaterialTheme.typography.bodyMedium)
                    TextButton(onClick={detailsExpanded=!detailsExpanded}) {Text(I18n.t("backup.details"))}
                    if(detailsExpanded) SelectionContainer {Text(failure!!,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)}
                }
                confirming -> {
                    val result=JSONObject(summary!!)
                    Text(I18n.t("backup.confirm_restore"),style=MaterialTheme.typography.titleMedium)
                    Surface(shape=RoundedCornerShape(16.dp),color=MaterialTheme.colorScheme.surfaceContainerLow) {
                        Column(Modifier.fillMaxWidth().padding(16.dp),verticalArrangement=Arrangement.spacedBy(10.dp)) {
                            SelectionContainer {Text(selectedName ?: File(selected!!).name,maxLines=2,overflow=TextOverflow.Ellipsis,style=MaterialTheme.typography.titleSmall)}
                            Row(horizontalArrangement=Arrangement.spacedBy(8.dp),verticalAlignment=Alignment.CenterVertically) {
                                Icon(Icons.Default.CheckCircle,null,tint=MaterialTheme.colorScheme.primary,modifier=Modifier.size(20.dp))
                                Text(I18n.t("backup.verified"),style=MaterialTheme.typography.labelLarge)
                            }
                            FlowRow(horizontalArrangement=Arrangement.spacedBy(16.dp),verticalArrangement=Arrangement.spacedBy(6.dp)) {
                                Text(I18n.tf1("backup.books",result.optInt("books").toString()))
                                Text(I18n.tf1("backup.archived",result.optInt("archived").toString()))
                                Text(android.text.format.Formatter.formatFileSize(context,result.optLong("file_bytes")))
                            }
                        }
                    }
                    Text(I18n.t("backup.rules"),style=MaterialTheme.typography.bodyMedium,color=MaterialTheme.colorScheme.onSurfaceVariant)
                    TextButton(onClick={choose.launch(arrayOf("*/*"))},enabled=available) {Text(I18n.t("backup.change_file"))}
                }
                finished -> {
                    Column(Modifier.fillMaxWidth().heightIn(min=280.dp),horizontalAlignment=Alignment.CenterHorizontally,verticalArrangement=Arrangement.spacedBy(12.dp,Alignment.CenterVertically)) {
                        Icon(Icons.Default.CheckCircle,null,tint=MaterialTheme.colorScheme.primary,modifier=Modifier.size(48.dp))
                        Text(I18n.t(message!!),Modifier.semantics {liveRegion=LiveRegionMode.Polite},style=MaterialTheme.typography.titleMedium)
                        if(message!="backup.done") {
                            exported?.let {SelectionContainer {Text(File(it).name,style=MaterialTheme.typography.bodyMedium)}}
                        }
                    }
                }
                else -> {
                    Surface(shape=RoundedCornerShape(16.dp),color=MaterialTheme.colorScheme.surfaceContainerLow) {
                        Column {
                            BackupActionRow("backup.create","backup.create_hint",Icons.Default.Save,available,::create)
                            HorizontalDivider(Modifier.padding(start=56.dp),color=MaterialTheme.colorScheme.outlineVariant)
                            BackupActionRow("backup.restore_title","backup.restore_hint",Icons.Default.FolderOpen,available) {choose.launch(arrayOf("*/*"))}
                        }
                    }
                    Text(I18n.t("backup.privacy_short"),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                    TextButton(onClick={contentsExpanded=!contentsExpanded}) {Text(I18n.t("backup.contents"))}
                    if(contentsExpanded) Column(verticalArrangement=Arrangement.spacedBy(8.dp)) {
                        Text(I18n.t("backup.scope"),style=MaterialTheme.typography.bodyMedium)
                        Text(I18n.t("backup.excludes"),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
            }
            if(!available && !busy) Text(I18n.t("backup.wait"),style=MaterialTheme.typography.bodySmall)
            Spacer(Modifier.height(8.dp))
        }
        if(confirming || finished || failure!=null) {
            HorizontalDivider(Modifier.padding(vertical=12.dp))
            val primary=when {confirming->"backup.merge";failure!=null->"backup.retry";message=="backup.created"->"backup.save";else->"manage.done"}
            Button(onClick={
                when {
                    confirming->merge()
                    failure!=null->when(operation) {
                        "restore"->merge()
                        "export"->create()
                        "save"->exported?.let {save.launch(File(it).name)}
                        else->choose.launch(arrayOf("*/*"))
                    }
                    message=="backup.created"->exported?.let {save.launch(File(it).name)}
                    else->onDone()
                }
            },enabled=available,modifier=Modifier.align(Alignment.CenterHorizontally).widthIn(max=400.dp).fillMaxWidth().heightIn(min=48.dp)) {Text(I18n.t(primary))}
            TextButton(onClick=::home,enabled=!busy,modifier=Modifier.fillMaxWidth().heightIn(min=44.dp)) {Text(I18n.t("backup.back"))}
        }
    }
}

@Composable
private fun BackupActionRow(title:String,subtitle:String,icon:androidx.compose.ui.graphics.vector.ImageVector,enabled:Boolean,onClick:()->Unit) {
    ListItem(headlineContent={Text(I18n.t(title))},supportingContent={Text(I18n.t(subtitle))},
        leadingContent={Icon(icon,null,tint=MaterialTheme.colorScheme.primary)},
        trailingContent={Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight,null,tint=MaterialTheme.colorScheme.onSurfaceVariant)},
        colors=ListItemDefaults.colors(containerColor=MaterialTheme.colorScheme.surfaceContainerLow),
        modifier=Modifier.clickable(enabled=enabled,role=Role.Button,onClick=onClick))
}

internal fun backupFailureHint(error:String):String {
    val lower=error.lowercase(java.util.Locale.ROOT)
    return when {
        lower.contains("backup changed after verification")->"backup.error_changed"
        lower.contains("unsupported library backup version")->"backup.error_version"
        lower.contains("no space left") || lower.contains("os error 28")->"backup.error_space"
        lower.contains("permission denied") || lower.contains("access is denied")->"backup.error_permission"
        else->"backup.error_hint"
    }
}
