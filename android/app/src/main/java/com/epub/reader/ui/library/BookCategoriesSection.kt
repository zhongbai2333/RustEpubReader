package com.zhongbai233.epub.reader.ui.library

import androidx.compose.animation.*
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.tween
import androidx.compose.animation.core.updateTransition
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.MoreHoriz
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.model.BookCategory
import com.zhongbai233.epub.reader.model.BookClassification
import org.json.JSONArray
import org.json.JSONObject

private data class CategoryPanel(val managing: Boolean, val books: List<String> = emptyList())

@Composable
internal fun BookCategoriesSection(
    state: BookClassification,
    filter: String?,
    onFilter: (String?) -> Unit,
    targets: List<String>?,
    busy: Boolean,
    failure: String?,
    onAction: (JSONObject, () -> Unit) -> Unit,
    reduceMotion: Boolean = false,
    onPanelActivity: (Boolean) -> Unit = {},
    onDismiss: () -> Unit,
) {
    var managing by remember { mutableStateOf(false) }
    var naming by remember { mutableStateOf(false) }
    var editing by remember { mutableStateOf<BookCategory?>(null) }
    var name by remember { mutableStateOf("") }
    var deleting by remember { mutableStateOf<String?>(null) }
    val focus = LocalFocusManager.current
    val keyboard = LocalSoftwareKeyboardController.current
    val nameFocus = remember { FocusRequester() }
    fun stopNaming() { if (naming) { focus.clearFocus(); keyboard?.hide() }; naming = false }
    LaunchedEffect(naming) { if (naming) nameFocus.requestFocus() }
    LaunchedEffect(filter) { managing = false; stopNaming() }
    LaunchedEffect(targets) { if (targets != null) { managing = false; stopNaming(); deleting = null } }
    val panel = if (managing) CategoryPanel(true) else targets?.let { CategoryPanel(false, it) }
    val panelTransition = updateTransition(panel, label = "category-panel-state")
    val panelActive = panel != null || panelTransition.currentState != null || panelTransition.isRunning
    LaunchedEffect(panelActive) { onPanelActivity(panelActive) }
    DisposableEffect(Unit) { onDispose { onPanelActivity(false) } }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Row(Modifier.weight(1f).horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilterChip(modifier = Modifier.animateContentSize(if (reduceMotion) snap() else tween(240)), selected = filter == null, onClick = { onFilter(null) }, label = { Text(I18n.t("ui.all_books")) }, leadingIcon = { if (filter == null) Icon(Icons.Default.Check, null, Modifier.size(16.dp)) })
                FilterChip(modifier = Modifier.animateContentSize(if (reduceMotion) snap() else tween(240)), selected = filter == "", onClick = { onFilter("") }, label = { Text(I18n.t("category.uncategorized")) }, leadingIcon = { if (filter == "") Icon(Icons.Default.Check, null, Modifier.size(16.dp)) })
                state.categories.forEach { category ->
                    FilterChip(modifier = Modifier.animateContentSize(if (reduceMotion) snap() else tween(240)), selected = filter == category.id, onClick = { onFilter(category.id) }, label = { Text(category.name, Modifier.widthIn(max = 160.dp), maxLines = 1, overflow = TextOverflow.Ellipsis) }, leadingIcon = { if (filter == category.id) Icon(Icons.Default.Check, null, Modifier.size(16.dp)) })
                }
            }
            if (LocalConfiguration.current.screenWidthDp >= 600) {
                FilterChip(selected = managing, onClick = { onPanelActivity(true); managing = !managing; if (!managing) stopNaming(); onDismiss() }, enabled = !busy,
                    modifier = Modifier.padding(start = 8.dp), label = { Text(I18n.t("category.manage")) },
                    leadingIcon = { CategoryManagementIcon() })
            } else {
                IconToggleButton(checked = managing, onCheckedChange = { onPanelActivity(true); managing = it; if (!it) stopNaming(); onDismiss() }, enabled = !busy) { CategoryManagementIcon() }
            }
        }
        panelTransition.AnimatedContent(modifier = Modifier.fillMaxWidth(), contentAlignment = Alignment.TopStart, contentKey = { it?.managing },
            transitionSpec = {
                (if (reduceMotion) EnterTransition.None togetherWith ExitTransition.None
                else (fadeIn(tween(200)) + scaleIn(tween(300), initialScale = 0.98f, transformOrigin = TransformOrigin(0.5f, 0f))) togetherWith fadeOut(tween(140)))
                    .using(SizeTransform(clip = true) { _, _ -> if (reduceMotion) snap() else tween(300, easing = FastOutSlowInEasing) })
            }) { shown ->
            if (shown == null) Spacer(Modifier.fillMaxWidth().height(0.dp))
            else Surface(modifier = Modifier.animateContentSize(if (reduceMotion) snap() else tween(260, easing = FastOutSlowInEasing)), shape = MaterialTheme.shapes.medium, color = MaterialTheme.colorScheme.surface) {
                val common = shown.books.map { state.assignments[it] ?: "" }.distinct().singleOrNull()
                Column(Modifier.fillMaxWidth().padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text(I18n.t(if (shown.managing) "category.manage" else "category.assign"), style = MaterialTheme.typography.titleSmall)
                            if (!shown.managing) Text(I18n.tf1("manage.count", shown.books.size.toString()), style = MaterialTheme.typography.bodySmall)
                        }
                        TextButton(onClick = { if (shown.managing) { managing = false; stopNaming() } else onDismiss() }, enabled = !busy && panel != null) { Text(I18n.t("manage.done")) }
                    }
                    if (shown.managing) {
                        Text(I18n.t("category.note"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        if (naming) {
                            OutlinedTextField(value = name, onValueChange = { name = it }, label = { Text(I18n.t("category.name")) }, singleLine = true, modifier = Modifier.fillMaxWidth().focusRequester(nameFocus), enabled = !busy)
                            Row {
                                TextButton(onClick = { onAction(JSONObject().put("action", if (editing == null) "create" else "rename").put("id", editing?.id ?: "").put("name", name)) { stopNaming() } }, enabled = !busy && name.isNotBlank()) { Text(I18n.t("error.ok")) }
                                TextButton(onClick = { stopNaming() }, enabled = !busy) { Text(I18n.t("settings.cancel")) }
                            }
                        } else TextButton(onClick = { editing = null; name = ""; naming = true; deleting = null }, enabled = !busy) { Text(I18n.t("category.create")) }
                        Column(Modifier.heightIn(max = 160.dp).verticalScroll(rememberScrollState())) {
                            state.categories.forEach { category ->
                                key(category.id) {
                                    var menu by remember { mutableStateOf(false) }
                                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                                        Text(category.name, Modifier.weight(1f), maxLines = 2, overflow = TextOverflow.Ellipsis)
                                        Box {
                                            IconButton(onClick = { menu = true }, enabled = !busy) { Icon(Icons.Default.MoreHoriz, category.name + ", " + I18n.t("category.manage")) }
                                            DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                                                DropdownMenuItem(text = { Text(I18n.t("category.rename")) }, onClick = { menu = false; editing = category; name = category.name; naming = true; deleting = null })
                                                DropdownMenuItem(text = { Text(I18n.t("category.delete")) }, onClick = { menu = false; deleting = category.id; stopNaming() })
                                            }
                                        }
                                    }
                                    if (deleting == category.id) {
                                        Text(I18n.t("category.delete_note"), style = MaterialTheme.typography.bodySmall)
                                        Row {
                                            TextButton(onClick = { onAction(JSONObject().put("action", "delete").put("id", category.id)) { deleting = null } }, enabled = !busy) { Text(I18n.t("category.delete"), color = MaterialTheme.colorScheme.error) }
                                            TextButton(onClick = { deleting = null }, enabled = !busy) { Text(I18n.t("settings.cancel")) }
                                        }
                                    }
                                    HorizontalDivider()
                                }
                            }
                        }
                    } else {
                        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            (listOf(BookCategory("", I18n.t("category.uncategorized"))) + state.categories).forEach { category ->
                                FilterChip(selected = common == category.id, onClick = { onAction(JSONObject().put("action", "assign").put("id", category.id).put("books", JSONArray(shown.books)), onDismiss) }, enabled = !busy && panel != null,
                                    label = { Text(category.name, Modifier.widthIn(max = 160.dp), maxLines = 1, overflow = TextOverflow.Ellipsis) }, leadingIcon = { if (common == category.id) Icon(Icons.Default.Check, null, Modifier.size(16.dp)) })
                            }
                        }
                    }
                    if (failure != null) Text(failure, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
                }
            }
        }
    }
}

@Composable
private fun CategoryManagementIcon() {
    Box(Modifier.size(24.dp)) {
        Icon(Icons.Default.Folder, null, Modifier.size(22.dp).align(Alignment.CenterStart))
        Icon(Icons.Default.Settings, I18n.t("category.manage"), Modifier.size(12.dp).align(Alignment.BottomEnd))
    }
}
