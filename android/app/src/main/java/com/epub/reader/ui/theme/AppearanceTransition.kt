package com.zhongbai233.epub.reader.ui.theme

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.tween
import androidx.compose.material3.ColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp

internal const val APPEARANCE_TRANSITION_MILLIS = 600
private val AppearanceEasing = CubicBezierEasing(0.42f, 0f, 0.58f, 1f)

/** Animate actual colors, including separate dialog windows. Retaining a display
 * list does not freeze its mutable child layers and cannot cover dialog surfaces. */
@Composable
internal fun appearanceColor(target: Color, label: String): Color = animateColorAsState(
    targetValue = target,
    animationSpec = tween(APPEARANCE_TRANSITION_MILLIS, easing = AppearanceEasing),
    label = label,
).value

/** One clock keeps every Material role in step, also when a transition reverses. */
@Composable
internal fun appearanceColorScheme(light: ColorScheme, dark: ColorScheme, night: Boolean): ColorScheme {
    val fraction by animateFloatAsState(
        if (night) 1f else 0f,
        tween(APPEARANCE_TRANSITION_MILLIS, easing = AppearanceEasing),
        label = "application appearance",
    )
    return light.copy(
        primary = lerp(light.primary, dark.primary, fraction),
        onPrimary = lerp(light.onPrimary, dark.onPrimary, fraction),
        primaryContainer = lerp(light.primaryContainer, dark.primaryContainer, fraction),
        onPrimaryContainer = lerp(light.onPrimaryContainer, dark.onPrimaryContainer, fraction),
        inversePrimary = lerp(light.inversePrimary, dark.inversePrimary, fraction),
        secondary = lerp(light.secondary, dark.secondary, fraction),
        onSecondary = lerp(light.onSecondary, dark.onSecondary, fraction),
        secondaryContainer = lerp(light.secondaryContainer, dark.secondaryContainer, fraction),
        onSecondaryContainer = lerp(light.onSecondaryContainer, dark.onSecondaryContainer, fraction),
        tertiary = lerp(light.tertiary, dark.tertiary, fraction),
        onTertiary = lerp(light.onTertiary, dark.onTertiary, fraction),
        tertiaryContainer = lerp(light.tertiaryContainer, dark.tertiaryContainer, fraction),
        onTertiaryContainer = lerp(light.onTertiaryContainer, dark.onTertiaryContainer, fraction),
        background = lerp(light.background, dark.background, fraction),
        onBackground = lerp(light.onBackground, dark.onBackground, fraction),
        surface = lerp(light.surface, dark.surface, fraction),
        onSurface = lerp(light.onSurface, dark.onSurface, fraction),
        surfaceVariant = lerp(light.surfaceVariant, dark.surfaceVariant, fraction),
        onSurfaceVariant = lerp(light.onSurfaceVariant, dark.onSurfaceVariant, fraction),
        surfaceTint = lerp(light.surfaceTint, dark.surfaceTint, fraction),
        inverseSurface = lerp(light.inverseSurface, dark.inverseSurface, fraction),
        inverseOnSurface = lerp(light.inverseOnSurface, dark.inverseOnSurface, fraction),
        error = lerp(light.error, dark.error, fraction),
        onError = lerp(light.onError, dark.onError, fraction),
        errorContainer = lerp(light.errorContainer, dark.errorContainer, fraction),
        onErrorContainer = lerp(light.onErrorContainer, dark.onErrorContainer, fraction),
        outline = lerp(light.outline, dark.outline, fraction),
        outlineVariant = lerp(light.outlineVariant, dark.outlineVariant, fraction),
        scrim = lerp(light.scrim, dark.scrim, fraction),
        surfaceBright = lerp(light.surfaceBright, dark.surfaceBright, fraction),
        surfaceDim = lerp(light.surfaceDim, dark.surfaceDim, fraction),
        surfaceContainer = lerp(light.surfaceContainer, dark.surfaceContainer, fraction),
        surfaceContainerHigh = lerp(light.surfaceContainerHigh, dark.surfaceContainerHigh, fraction),
        surfaceContainerHighest = lerp(light.surfaceContainerHighest, dark.surfaceContainerHighest, fraction),
        surfaceContainerLow = lerp(light.surfaceContainerLow, dark.surfaceContainerLow, fraction),
        surfaceContainerLowest = lerp(light.surfaceContainerLowest, dark.surfaceContainerLowest, fraction),
    )
}
