package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.MoreHoriz
import androidx.compose.material.icons.filled.Swipe
import androidx.compose.material.icons.filled.BatteryFull
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.i18n.I18n

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun ReaderIntroductionSheet(onShown: () -> Unit, onDismiss: () -> Unit) {
    val latestShown by rememberUpdatedState(onShown)
    LaunchedEffect(Unit) { latestShown() }
    val sheetState=rememberModalBottomSheetState(skipPartiallyExpanded=true)
    val closeSheet = rememberReaderSheetDismiss(sheetState, onDismiss)
    ModalBottomSheet(onDismissRequest=onDismiss,sheetState=sheetState) {
        Column(Modifier.fillMaxWidth().heightIn(max=LocalConfiguration.current.screenHeightDp.dp*0.82f)
            .padding(horizontal=24.dp).navigationBarsPadding().padding(bottom=20.dp),verticalArrangement=Arrangement.spacedBy(20.dp)) {
            Row(Modifier.fillMaxWidth(),verticalAlignment=Alignment.CenterVertically) {
                Text(I18n.t("reader.intro_title"),style=MaterialTheme.typography.titleLarge,modifier=Modifier.weight(1f).semantics {heading()})
                IconButton(onClick=closeSheet) {Icon(Icons.Default.Close,I18n.t("settings.close"))}
            }
            Column(Modifier.weight(1f,fill=false).verticalScroll(rememberScrollState()),verticalArrangement=Arrangement.spacedBy(22.dp)) {
                IntroductionRow(Icons.Default.Swipe,"reader.intro_turns_title","reader.intro_turns_mobile")
                IntroductionRow(Icons.Default.MoreHoriz,"reader.intro_menu_title","reader.intro_menu_mobile")
                IntroductionRow(Icons.Default.BatteryFull,"reader.intro_status_title","reader.intro_status_body")
            }
            Button(onClick=closeSheet,modifier=Modifier.fillMaxWidth().heightIn(min=48.dp)) {Text(I18n.t("reader.intro_start"))}
        }
    }
}

@Composable
private fun IntroductionRow(icon:ImageVector,title:String,body:String) {
    Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.spacedBy(14.dp),verticalAlignment=Alignment.Top) {
        Icon(icon,null,Modifier.size(26.dp),tint=MaterialTheme.colorScheme.primary)
        Column(Modifier.weight(1f),verticalArrangement=Arrangement.spacedBy(6.dp)) {
            Text(I18n.t(title),style=MaterialTheme.typography.titleSmall)
            Text(I18n.t(body),style=MaterialTheme.typography.bodyMedium,color=MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}
