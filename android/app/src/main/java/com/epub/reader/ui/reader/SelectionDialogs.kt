package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.ui.theme.ReaderGeometry

@Composable
fun TranslateDialog(selectedText:String,translateApiUrl:String,translateApiKey:String,onDismiss:()->Unit,model:String="gpt-4o-mini",provider:String="auto"){
    NativeServiceDialog(selectedText,translateApiUrl,translateApiKey,"translate",onDismiss,model,provider)
}
@Composable
fun DictionaryDialog(selectedText:String,dictionaryApiUrl:String,dictionaryApiKey:String,onDismiss:()->Unit,model:String="gpt-4o-mini",provider:String="auto"){
    NativeServiceDialog(selectedText,dictionaryApiUrl,dictionaryApiKey,"dictionary",onDismiss,model,provider)
}
@Composable
private fun NativeServiceDialog(text:String,endpoint:String,key:String,kind:String,onDismiss:()->Unit,model:String,provider:String){
    val context=androidx.compose.ui.platform.LocalContext.current
    var result by remember(text,endpoint,kind){mutableStateOf<String?>(null)}
    var error by remember(text,endpoint,kind){mutableStateOf<String?>(null)}
    var loading by remember {mutableStateOf(false)}
    var retry by remember {mutableIntStateOf(0)}
    var target by remember{mutableStateOf("zh")}
    LaunchedEffect(text,endpoint,key,kind,retry,target,model,provider){
        if(endpoint.isBlank())return@LaunchedEffect
        loading=true;error=null;result=null
        try{
            val value=kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO){
                val request=org.json.JSONObject().put("op","service").put("endpoint",endpoint).put("key",key).put("text",text).put("kind",kind).put("target",target).put("model",model).put("provider",provider)
                val envelope=org.json.JSONObject(com.zhongbai233.epub.reader.RustBridge.readingTools(request.toString())?:error("No reading service response"))
                check(envelope.optBoolean("ok")){envelope.optString("error")};envelope.getString("result")
            };result=value
        }catch(e:kotlinx.coroutines.CancellationException){throw e}catch(e:Exception){error=e.message}finally{loading=false}
    }
    AlertDialog(onDismissRequest=onDismiss,title={Text(I18n.t(if(kind=="dictionary")"context.dictionary" else "context.translate"))},
        text={Column(Modifier.fillMaxWidth().heightIn(max=420.dp).verticalScroll(rememberScrollState()),verticalArrangement=Arrangement.spacedBy(12.dp)){
            androidx.compose.foundation.text.selection.SelectionContainer{Text(text,color=MaterialTheme.colorScheme.onSurfaceVariant)}
            HorizontalDivider()
            if(kind=="translate")Row(horizontalArrangement=Arrangement.spacedBy(8.dp)){
                Text(I18n.t("services.target"));FilterChip(selected=target=="zh",onClick={target="zh"},enabled=!loading,label={Text(I18n.t("fonts.chinese"))});FilterChip(selected=target=="en",onClick={target="en"},enabled=!loading,label={Text(I18n.t("fonts.english"))})
            }
            if(loading)CircularProgressIndicator(Modifier.size(24.dp))
            result?.let{value->androidx.compose.foundation.text.selection.SelectionContainer{Text(value)}}
            error?.let{Text(it,color=MaterialTheme.colorScheme.error);TextButton(onClick={retry++}){Text(I18n.t("services.retry"))}}
            if(endpoint.isBlank()){
                Text(I18n.t("services.missing"))
                TextButton(onClick={val uri=if(kind=="dictionary") "https://www.zdic.net/hans/${android.net.Uri.encode(text)}" else "https://translate.google.com/?sl=auto&tl=zh-CN&text=${android.net.Uri.encode(text)}";runCatching{context.startActivity(android.content.Intent(android.content.Intent.ACTION_VIEW,android.net.Uri.parse(uri)))}}){Text(I18n.t(if(kind=="dictionary")"services.online_dictionary" else "services.online_translation"))}
            }
        }},confirmButton={TextButton(onClick=onDismiss){Text(I18n.t("common.close"))}})
}

@Composable
fun CorrectionDialog(
    selectedText: String,
    onDismiss: () -> Unit,
) {
    val correctedText = I18n.t("selection.correction_placeholder")

    Dialog(onDismissRequest = onDismiss) {
        Surface(
            shape = ReaderGeometry.DialogShape,
            color = MaterialTheme.colorScheme.surface,
            tonalElevation = 6.dp,
        ) {
            Column(modifier = Modifier.padding(ReaderGeometry.CardInset)) {
                Text(
                    I18n.t("selection.correct"),
                    fontWeight = FontWeight.Bold,
                    style = MaterialTheme.typography.titleMedium,
                )
                Spacer(modifier = Modifier.height(8.dp))
                Text(selectedText, style = MaterialTheme.typography.bodyMedium, color = Color.Gray)
                HorizontalDivider(modifier = Modifier.padding(vertical = 8.dp))
                Text(correctedText, style = MaterialTheme.typography.bodyLarge)
                Spacer(modifier = Modifier.height(16.dp))
                Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                    TextButton(
                        onClick = onDismiss,
                        shape = ReaderGeometry.DialogShape,
                        contentPadding = ReaderGeometry.ButtonPadding,
                    ) {
                        Text(I18n.t("common.close"))
                    }
                }
            }
        }
    }
}

@Composable
fun NoteDialog(
    selectedText: String,
    onSaveNote: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    var noteContent by remember { mutableStateOf("") }

    Dialog(onDismissRequest = onDismiss) {
        Surface(
            shape = ReaderGeometry.DialogShape,
            color = MaterialTheme.colorScheme.surface,
            tonalElevation = 6.dp,
        ) {
            Column(modifier = Modifier.padding(ReaderGeometry.CardInset)) {
                Text(
                    I18n.t("selection.note"),
                    fontWeight = FontWeight.Bold,
                    style = MaterialTheme.typography.titleMedium,
                )
                Spacer(modifier = Modifier.height(8.dp))
                Text(
                    selectedText,
                    style = MaterialTheme.typography.bodyMedium,
                    color = Color.Gray,
                    maxLines = 3,
                )
                Spacer(modifier = Modifier.height(8.dp))
                OutlinedTextField(
                    value = noteContent,
                    onValueChange = { noteContent = it },
                    label = { Text(I18n.t("selection.note_content")) },
                    modifier = Modifier.fillMaxWidth(),
                    minLines = 3,
                    shape = ReaderGeometry.DialogShape,
                )
                Spacer(modifier = Modifier.height(16.dp))
                Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                    TextButton(
                        onClick = onDismiss,
                        shape = ReaderGeometry.DialogShape,
                        contentPadding = ReaderGeometry.ButtonPadding,
                    ) {
                        Text(I18n.t("common.cancel"))
                    }
                    Spacer(modifier = Modifier.width(8.dp))
                    Button(
                        onClick = { onSaveNote(noteContent) },
                        shape = ReaderGeometry.DialogShape,
                        contentPadding = ReaderGeometry.ButtonPadding,
                    ) {
                        Text(I18n.t("common.save"))
                    }
                }
            }
        }
    }
}
