package com.zhongbai233.epub.reader.ui.library

import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.MenuBook
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Sync
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.zhongbai233.epub.reader.i18n.I18n
import kotlinx.coroutines.delay

/** One bottom tool rail; search filters the existing shelf instead of opening a tab. */
@Composable
fun HomeNavigation(
    sync: Boolean, onLibrary: () -> Unit, onSync: () -> Unit,
    onSearch: () -> Unit = {}, searching: Boolean = false, query: String = "",
    onQueryChange: (String) -> Unit = {}, onSearchClose: () -> Unit = {}, interactive: Boolean = true,
) {
    var appeared by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { appeared = true }
    val progress by animateFloatAsState(if (searching && appeared) 1f else 0f, tween(260), label = "shelf-search-width")
    val density = LocalDensity.current
    var railHeight by remember { mutableStateOf(56.dp) }
    val focus = remember { FocusRequester() }
    val focusManager = LocalFocusManager.current
    val keyboard = LocalSoftwareKeyboardController.current
    val cancel = { focusManager.clearFocus(); keyboard?.hide(); onSearchClose() }
    BackHandler(enabled = searching && interactive, onBack = cancel)
    LaunchedEffect(searching) {
        if (searching && interactive) { delay(120); focus.requestFocus(); keyboard?.show() }
    }
    LaunchedEffect(interactive) { if (!interactive) { focusManager.clearFocus(); keyboard?.hide() } }
    BoxWithConstraints(Modifier.fillMaxWidth().imePadding()
        .windowInsetsPadding(NavigationBarDefaults.windowInsets)
        .padding(horizontal = 16.dp, vertical = 8.dp)) {
        val availableWidth = maxWidth
        // Native font metrics can increase the row; do not clip large-text labels.
        Box(Modifier.fillMaxWidth().heightIn(min = 56.dp), contentAlignment = Alignment.CenterEnd) {
            if (progress < 0.99f) {
                Surface(shape = RoundedCornerShape(32.dp), color = MaterialTheme.colorScheme.surfaceContainer,
                    tonalElevation = 2.dp, shadowElevation = 2.dp,
                    modifier = Modifier.align(Alignment.CenterStart).width(minOf(availableWidth - 68.dp, 224.dp)).alpha(1f - progress).onSizeChanged { size -> railHeight = with(density) { size.height.toDp() } }) {
                    Row(Modifier.padding(4.dp).selectableGroup()) {
                        HomeItem(!sync, I18n.t("library.title"), Icons.AutoMirrored.Filled.MenuBook, onLibrary, Modifier.weight(1f), !searching)
                        HomeItem(sync, I18n.t("sync.multiplatform"), Icons.Default.Sync, onSync, Modifier.weight(1f), !searching)
                    }
                }
            }
            Surface(shape = CircleShape, color = MaterialTheme.colorScheme.surfaceContainer,
                tonalElevation = 2.dp, shadowElevation = 2.dp,
                modifier = Modifier.width(railHeight + (availableWidth - railHeight) * progress).clipToBounds()) {
                if (searching) Row(Modifier.fillMaxWidth().heightIn(min = railHeight), verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Default.Search, null, Modifier.padding(start = 14.dp, end = 8.dp).size(20.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
                    BasicTextField(query, onQueryChange,
                        modifier = Modifier.weight(1f).heightIn(min = 48.dp).wrapContentHeight().focusRequester(focus)
                            .semantics { contentDescription = I18n.t("ui.search_books") },
                        singleLine = true, textStyle = MaterialTheme.typography.bodyLarge.copy(color = MaterialTheme.colorScheme.onSurface),
                        cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
                        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                        keyboardActions = KeyboardActions(onSearch = { focusManager.clearFocus(); keyboard?.hide() }),
                        decorationBox = { inner -> Box(contentAlignment = Alignment.CenterStart) {
                            if (query.isEmpty()) Text(I18n.t("ui.search_books"), style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
                            inner()
                        } })
                    if (query.isNotEmpty()) IconButton(onClick = { onQueryChange("") }) { Icon(Icons.Default.Close, I18n.t("ui.clear_search"), Modifier.size(18.dp)) }
                    TextButton(onClick = cancel, modifier = Modifier.heightIn(min = 48.dp), contentPadding = PaddingValues(horizontal = 16.dp)) { Text(I18n.t("settings.cancel"), maxLines = 1) }
                } else IconButton(onClick = onSearch, modifier = Modifier.size(railHeight)) { Icon(Icons.Default.Search, I18n.t("ui.search_books")) }
            }
        }
    }
}

@Composable
private fun HomeItem(selected: Boolean, title: String, icon: ImageVector, action: () -> Unit, modifier: Modifier, enabled: Boolean) {
    Column(modifier.clip(CircleShape)
        .selectable(selected = selected, role = Role.Tab, enabled = enabled, onClick = action)
        .background(if (selected) MaterialTheme.colorScheme.primaryContainer else androidx.compose.ui.graphics.Color.Transparent)
        .heightIn(min = 48.dp).padding(horizontal = 8.dp, vertical = 4.dp),
        horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
        val tint = if (selected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant
        Icon(icon, null, Modifier.size(20.dp), tint = tint)
        Spacer(Modifier.height(3.dp))
        Text(title, color = tint, fontSize = 11.sp, lineHeight = 14.sp, maxLines = 2,
            textAlign = androidx.compose.ui.text.style.TextAlign.Center)
    }
}
