package com.zhongbai233.epub.reader.ui.library

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Code
import androidx.compose.material.icons.filled.Description
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.Image
import com.zhongbai233.epub.reader.BuildConfig
import com.zhongbai233.epub.reader.R
import com.zhongbai233.epub.reader.i18n.I18n

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AboutScreen(
    onNavigateBack: () -> Unit,
    onExportLogs: () -> String?
) {
    @Suppress("UNUSED_VARIABLE")
    val langVersion = I18n.version
    val uriHandler = LocalUriHandler.current
    var exportStatus by remember { mutableStateOf<String?>(null) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(I18n.t("about.title"), fontWeight = FontWeight.Bold) },
                navigationIcon = {
                    IconButton(onClick = onNavigateBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = I18n.t("about.back"))
                    }
                }
            )
        }
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(padding)
                .padding(horizontal = 24.dp, vertical = 16.dp),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            Spacer(Modifier.height(24.dp))

            // ── App Logo ──
            Image(
                painter = painterResource(R.drawable.app_icon),
                contentDescription = I18n.t("about.app_name"),
                contentScale = ContentScale.Fit,
                modifier = Modifier
                    .size(96.dp)
                    .clip(RoundedCornerShape(22.dp))
            )

            Spacer(Modifier.height(20.dp))

            // ── App Name ──
            Text(
                text = I18n.t("about.app_name"),
                fontSize = 24.sp,
                fontWeight = FontWeight.Bold,
                color = MaterialTheme.colorScheme.onSurface
            )
            Spacer(Modifier.height(6.dp))
            Text(
                text = "v${BuildConfig.APP_VERSION_NAME}",
                fontSize = 14.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
            Spacer(Modifier.height(4.dp))
            Text(
                text = I18n.t("about.author_line"),
                fontSize = 13.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center
            )

            Spacer(Modifier.height(32.dp))
            HorizontalDivider()
            Spacer(Modifier.height(24.dp))

            // ── Buttons ──
            OutlinedButton(
                onClick = {
                    runCatching {
                        uriHandler.openUri("https://github.com/zhongbai2333/RustEpubReader")
                    }
                },
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(10.dp)
            ) {
                Icon(Icons.Default.Code, contentDescription = null, modifier = Modifier.size(18.dp))
                Spacer(Modifier.width(8.dp))
                Text(I18n.t("about.github_repo"), fontSize = 14.sp)
            }

            Spacer(Modifier.height(12.dp))

            OutlinedButton(
                onClick = {
                    val path = onExportLogs()
                    exportStatus = if (path != null) {
                        I18n.tf1("feedback.export_success", path)
                    } else {
                        I18n.t("feedback.export_failed_generic")
                    }
                },
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(10.dp)
            ) {
                Icon(Icons.Default.Description, contentDescription = null, modifier = Modifier.size(18.dp))
                Spacer(Modifier.width(8.dp))
                Text(I18n.t("feedback.export_logs"), fontSize = 14.sp)
            }

            if (!exportStatus.isNullOrBlank()) {
                Spacer(Modifier.height(8.dp))
                Text(
                    exportStatus!!,
                    fontSize = 12.sp,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.fillMaxWidth()
                )
            }

            Spacer(Modifier.height(32.dp))
            HorizontalDivider()
            Spacer(Modifier.height(20.dp))

            // ── Open Source Notices ──
            Text(
                text = I18n.t("about.open_source"),
                fontSize = 15.sp,
                fontWeight = FontWeight.SemiBold,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.fillMaxWidth()
            )
            Spacer(Modifier.height(12.dp))
            OpenSourceEntry(
                name = "pagecurl",
                license = "Apache 2.0",
                url = "https://github.com/oleksandrbalan/pagecurl",
                uriHandler = { uriHandler.openUri(it) }
            )
            Spacer(Modifier.height(8.dp))
            OpenSourceEntry(
                name = "egui / eframe",
                license = "MIT / Apache 2.0",
                url = "https://github.com/emilk/egui",
                uriHandler = { uriHandler.openUri(it) }
            )
            Spacer(Modifier.height(8.dp))
            OpenSourceEntry(
                name = "rbook",
                license = "Apache 2.0",
                url = "https://crates.io/crates/rbook",
                uriHandler = { uriHandler.openUri(it) }
            )
            Spacer(Modifier.height(8.dp))
            OpenSourceEntry(
                name = "Jetpack Compose",
                license = "Apache 2.0",
                url = "https://developer.android.com/jetpack/compose",
                uriHandler = { uriHandler.openUri(it) }
            )
            Spacer(Modifier.height(8.dp))
            OpenSourceEntry(
                name = "kotlinx.serialization",
                license = "Apache 2.0",
                url = "https://github.com/Kotlin/kotlinx.serialization",
                uriHandler = { uriHandler.openUri(it) }
            )
            Spacer(Modifier.height(40.dp))
        }
    }
}

@Composable
private fun OpenSourceEntry(
    name: String,
    license: String,
    url: String,
    uriHandler: (String) -> Unit
) {
    Surface(
        shape = RoundedCornerShape(8.dp),
        color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.4f),
        modifier = Modifier.fillMaxWidth()
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 14.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(name, fontSize = 13.sp, fontWeight = FontWeight.Medium)
                Text(license, fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            TextButton(onClick = { runCatching { uriHandler(url) } }) {
                Text(I18n.t("about.view_license"), fontSize = 12.sp)
            }
        }
    }
}
