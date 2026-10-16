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
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.unit.dp
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.ui.reader.GithubAccountContent
import com.zhongbai233.epub.reader.ui.theme.ReaderGeometry
import com.zhongbai233.epub.reader.viewmodel.ReaderViewModel

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SyncScreen(vm: ReaderViewModel, onLibrary: () -> Unit = {}, onBack: (() -> Unit)? = null, onSearch: () -> Unit = {}) {
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
                expandedHeight = if (LocalDensity.current.fontScale > 1.3f) 72.dp else 56.dp,
                colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.background),
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
        bottomBar = { if (onBack == null) HomeNavigation(true, onLibrary, {}, onSearch = onSearch) },
    ) { padding ->
        val layoutDirection = LocalLayoutDirection.current
        // Keep the tab view behind the floating controls, with clearance only
        // at the end of each tab's scrollable content.
        val contentPadding = PaddingValues(
            start = ReaderGeometry.PageInset,
            top = ReaderGeometry.PageInset,
            end = ReaderGeometry.PageInset,
            bottom = ReaderGeometry.PageInset + padding.calculateBottomPadding(),
        )
        Column(Modifier.fillMaxSize().padding(
            start = padding.calculateStartPadding(layoutDirection),
            top = padding.calculateTopPadding(),
            end = padding.calculateEndPadding(layoutDirection),
        )) {
            TabRow(selectedTabIndex = tab) {
                listOf("ui.github_sync", "ui.local_sync").forEachIndexed { index, key ->
                    Tab(selected = tab == index, onClick = { tab = index }, text = { Text(I18n.t(key)) })
                }
            }
            if (tab == 0) Column(Modifier.weight(1f).verticalScroll(githubScroll).padding(contentPadding), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                GithubAccountContent(vm, embedded = true)
            }
            else {
                    SharingContent(
                        listState = nearbyScroll,
                        contentPadding = contentPadding,
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
