package com.zhongbai233.epub.reader.ui.reader

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.os.SystemClock
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.*
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.zhongbai233.epub.reader.model.AmbientReadingSwitch
import kotlinx.coroutines.delay

internal data class AmbientAppearanceSignal(val dark: Boolean?, val available: Boolean)

@Composable
internal fun automaticReadingAppearance(enabled: Boolean): AmbientAppearanceSignal {
    val systemDark = isSystemInDarkTheme()
    val context = LocalContext.current
    val manager = remember(context) { context.getSystemService(Context.SENSOR_SERVICE) as? SensorManager }
    val sensor = remember(manager) { manager?.getDefaultSensor(Sensor.TYPE_LIGHT) }
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    var available by remember(manager, sensor) { mutableStateOf(sensor != null) }
    var active by remember { mutableStateOf(false) }
    var lux by remember { mutableStateOf<Float?>(null) }
    var ambientDark by remember { mutableStateOf<Boolean?>(null) }
    DisposableEffect(enabled, lifecycle, manager, sensor) {
        val listener = object : SensorEventListener {
            override fun onAccuracyChanged(sensor: Sensor?, accuracy: Int) {}
            override fun onSensorChanged(event: SensorEvent) { lux = event.values.firstOrNull() }
        }
        fun update() {
            manager?.unregisterListener(listener); lux = null; active = false; ambientDark = null
            if(enabled && sensor != null && lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) {
                available = manager?.registerListener(listener, sensor, SensorManager.SENSOR_DELAY_NORMAL) == true
                active = available
            }
        }
        val observer = LifecycleEventObserver { _, _ -> update() }
        lifecycle.addObserver(observer); update()
        onDispose { lifecycle.removeObserver(observer); manager?.unregisterListener(listener) }
    }
    LaunchedEffect(enabled, active) {
        ambientDark = null
        val switch = AmbientReadingSwitch(systemDark)
        while(enabled && active) {
            lux?.takeIf { it.isFinite() && it >= 0f }?.let { ambientDark = switch.sample(it, SystemClock.elapsedRealtime()) }
            delay(250)
        }
    }
    return AmbientAppearanceSignal(if(enabled && available && active) ambientDark else null, available)
}
