package com.zhongbai233.epub.reader.ui.library

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Palette
import androidx.compose.material.icons.filled.Translate
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.ui.reader.AppearanceLinkageSettings
import com.zhongbai233.epub.reader.ui.theme.ReaderGeometry
import com.zhongbai233.epub.reader.viewmodel.ReaderViewModel
import kotlinx.coroutines.launch

private enum class LibrarySettingsPage { Home, Appearance, Language, About }

/** One screen owns the settings back stack; the shelf remains mounted beneath it. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LibrarySettingsScreen(vm: ReaderViewModel, onDismiss: () -> Unit) {
    @Suppress("UNUSED_VARIABLE") val languageVersion = I18n.version
    var page by rememberSaveable { mutableStateOf(LibrarySettingsPage.Home) }
    val back = { if (page == LibrarySettingsPage.Home) onDismiss() else page = LibrarySettingsPage.Home }
    BackHandler(onBack = back)
    Surface(Modifier.fillMaxSize()) {
        if (page == LibrarySettingsPage.About) {
            AboutScreen(onNavigateBack = back, onExportLogs = vm::exportFeedbackLogs)
        } else {
            Scaffold(topBar = {
                TopAppBar(
                    title = { Text(I18n.t(when (page) {
                        LibrarySettingsPage.Home -> "settings.title"
                        LibrarySettingsPage.Appearance -> "ui.appearance"
                        else -> "ui.language"
                    })) },
                    navigationIcon = { IconButton(onClick = back) { Icon(Icons.AutoMirrored.Filled.ArrowBack, I18n.t("settings.back")) } },
                )
            }) { padding ->
                Column(
                    Modifier.fillMaxSize().padding(padding).verticalScroll(rememberScrollState())
                        .padding(horizontal = 20.dp, vertical = 16.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Surface(shape = ReaderGeometry.Shape, color = MaterialTheme.colorScheme.surfaceContainerLow,
                        modifier = Modifier.widthIn(max = 600.dp).fillMaxWidth()) {
                        when (page) {
                            LibrarySettingsPage.Home -> Column {
                                LibrarySettingsRow("ui.appearance", Icons.Default.Palette) { page = LibrarySettingsPage.Appearance }
                                HorizontalDivider(Modifier.padding(start = 56.dp))
                                LibrarySettingsRow("ui.language", Icons.Default.Translate) { page = LibrarySettingsPage.Language }
                                HorizontalDivider(Modifier.padding(start = 56.dp))
                                LibrarySettingsRow("about.title", Icons.Default.Info) { page = LibrarySettingsPage.About }
                            }
                            LibrarySettingsPage.Appearance -> Box(Modifier.padding(16.dp)) {
                                AppearanceLinkageSettings(vm.appearanceLinkage, vm.systemAppearanceDark,
                                    vm.ambientAppearanceDark, vm.ambientSensorAvailable, vm::updateAppearanceLinkage, showTitle = false)
                            }
                            LibrarySettingsPage.Language -> Column(Modifier.selectableGroup()) {
                                I18n.availableLanguages.forEach { (code, label) ->
                                    LibraryChoiceRow(label, vm.readerLanguage == code) { vm.updateLanguage(code) }
                                }
                            }
                            LibrarySettingsPage.About -> Unit
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun LibrarySettingsRow(key: String, icon: ImageVector, onClick: () -> Unit) {
    ListItem(
        headlineContent = { Text(I18n.t(key)) },
        leadingContent = { Icon(icon, null, tint = MaterialTheme.colorScheme.primary) },
        trailingContent = { Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, tint = MaterialTheme.colorScheme.onSurfaceVariant) },
        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
        modifier = Modifier.clickable(role = Role.Button, onClick = onClick),
    )
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun LibrarySortDisplaySheet(
    sortTitle: Boolean, listMode: Boolean,
    onSort: (Boolean) -> Unit, onDisplay: (Boolean) -> Unit, onDismiss: () -> Unit,
) {
    val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val scope = rememberCoroutineScope()
    @Suppress("UNUSED_VARIABLE") val languageVersion = I18n.version
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = sheet) {
        Column(Modifier.fillMaxWidth().heightIn(max = 620.dp).padding(horizontal = 24.dp),
            horizontalAlignment = Alignment.CenterHorizontally) {
            Text(I18n.t("library.sort_display"), style = MaterialTheme.typography.titleLarge,
                modifier = Modifier.fillMaxWidth().padding(bottom = 16.dp))
            LazyColumn(Modifier.weight(1f, fill = false)) {
                item {
                    Text(I18n.t("common.sort"), style = MaterialTheme.typography.labelLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(vertical = 8.dp))
                    Column(Modifier.selectableGroup()) {
                        LibraryChoiceRow(I18n.t("library.sort_recently_read"), !sortTitle) { onSort(false) }
                        LibraryChoiceRow(I18n.t("txt.book_title"), sortTitle) { onSort(true) }
                    }
                    HorizontalDivider(Modifier.padding(vertical = 12.dp))
                    Text(I18n.t("library.display"), style = MaterialTheme.typography.labelLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(vertical = 8.dp))
                    Column(Modifier.selectableGroup()) {
                        LibraryChoiceRow(I18n.t("library.view_grid"), !listMode) { onDisplay(false) }
                        LibraryChoiceRow(I18n.t("library.view_list"), listMode) { onDisplay(true) }
                    }
                }
            }
            TextButton(onClick = { scope.launch { sheet.hide(); onDismiss() } }, modifier = Modifier.fillMaxWidth().padding(vertical = 8.dp).heightIn(min = 48.dp)) {
                Text(I18n.t("manage.done"))
            }
        }
    }
}

@Composable
private fun LibraryChoiceRow(label: String, selected: Boolean, onClick: () -> Unit) {
    Row(Modifier.fillMaxWidth().heightIn(min = 52.dp)
        .selectable(selected = selected, role = Role.RadioButton, onClick = onClick).padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
        Text(label, modifier = Modifier.weight(1f), style = MaterialTheme.typography.bodyLarge)
        RadioButton(selected = selected, onClick = null)
    }
}
