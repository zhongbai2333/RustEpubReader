package com.zhongbai233.epub.reader.ui.library

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.MenuBook
import androidx.compose.material.icons.filled.Sync
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.ui.reader.GithubAccountContent
import com.zhongbai233.epub.reader.ui.theme.ReaderGeometry
import com.zhongbai233.epub.reader.viewmodel.ReaderViewModel

@Composable
fun HomeNavigation(sync: Boolean, onLibrary: () -> Unit, onSync: () -> Unit) {
    NavigationBar(containerColor = MaterialTheme.colorScheme.surface) {
        NavigationBarItem(
            selected = !sync,
            onClick = onLibrary,
            icon = { Icon(Icons.AutoMirrored.Filled.MenuBook, null) },
            label = { Text(I18n.t("library.title")) },
        )
        NavigationBarItem(
            selected = sync,
            onClick = onSync,
            icon = { Icon(Icons.Default.Sync, null) },
            label = { Text(I18n.t("sync.multiplatform")) },
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SyncScreen(vm: ReaderViewModel, onLibrary: () -> Unit = {}, onBack: (() -> Unit)? = null) {
    var tab by rememberSaveable { mutableStateOf(0) }
    val uriHandler = LocalUriHandler.current
    val githubScroll = rememberScrollState()
    val nearbyScroll = androidx.compose.foundation.lazy.rememberLazyListState()
    LaunchedEffect(tab) {
        if (tab == 1) {
            if (!vm.sharingServerRunning && vm.sharingPin.isEmpty()) vm.generatePin()
            vm.loadPairedDevices()
        }
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(I18n.t("sync.multiplatform")) },
                navigationIcon = {
                    onBack?.let {
                        IconButton(onClick = it) {
                            Icon(Icons.AutoMirrored.Filled.ArrowBack, I18n.t("settings.close"))
                        }
                    }
                },
            )
        },
        bottomBar = { if (onBack == null) HomeNavigation(true, onLibrary, {}) },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            TabRow(selectedTabIndex = tab) {
                listOf("ui.github_sync", "ui.local_sync").forEachIndexed { index, key ->
                    Tab(selected = tab == index, onClick = { tab = index }, text = { Text(I18n.t(key)) })
                }
            }
            if (tab == 0) Column(Modifier.weight(1f).verticalScroll(githubScroll).padding(ReaderGeometry.PageInset), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                GithubAccountContent(vm, embedded = true)
            }
            else {
                    SharingContent(
                        listState = nearbyScroll,
                        serverRunning = vm.sharingServerRunning,
                        serverAddr = vm.sharingServerAddr,
                        pin = vm.sharingPin,
                        connectAddr = vm.connectAddrInput,
                        connectPin = vm.connectPinInput,
                        sharingStatus = vm.sharingStatus,
                        autoStartSharing = vm.autoStartSharing,
                        pairedDevices = vm.pairedDevices,
                        discoveredPeers = vm.discoveredPeers,
                        onPinChange = { vm.sharingPin = it },
                        onConnectAddrChange = { vm.connectAddrInput = it },
                        onConnectPinChange = { vm.connectPinInput = it },
                        onAutoStartChange = { vm.updateAutoStartSharing(it) },
                        onStartServer = { vm.startSharingServer() },
                        onStopServer = { vm.stopSharingServer() },
                        onManualSync = { vm.manualSync() },
                        onConnectToPeer = { addr, pin, id -> vm.connectToPeer(addr, pin, id) },
                        onStartDiscovery = { vm.startDiscovery() },
                        onRefreshPeers = { vm.refreshDiscoveredPeers() },
                        onRemovePaired = { vm.removePairedDevice(it) },
                        onOpenGithubFeedback = {
                            uriHandler.openUri(
                                "https://github.com/zhongbai2333/RustEpubReader/issues/new/choose"
                            )
                        },
                    )
            }
        }
    }
}
