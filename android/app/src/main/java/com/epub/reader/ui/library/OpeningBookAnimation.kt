package com.zhongbai233.epub.reader.ui.library
import androidx.compose.animation.core.*
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import android.provider.Settings

@Composable
fun OpeningBookAnimation() {
    val context = LocalContext.current
    val reduced = remember { Settings.Global.getFloat(context.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f }
    val transition = rememberInfiniteTransition(label = "book-opening")
    val fold by transition.animateFloat(-8f, -55f, infiniteRepeatable(tween(1200), RepeatMode.Reverse), label = "cover")
    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(20.dp)) {
        Box(Modifier.size(130.dp, 160.dp), contentAlignment = Alignment.Center) {
            Box(Modifier.size(108.dp, 156.dp).background(MaterialTheme.colorScheme.surfaceVariant, RoundedCornerShape(6.dp))) {
                Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) { repeat(8) { Box(Modifier.fillMaxWidth().height(1.dp).background(MaterialTheme.colorScheme.outlineVariant)) } }
            }
            Box(Modifier.size(108.dp, 156.dp).graphicsLayer { rotationY = if (reduced) 0f else fold; transformOrigin = TransformOrigin(0f, 0.5f); cameraDistance = 12*density }.background(MaterialTheme.colorScheme.primary, RoundedCornerShape(6.dp)))
        }
        CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
    }
}
