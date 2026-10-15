package com.zhongbai233.epub.reader.ui.library

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Bundle
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.google.zxing.BarcodeFormat
import com.journeyapps.barcodescanner.*
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.ui.theme.EpubReaderTheme

/** Camera preview only: no image capture, file output or screen recording. */
class BookScanActivity : ComponentActivity() {
    companion object {
        const val CONTENTS = "reader.share.qr"
        const val PICK_IMAGE = "reader.share.image"
    }
    private var completed = false
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge(
            statusBarStyle = androidx.activity.SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
            navigationBarStyle = androidx.activity.SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
        )
        setContent { EpubReaderTheme(darkTheme = true) {
            BookScanner(
                back = { finish() },
                chooseImage = { complete(Intent().putExtra(PICK_IMAGE, true)) },
                decoded = { complete(Intent().putExtra(CONTENTS, it)) },
            )
        } }
    }
    private fun complete(data: Intent) {
        if (completed) return
        completed = true; setResult(Activity.RESULT_OK, data); finish()
    }
}

@Composable
private fun BookScanner(back: () -> Unit, chooseImage: () -> Unit, decoded: (String) -> Unit) {
    val context = LocalContext.current
    val activity = context as Activity
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    var granted by remember { mutableStateOf(ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) }
    var requested by rememberSaveable { mutableStateOf(false) }
    var cameraError by remember { mutableStateOf(false) }
    var invalid by remember { mutableStateOf(false) }
    var torch by rememberSaveable { mutableStateOf(false) }
    var preview by remember { mutableStateOf<BarcodeView?>(null) }
    var frame by remember { mutableStateOf<Rect?>(null) }
    val decodedNow by rememberUpdatedState(decoded)
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted = it; requested = true }
    LaunchedEffect(Unit) { if (!granted && !requested) { requested = true; permission.launch(Manifest.permission.CAMERA) } }
    DisposableEffect(preview, lifecycle, granted) {
        val camera = preview
        fun resume() {
            if (granted) runCatching { camera?.resume(); camera?.setTorch(torch) }.onFailure { cameraError = true }
        }
        val observer = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_RESUME) {
                granted = ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED
                resume()
            }
            if (event == Lifecycle.Event.ON_PAUSE) { camera?.setTorch(false); camera?.pause() }
        }
        lifecycle.addObserver(observer)
        if (lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) resume()
        onDispose { lifecycle.removeObserver(observer); camera?.setTorch(false); camera?.pause() }
    }
    LaunchedEffect(torch, preview) { preview?.setTorch(torch) }
    Column(Modifier.fillMaxSize().background(Color.Black).safeDrawingPadding()) {
        Row(Modifier.fillMaxWidth().heightIn(min = 56.dp).padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(back) { Icon(Icons.AutoMirrored.Outlined.ArrowBack, I18n.t("settings.back"), tint = Color.White) }
            Text(I18n.t("transfer.scan"), style = MaterialTheme.typography.titleMedium, color = Color.White,
                modifier = Modifier.weight(1f), textAlign = TextAlign.Center)
            Spacer(Modifier.width(48.dp))
        }
        BoxWithConstraints(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
            val density = LocalDensity.current
            val side = with(density) { (minOf(maxWidth, maxHeight) * 0.78f).toPx().toInt().coerceAtLeast(1) }
            if (granted && !cameraError) {
                AndroidView(factory = { viewContext ->
                    BarcodeView(viewContext).apply {
                        setUseTextureView(true)
                        decoderFactory = DefaultDecoderFactory(listOf(BarcodeFormat.QR_CODE))
                        framingRectSize = com.journeyapps.barcodescanner.Size(side, side)
                        addStateListener(object : CameraPreview.StateListener {
                            override fun previewSized() {
                                framingRect?.let { r -> frame = Rect(r.left.toFloat(), r.top.toFloat(), r.right.toFloat(), r.bottom.toFloat()) }
                            }
                            override fun previewStarted() = Unit
                            override fun previewStopped() = Unit
                            override fun cameraClosed() = Unit
                            override fun cameraError(error: Exception) { activity.runOnUiThread { cameraError = true; torch = false } }
                        })
                        decodeContinuous(object : BarcodeCallback {
                            override fun barcodeResult(result: BarcodeResult) {
                                val value = result.text?.trim().orEmpty()
                                if (!isBookShareQr(value)) { invalid = true; return }
                                stopDecoding(); setTorch(false); pause(); decodedNow(value)
                            }
                        })
                        preview = this
                    }
                }, update = { view ->
                    if (view.framingRectSize?.width != side) { view.framingRectSize = com.journeyapps.barcodescanner.Size(side, side); view.requestLayout() }
                }, modifier = Modifier.fillMaxSize())
                Canvas(Modifier.fillMaxSize()) {
                    val r = frame ?: Rect((size.width-side)/2, (size.height-side)/2, (size.width+side)/2, (size.height+side)/2)
                    val shade = Path().apply { fillType = PathFillType.EvenOdd; addRect(Rect(Offset.Zero, size)); addRect(r) }
                    drawPath(shade, Color.Black.copy(alpha = 0.52f))
                    val length = minOf(24.dp.toPx(), r.width / 5)
                    val stroke = 3.dp.toPx()
                    for ((corner, horizontal, vertical) in listOf(Triple(r.topLeft, 1, 1), Triple(r.topRight, -1, 1), Triple(r.bottomLeft, 1, -1), Triple(r.bottomRight, -1, -1))) {
                        drawLine(Color.White, corner, corner + Offset(horizontal * length, 0f), stroke, StrokeCap.Round)
                        drawLine(Color.White, corner, corner + Offset(0f, vertical * length), stroke, StrokeCap.Round)
                    }
                }
            } else {
                Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(32.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    Icon(if (cameraError) Icons.Outlined.NoPhotography else Icons.Outlined.CameraAlt, null, Modifier.size(40.dp), tint = Color.White.copy(alpha = 0.7f))
                    Text(I18n.t(if (cameraError) "scan.camera_error" else "scan.permission_title"), color = Color.White, style = MaterialTheme.typography.titleMedium)
                    Text(I18n.t(if (cameraError) "scan.camera_error_hint" else "scan.permission_hint"), color = Color.White.copy(alpha = 0.72f), textAlign = TextAlign.Center)
                    if (!cameraError) {
                        val settings = requested && !activity.shouldShowRequestPermissionRationale(Manifest.permission.CAMERA)
                        FilledTonalButton(onClick = {
                            if (settings) context.startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, android.net.Uri.parse("package:${context.packageName}")))
                            else permission.launch(Manifest.permission.CAMERA)
                        }) { Text(I18n.t(if (settings) "scan.settings" else "scan.allow")) }
                    }
                }
            }
        }
        if (granted && !cameraError) Text(I18n.t(if (invalid) "scan.invalid" else "scan.guide"), color = Color.White.copy(alpha = 0.8f),
            style = MaterialTheme.typography.bodyMedium, textAlign = TextAlign.Center,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 16.dp).semantics { liveRegion = LiveRegionMode.Polite })
        Row(Modifier.fillMaxWidth().padding(horizontal = 24.dp).padding(bottom = 20.dp), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
            if (context.packageManager.hasSystemFeature(PackageManager.FEATURE_CAMERA_FLASH)) {
                FilledTonalIconButton(onClick = { torch = !torch }, enabled = granted && !cameraError, modifier = Modifier.size(48.dp)) {
                    Icon(if (torch) Icons.Outlined.FlashlightOn else Icons.Outlined.FlashlightOff, I18n.t(if (torch) "scan.torch_off" else "scan.torch_on"))
                }
            }
            OutlinedButton(chooseImage, Modifier.weight(1f).heightIn(min = 48.dp), colors = ButtonDefaults.outlinedButtonColors(contentColor = Color.White)) {
                Icon(Icons.Outlined.Image, null, Modifier.size(20.dp)); Spacer(Modifier.width(8.dp)); Text(I18n.t("transfer.image"))
            }
        }
    }
}

// Only filter the URI envelope here. Core still validates ticket, expiry and key.
internal fun isBookShareQr(value: String): Boolean = value.startsWith("rustepubreader://share/v1/") && value.length in 30..16384
