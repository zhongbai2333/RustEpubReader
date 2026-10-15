package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.ui.theme.ReaderGeometry
import com.zhongbai233.epub.reader.viewmodel.ReaderViewModel

/** Compact entry used in reader settings. Authorization lives in one account screen. */
@Composable
fun GithubSyncSettings(vm: ReaderViewModel, onManageAccount: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        GithubAccountSummary(vm)
        OutlinedButton(
            onClick = onManageAccount,
            shape = ReaderGeometry.Shape,
            contentPadding = ReaderGeometry.ButtonPadding,
        ) {
            Text(I18n.t("sync.multiplatform"))
        }
    }
}

@Composable
private fun GithubAccountSummary(vm: ReaderViewModel) {
    Text(
        if (vm.githubToken != null && vm.githubUsername != null)
            I18n.tf1("account.logged_in", vm.githubUsername!!)
        else I18n.t("account.signed_out")
    )
    Text(
        I18n.t(if (vm.githubSyncEnabled) "cloud.enabled" else "cloud.disabled"),
        style = MaterialTheme.typography.bodySmall,
    )
}

/** Full-width screen shared by library, reader and contribution entries. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MultiPlatformSyncDialog(vm: ReaderViewModel, onDismiss: () -> Unit) {
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        com.zhongbai233.epub.reader.ui.library.SyncScreen(vm, onBack = onDismiss)
    }
}

@Composable
fun GithubAccountContent(vm: ReaderViewModel, embedded: Boolean = false) {
    val uriHandler = LocalUriHandler.current
    val clipboard = LocalClipboardManager.current
    var showDetails by remember { mutableStateOf(false) }
    var showPermissions by remember { mutableStateOf(false) }
    var showContributions by remember { mutableStateOf(false) }
    Column(
        (if (embedded) Modifier.fillMaxWidth() else Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(ReaderGeometry.PageInset)),
        verticalArrangement = Arrangement.spacedBy(ReaderGeometry.BlockGap),
    ) {
        if (vm.githubAuthPolling) {
            Text(I18n.t("account.auth_title"), style = MaterialTheme.typography.titleMedium)
            Text(I18n.t("account.auth_instruction"))
            Text(
                I18n.t(
                    if (vm.githubAuthForSync) "cloud.permissions" else "account.basic_permissions"
                ),
                style = MaterialTheme.typography.bodySmall,
            )
            if (vm.githubAuthForSync) PermissionDetails(showPermissions) { showPermissions = !showPermissions }
            vm.githubUserCode?.let { code ->
                Text(
                    code,
                    style =
                        MaterialTheme.typography.headlineMedium.copy(
                            fontFamily = FontFamily.Monospace
                        ),
                )
                OutlinedButton(
                    onClick = { clipboard.setText(AnnotatedString(code)) },
                    modifier = Modifier.fillMaxWidth(),
                    shape = ReaderGeometry.Shape,
                    contentPadding = ReaderGeometry.ButtonPadding,
                ) {
                    Text(I18n.t("csc.copy_code"))
                }
                Button(
                    onClick = { uriHandler.openUri("https://github.com/login/device") },
                    modifier = Modifier.fillMaxWidth(),
                    shape = ReaderGeometry.Shape,
                    contentPadding = ReaderGeometry.ButtonPadding,
                ) {
                    Text(I18n.t("account.open_auth"))
                }
            }
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(ReaderGeometry.BlockGap),
            ) {
                CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
                Text(
                    I18n.t(
                        if (vm.githubUserCode != null) "account.waiting" else "csc.requesting_code"
                    )
                )
            }
            TextButton(
                onClick = vm::cancelGitHubLogin,
                shape = ReaderGeometry.Shape,
                contentPadding = ReaderGeometry.ButtonPadding,
            ) {
                Text(I18n.t("account.cancel_auth"))
            }
        } else {
            Text(I18n.t("sync.account_section"), style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Surface(color = MaterialTheme.colorScheme.surface, shape = ReaderGeometry.Shape) {
                Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    SyncValueRow(I18n.t("sync.account"), vm.githubUsername ?: I18n.t("account.signed_out"))
                    SyncValueRow(I18n.t("sync.repo"), "EpubReaderInfo")
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                        TextButton(onClick = { vm.startGitHubLogin(privateSync = true, enableAfterLogin = false) }) { Text(I18n.t(if (vm.githubToken == null) "account.login" else "account.reauthorize")) }
                        if (vm.githubToken != null) TextButton(onClick = vm::logoutGitHub) { Text(I18n.t("csc.logout"), color = MaterialTheme.colorScheme.error) }
                    }
                }
            }
            Column(Modifier.padding(horizontal = 16.dp)) {
                Text(I18n.t("sync.scope_note"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                PermissionDetails(showPermissions) { showPermissions = !showPermissions }
            }
            Text(I18n.t("sync.state_section"), style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Surface(color = MaterialTheme.colorScheme.surface, shape = ReaderGeometry.Shape) {
                Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                        Text(I18n.t("cloud.enable"), modifier = Modifier.weight(1f))
                        Switch(checked = vm.githubSyncEnabled, enabled = vm.githubToken != null && vm.githubRepoAuthorized && !vm.githubSyncNeedsAuth, onCheckedChange = { if (it) vm.prepareGithubSync() else vm.updateGithubSyncEnabled(false) })
                    }
                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                        Text(I18n.t("sync.contributions"), modifier = Modifier.weight(1f))
                        Switch(checked = vm.githubSyncContributions, enabled = !vm.githubSyncRunning, onCheckedChange = vm::updateGithubSyncContributions)
                    }
                    Text(I18n.t("sync.contributions_note"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    TextButton(onClick = { showContributions = !showContributions }, contentPadding = PaddingValues(0.dp)) { Text(I18n.t("sync.contributions_details")) }
                    if (showContributions) Text(I18n.t("sync.contributions_detail"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    if (vm.githubSyncNeedsAuth) TextButton(onClick = { vm.startGitHubLogin(privateSync = true, enableAfterLogin = false) }) { Text(I18n.t("account.reauthorize")) }
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                        OutlinedButton(onClick = vm::syncGithubNow, enabled = vm.githubSyncEnabled && !vm.githubSyncRunning, shape = ReaderGeometry.Shape) { Text(I18n.t("cloud.now")) }
                    }
                    if (vm.githubSyncRunning) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
                    if (vm.githubSyncStatus.isNotBlank()) Text(I18n.message(vm.githubSyncStatus), style = MaterialTheme.typography.bodySmall)
                    if (vm.githubSyncFailures.isNotEmpty()) {
                        Text(I18n.tf1("cloud.files_failed", vm.githubSyncFailures.size.toString()), style = MaterialTheme.typography.bodySmall)
                        vm.githubSyncFailures.forEach { failure ->
                            Text(failure.optString("title"), style = MaterialTheme.typography.bodySmall)
                        }
                        OutlinedButton(onClick = vm::retryGithubFiles, enabled = vm.githubSyncEnabled && !vm.githubSyncRunning && !vm.githubSyncThrottled, shape = ReaderGeometry.Shape) { Text(I18n.t("cloud.retry_files")) }
                    }
                    vm.githubSyncLastSuccess?.let { completed -> Text(I18n.tf1("cloud.last_sync", ((System.currentTimeMillis()-completed).coerceAtLeast(0)/60_000).toString()), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                    if (vm.githubSyncLastSuccess == null) SyncValueRow(I18n.t("sync.last_sync"), I18n.t("sync.never_synced"))
                    vm.githubSyncOffer?.let { offer -> Text("${offer.optString("title")} · ${offer.optInt("chapter")+1}"); TextButton(onClick = vm::applyGithubProgress) { Text(I18n.t("cloud.apply")) } }
                }
            }
            if (vm.githubToken == null) Text(I18n.t("sync.login_required"), modifier = Modifier.padding(horizontal = 16.dp), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Text(I18n.t("sync.contents"), style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Surface(color = MaterialTheme.colorScheme.surface, shape = ReaderGeometry.Shape) {
                Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(I18n.t("sync.content_note"))
                    Text(I18n.t("sync.content_limit"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    TextButton(onClick = { showDetails = !showDetails }) { Text(I18n.t("cloud.details")) }
                    if (showDetails) Text(I18n.t("cloud.limits"), style = MaterialTheme.typography.bodySmall)
                }
            }
        }
    }
}

@Composable
private fun SyncValueRow(label: String, value: String) {
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(16.dp)) {
        Text(label, modifier = Modifier.weight(1f))
        Text(value, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun PermissionDetails(expanded: Boolean, onClick: () -> Unit) {
    TextButton(onClick = onClick, contentPadding = PaddingValues(0.dp)) { Text(I18n.t("sync.permission_details")) }
    if (expanded) Text(I18n.t("sync.scope_detail"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
}
