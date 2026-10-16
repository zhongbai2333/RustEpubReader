package com.zhongbai233.epub.reader.ui.theme

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.sp

// Shared accent with desktop chrome.
val AccentBlue = Color(0xFF2F64CF)
val AccentBlueLight = Color(0xFF8CB5FF)

// 深色主题
private val DarkColorScheme =
    darkColorScheme(
        primary = AccentBlueLight,
        onPrimary = Color(0xFF122548),
        primaryContainer = Color(0xFF1A3A6B),
        onPrimaryContainer = Color(0xFFD6E3FF),
        secondary = AccentBlueLight,
        background = Color(0xFF12161C),
        onBackground = Color(0xFFE9EDF4),
        surface = Color(0xFF1B2129),
        onSurface = Color(0xFFE9EDF4),
        surfaceVariant = Color(0xFF252D38),
        onSurfaceVariant = Color(0xFFA7B2C2),
        outline = Color(0xFF353F4C),
        outlineVariant = Color(0xFF353F4C),
        secondaryContainer = Color(0xFF253F6A),
        onSecondaryContainer = Color(0xFF8CB5FF),
        surfaceContainerLowest = Color(0xFF12161C),
        surfaceContainerLow = Color(0xFF1B2129),
        surfaceContainer = Color(0xFF1B2129),
        surfaceContainerHigh = Color(0xFF252D38),
        surfaceContainerHighest = Color(0xFF252D38),
    )

// 浅色主题
private val LightColorScheme =
    lightColorScheme(
        primary = AccentBlue,
        onPrimary = Color.White,
        primaryContainer = Color(0xFFD6E3FF),
        onPrimaryContainer = Color(0xFF001A41),
        secondary = AccentBlue,
        background = Color(0xFFF4F6F9),
        onBackground = Color(0xFF1F2937),
        surface = Color.White,
        onSurface = Color(0xFF1F2937),
        surfaceVariant = Color(0xFFEAEFF7),
        onSurfaceVariant = Color(0xFF5E6D81),
        outline = Color(0xFFDBE2EC),
        outlineVariant = Color(0xFFDBE2EC),
        secondaryContainer = Color(0xFFDDE8FC),
        onSecondaryContainer = Color(0xFF2F64CF),
        surfaceContainerLowest = Color.White,
        surfaceContainerLow = Color(0xFFF4F6F9),
        surfaceContainer = Color.White,
        surfaceContainerHigh = Color(0xFFEAEFF7),
        surfaceContainerHighest = Color(0xFFEAEFF7),
    )

@Composable
fun EpubReaderTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    dynamicColor: Boolean = false,
    content: @Composable () -> Unit,
) {
    val context = LocalContext.current
    val dynamic = dynamicColor && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S
    val light = if (dynamic) dynamicLightColorScheme(context) else LightColorScheme
    val dark = if (dynamic) dynamicDarkColorScheme(context) else DarkColorScheme
    val colorScheme = appearanceColorScheme(light, dark, darkTheme)

    MaterialTheme(
        colorScheme = colorScheme,
        typography =
            Typography(
                bodyLarge = TextStyle(fontSize = 16.sp, lineHeight = 24.sp),
                bodyMedium = TextStyle(fontSize = 14.sp, lineHeight = 21.sp),
                bodySmall = TextStyle(fontSize = 12.sp, lineHeight = 18.sp),
            ),
        shapes =
            Shapes(
                extraSmall = ReaderGeometry.Shape,
                small = ReaderGeometry.Shape,
                medium = ReaderGeometry.Shape,
                large = ReaderGeometry.Shape,
                extraLarge = ReaderGeometry.DialogShape,
            ),
        content = content,
    )
}
