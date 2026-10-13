package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.foundation.layout.*
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
fun TranslateDialog(
    selectedText: String,
    translateApiUrl: String,
    translateApiKey: String,
    onDismiss: () -> Unit,
) {
    var translatedText by remember {
        mutableStateOf(I18n.t("selection.translating") ?: "Translating...")
    }

    LaunchedEffect(selectedText) {
        if (translateApiUrl.isNotEmpty()) {
            translatedText = "[] of translation from API (Mock)"
        } else {
            translatedText = I18n.t("settings.translate_api_url") + " not set."
        }
    }

    Dialog(onDismissRequest = onDismiss) {
        Surface(
            shape = ReaderGeometry.DialogShape,
            color = MaterialTheme.colorScheme.surface,
            tonalElevation = 6.dp,
        ) {
            Column(modifier = Modifier.padding(ReaderGeometry.CardInset)) {
                Text(
                    I18n.t("selection.translate") ?: "Translate",
                    fontWeight = FontWeight.Bold,
                    style = MaterialTheme.typography.titleMedium,
                )
                Spacer(modifier = Modifier.height(8.dp))
                Text(selectedText, style = MaterialTheme.typography.bodyMedium, color = Color.Gray)
                HorizontalDivider(modifier = Modifier.padding(vertical = 8.dp))
                Text(translatedText, style = MaterialTheme.typography.bodyLarge)
                Spacer(modifier = Modifier.height(16.dp))
                Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                    TextButton(
                        onClick = onDismiss,
                        shape = ReaderGeometry.DialogShape,
                        contentPadding = ReaderGeometry.ButtonPadding,
                    ) {
                        Text(I18n.t("common.close") ?: "Close")
                    }
                }
            }
        }
    }
}

@Composable
fun DictionaryDialog(
    selectedText: String,
    dictionaryApiUrl: String,
    dictionaryApiKey: String,
    onDismiss: () -> Unit,
) {
    var meaning by remember { mutableStateOf(I18n.t("selection.loading") ?: "Loading...") }

    LaunchedEffect(selectedText) {
        if (dictionaryApiUrl.isNotEmpty()) {
            meaning = "Dictionary meaning for: selectedText (Mock)"
        } else {
            meaning = I18n.t("settings.dictionary_api_url") + " not set."
        }
    }

    Dialog(onDismissRequest = onDismiss) {
        Surface(
            shape = ReaderGeometry.DialogShape,
            color = MaterialTheme.colorScheme.surface,
            tonalElevation = 6.dp,
        ) {
            Column(modifier = Modifier.padding(ReaderGeometry.CardInset)) {
                Text(
                    I18n.t("selection.dictionary") ?: "Dictionary",
                    fontWeight = FontWeight.Bold,
                    style = MaterialTheme.typography.titleMedium,
                )
                Spacer(modifier = Modifier.height(8.dp))
                Text(selectedText, style = MaterialTheme.typography.titleLarge)
                HorizontalDivider(modifier = Modifier.padding(vertical = 8.dp))
                Text(meaning, style = MaterialTheme.typography.bodyMedium)
                Spacer(modifier = Modifier.height(16.dp))
                Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                    TextButton(
                        onClick = onDismiss,
                        shape = ReaderGeometry.DialogShape,
                        contentPadding = ReaderGeometry.ButtonPadding,
                    ) {
                        Text(I18n.t("common.close") ?: "Close")
                    }
                }
            }
        }
    }
}

@Composable
fun CorrectionDialog(
    selectedText: String,
    onDismiss: () -> Unit,
) {
    var correctedText by remember {
        mutableStateOf(I18n.t("selection.correcting") ?: "Correcting...")
    }

    LaunchedEffect(selectedText) {
        correctedText = "Mock Correction for: selectedText"
    }

    Dialog(onDismissRequest = onDismiss) {
        Surface(
            shape = ReaderGeometry.DialogShape,
            color = MaterialTheme.colorScheme.surface,
            tonalElevation = 6.dp,
        ) {
            Column(modifier = Modifier.padding(ReaderGeometry.CardInset)) {
                Text(
                    I18n.t("selection.correct") ?: "Correction",
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
                        Text(I18n.t("common.close") ?: "Close")
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
                    I18n.t("selection.note") ?: "Note",
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
                    label = { Text(I18n.t("selection.note_content") ?: "Enter your note") },
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
                        Text(I18n.t("common.cancel") ?: "Cancel")
                    }
                    Spacer(modifier = Modifier.width(8.dp))
                    Button(
                        onClick = { onSaveNote(noteContent) },
                        shape = ReaderGeometry.DialogShape,
                        contentPadding = ReaderGeometry.ButtonPadding,
                    ) {
                        Text(I18n.t("common.save") ?: "Save")
                    }
                }
            }
        }
    }
}
