package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.SheetState
import androidx.compose.runtime.*
import kotlinx.coroutines.launch

/** Explicit close buttons must finish the sheet's downward animation before
 * the host removes it. Material already animates scrim/back/swipe dismissal. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun rememberReaderSheetDismiss(sheet: SheetState, onDismiss: () -> Unit): () -> Unit {
    val scope = rememberCoroutineScope()
    val latestDismiss by rememberUpdatedState(onDismiss)
    var closing by remember(sheet) { mutableStateOf(false) }
    return {
        if (!closing) {
            closing = true
            scope.launch {
                try {
                    sheet.hide()
                    if (!sheet.isVisible) latestDismiss()
                } finally {
                    closing = false
                }
            }
        }
    }
}
