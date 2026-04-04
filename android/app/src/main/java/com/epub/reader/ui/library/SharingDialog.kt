package com.zhongbai233.epub.reader.ui.library

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Cloud
import androidx.compose.material.icons.filled.CloudOff
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Sync
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.model.DiscoveredPeer
import com.zhongbai233.epub.reader.model.PairedDevice

@Composable
fun SharingDialog(
    showDialog: Boolean,
    onDismiss: () -> Unit,
    serverRunning: Boolean,
    serverAddr: String,
    pin: String,
    connectAddr: String,
    connectPin: String,
    sharingStatus: String,
    autoStartSharing: Boolean,
    pairedDevices: List<PairedDevice>,
    discoveredPeers: List<DiscoveredPeer>,
    onPinChange: (String) -> Unit,
    onConnectAddrChange: (String) -> Unit,
    onConnectPinChange: (String) -> Unit,
    onAutoStartChange: (Boolean) -> Unit,
    onStartServer: () -> Unit,
    onStopServer: () -> Unit,
    onManualSync: () -> Unit,
    onConnectToPeer: (addr: String, pin: String, deviceId: String) -> Unit,
    onStartDiscovery: () -> Unit,
    onRefreshPeers: () -> Unit,
    onRemovePaired: (deviceId: String) -> Unit = {},
    onOpenGithubFeedback: () -> Unit = {},
) {
    @Suppress("UNUSED_VARIABLE")
    val langVersion = I18n.version

    if (!showDialog) return

    LaunchedEffect(Unit) {
        onStartDiscovery()
    }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(I18n.t("share.title"), fontWeight = FontWeight.Bold) },
        text = {
            LazyColumn(
                modifier = Modifier.fillMaxWidth(),
                verticalArrangement = Arrangement.spacedBy(12.dp)
            ) {
                // ── Auto-start checkbox ──
                item {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Checkbox(
                            checked = autoStartSharing,
                            onCheckedChange = onAutoStartChange
                        )
                        Text(I18n.t("share.auto_start"), fontSize = 13.sp)
                    }
                }

                // ── PIN Input (large, centered, face-to-face style) ──
                item {
                    Column(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalAlignment = Alignment.CenterHorizontally
                    ) {
                        Text(
                            I18n.t("share.pin_label"),
                            fontSize = 13.sp,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                        Spacer(Modifier.height(8.dp))
                        OutlinedTextField(
                            value = pin,
                            onValueChange = { newVal ->
                                val filtered = newVal.filter { it.isDigit() }.take(4)
                                onPinChange(filtered)
                            },
                            enabled = !serverRunning,
                            singleLine = true,
                            textStyle = LocalTextStyle.current.copy(
                                fontSize = 28.sp,
                                fontWeight = FontWeight.Bold,
                                textAlign = TextAlign.Center,
                                letterSpacing = 8.sp
                            ),
                            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                            modifier = Modifier.width(180.dp),
                            shape = RoundedCornerShape(12.dp)
                        )
                    }
                }

                // ── Start / Stop button ──
                item {
                    Column(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalAlignment = Alignment.CenterHorizontally
                    ) {
                        if (serverRunning) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Icon(
                                    Icons.Default.Cloud,
                                    contentDescription = null,
                                    tint = MaterialTheme.colorScheme.primary,
                                    modifier = Modifier.size(18.dp)
                                )
                                Spacer(Modifier.width(6.dp))
                                Text(
                                    I18n.t("share.server_running"),
                                    fontSize = 13.sp,
                                    color = MaterialTheme.colorScheme.primary
                                )
                            }
                            Spacer(Modifier.height(8.dp))
                            OutlinedButton(
                                onClick = onStopServer,
                                modifier = Modifier.fillMaxWidth()
                            ) {
                                Icon(Icons.Default.CloudOff, contentDescription = null, modifier = Modifier.size(16.dp))
                                Spacer(Modifier.width(6.dp))
                                Text(I18n.t("share.stop_server"), fontSize = 14.sp)
                            }
                        } else {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Icon(
                                    Icons.Default.CloudOff,
                                    contentDescription = null,
                                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                                    modifier = Modifier.size(18.dp)
                                )
                                Spacer(Modifier.width(6.dp))
                                Text(
                                    I18n.t("share.server_stopped"),
                                    fontSize = 13.sp,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant
                                )
                            }
                            Spacer(Modifier.height(8.dp))
                            Button(
                                onClick = onStartServer,
                                enabled = pin.length == 4,
                                modifier = Modifier.fillMaxWidth()
                            ) {
                                Icon(Icons.Default.Cloud, contentDescription = null, modifier = Modifier.size(16.dp))
                                Spacer(Modifier.width(6.dp))
                                Text(I18n.t("share.start_server"), fontSize = 14.sp)
                            }
                        }
                    }
                }

                // ── Discovered devices (click to pair) ──
                item {
                    Text(
                        I18n.t("share.discovered_devices"),
                        fontWeight = FontWeight.SemiBold,
                        fontSize = 14.sp
                    )
                }
                val otherPeers = discoveredPeers // show all, not filtered by PIN
                if (otherPeers.isEmpty()) {
                    item {
                        Text(
                            I18n.t("share.no_devices_nearby"),
                            fontSize = 12.sp,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                } else {
                    items(otherPeers) { peer ->
                        var showPairDialog by remember { mutableStateOf(false) }
                        var pairPin by remember { mutableStateOf("") }
                        val isPaired = pairedDevices.any { it.deviceId == peer.deviceId }

                        if (showPairDialog) {
                            AlertDialog(
                                onDismissRequest = { showPairDialog = false; pairPin = "" },
                                title = { Text(I18n.t("share.pair_title"), fontWeight = FontWeight.Bold) },
                                text = {
                                    Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.fillMaxWidth()) {
                                        Text(peer.deviceName, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
                                        Text(peer.addr, fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                        Spacer(Modifier.height(12.dp))
                                        Text(I18n.t("share.pair_enter_pin"), fontSize = 13.sp)
                                        Spacer(Modifier.height(8.dp))
                                        OutlinedTextField(
                                            value = pairPin,
                                            onValueChange = { newVal ->
                                                pairPin = newVal.filter { it.isDigit() }.take(4)
                                            },
                                            singleLine = true,
                                            textStyle = LocalTextStyle.current.copy(
                                                fontSize = 28.sp,
                                                fontWeight = FontWeight.Bold,
                                                textAlign = TextAlign.Center,
                                                letterSpacing = 8.sp
                                            ),
                                            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                                            modifier = Modifier.width(180.dp),
                                            shape = RoundedCornerShape(12.dp)
                                        )
                                    }
                                },
                                confirmButton = {
                                    TextButton(
                                        onClick = {
                                            onConnectToPeer(peer.addr, pairPin, peer.deviceId)
                                            showPairDialog = false
                                            pairPin = ""
                                        },
                                        enabled = pairPin.length == 4
                                    ) {
                                        Text(I18n.t("share.pair_connect"))
                                    }
                                },
                                dismissButton = {
                                    TextButton(onClick = { showPairDialog = false; pairPin = "" }) {
                                        Text(I18n.t("share.cancel"))
                                    }
                                }
                            )
                        }

                        Card(
                            modifier = Modifier.fillMaxWidth(),
                            shape = RoundedCornerShape(10.dp),
                            colors = CardDefaults.cardColors(
                                containerColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.5f)
                            )
                        ) {
                            Row(
                                modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp).fillMaxWidth(),
                                verticalAlignment = Alignment.CenterVertically
                            ) {
                                    Column(modifier = Modifier.weight(1f)) {
                                    Text(peer.deviceName, fontSize = 13.sp, fontWeight = FontWeight.Medium)
                                    Text(peer.addr, fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                    if (isPaired) {
                                        Text(I18n.t("share.paired"), fontSize = 11.sp, color = MaterialTheme.colorScheme.primary)
                                    }
                                }
                                TextButton(onClick = {
                                    if (isPaired) {
                                        // Already paired — connect directly without PIN
                                        onConnectToPeer(peer.addr, "", peer.deviceId)
                                    } else {
                                        showPairDialog = true
                                    }
                                }) {
                                    Text(I18n.t("share.connect"))
                                }
                            }
                        }
                    }
                }

                // ── Status ──
                if (sharingStatus.isNotEmpty()) {
                    item {
                        Text(
                            sharingStatus,
                            fontSize = 12.sp,
                            color = if (sharingStatus.contains("❌")) {
                                MaterialTheme.colorScheme.error
                            } else if (sharingStatus.contains("✅")) {
                                MaterialTheme.colorScheme.primary
                            } else {
                                MaterialTheme.colorScheme.tertiary
                            }
                        )
                    }
                }

                // ── Advanced Options (collapsed) ──
                item {
                    var expanded by remember { mutableStateOf(false) }
                    Column {
                        TextButton(onClick = { expanded = !expanded }) {
                            Text(
                                if (expanded) "▼ ${I18n.t("share.advanced")}" else "▶ ${I18n.t("share.advanced")}",
                                fontSize = 13.sp
                            )
                        }
                        if (expanded) {
                            Column(modifier = Modifier.padding(start = 8.dp)) {
                                if (serverRunning) {
                                    Text(I18n.tf1("share.address", serverAddr), fontSize = 12.sp)
                                    Spacer(Modifier.height(8.dp))
                                }

                                Text(I18n.t("share.connect_to_peer"), fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                                Spacer(Modifier.height(4.dp))
                                OutlinedTextField(
                                    value = connectAddr,
                                    onValueChange = onConnectAddrChange,
                                    label = { Text(I18n.t("share.enter_address"), fontSize = 12.sp) },
                                    singleLine = true,
                                    modifier = Modifier.fillMaxWidth()
                                )
                                Spacer(Modifier.height(4.dp))
                                OutlinedTextField(
                                    value = connectPin,
                                    onValueChange = onConnectPinChange,
                                    label = { Text(I18n.t("share.enter_pin"), fontSize = 12.sp) },
                                    singleLine = true,
                                    modifier = Modifier.fillMaxWidth()
                                )
                                Spacer(Modifier.height(8.dp))
                                OutlinedButton(
                                    onClick = onManualSync,
                                    modifier = Modifier.fillMaxWidth()
                                ) {
                                    Icon(Icons.Default.Sync, contentDescription = null, modifier = Modifier.size(14.dp))
                                    Spacer(Modifier.width(4.dp))
                                    Text(I18n.t("share.manual_sync"), fontSize = 12.sp)
                                }

                                Spacer(Modifier.height(12.dp))
                                Text(I18n.t("share.paired_devices"), fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                                if (pairedDevices.isEmpty()) {
                                    Text(I18n.t("share.no_paired"), fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                } else {
                                    pairedDevices.forEach { device ->
                                        Row(
                                            verticalAlignment = Alignment.CenterVertically,
                                            modifier = Modifier.fillMaxWidth()
                                        ) {
                                            Text(
                                                I18n.tf1("share.device_name", device.deviceName),
                                                fontSize = 12.sp,
                                                modifier = Modifier.weight(1f)
                                            )
                                            IconButton(
                                                onClick = { onRemovePaired(device.deviceId) },
                                                modifier = Modifier.size(28.dp)
                                            ) {
                                                Icon(
                                                    Icons.Default.Delete,
                                                    contentDescription = "Remove",
                                                    modifier = Modifier.size(16.dp),
                                                    tint = MaterialTheme.colorScheme.error
                                                )
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

            }
        },
        confirmButton = {
            TextButton(onClick = onDismiss) {
                Text(I18n.t("share.close"))
            }
        }
    )
}
