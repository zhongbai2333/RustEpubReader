package com.zhongbai233.epub.reader.ui.reader

import com.zhongbai233.epub.reader.model.*
import org.junit.Assert.*
import org.junit.Test

class ReadingAppearanceTest {
    @Test fun migrationRetainsLegacyColorsAndImages() {
        val value = ReadingAppearance.migrate(5, 0xFF151515.toInt(), 5, 0xFFEDEDED.toInt(), "content://old", .42f)
        assertEquals(ReadingDisplayMode.dark, value.mode)
        assertEquals("custom", value.selected)
        assertEquals(0xFF151515.toInt(), value.style.dark.paper)
        assertEquals(0xFFEDEDED.toInt(), value.style.dark.ink)
        assertEquals("content://old", value.style.light.image)
        assertEquals(value, ReadingAppearance.decode(value.encoded()))
    }
    @Test fun copiedImageAndEditedVariantsRemainIndependent() {
        val preset = ReadingStyle.presets.first()
        var draft = preset.copy(light = preset.light.copy(image = "first"))
        draft = draft.copy(dark = draft.dark.copy(image = draft.light.image, strength = .65f))
        draft = draft.copy(light = draft.light.copy(image = "second"))
        val value = ReadingAppearance().saved(draft)
        assertEquals("first", value.style.dark.image)
        assertEquals(.65f, value.style.dark.strength)
        assertEquals(.18f, value.style.light.strength)
        assertNull(preset.light.image)
        assertEquals(preset, ReadingStyle.presets.first())
    }
    @Test fun sixChoicesAndIndependentPresetSlotsSurviveRoundTrip() {
        val initial = ReadingAppearance(selected = "green")
        assertEquals(listOf("paper", "gray", "green", "light", "warm", "custom"), initial.choices.map { it.id })
        assertEquals(ReadingStyle.customTemplate, initial.choices.last())
        assertNull(initial.choices.last().light.image)
        val personal = ReadingStyle.customTemplate.copy(light = ReadingStyle.customTemplate.light.copy(paper = 0xFF112233.toInt(), image = "shared"))
        val withCustom = initial.saved(personal)
        val paper = ReadingStyle.presets[0].copy(light = ReadingStyle.presets[0].light.copy(paper = 0xFFABCDEF.toInt(), image = "shared"))
        val gray = ReadingStyle.presets[1].copy(dark = ReadingStyle.presets[1].dark.copy(image = "gray-only"))
        val saved = withCustom.saved(paper).saved(gray)
        assertEquals("gray", saved.selected)
        assertEquals(personal, saved.custom)
        assertEquals(paper, saved.copy(selected = "paper").style)
        assertEquals(ReadingStyle.presets[2], saved.copy(selected = "green").style)
        assertEquals(setOf("shared", "gray-only"), saved.imageReferences)
        assertEquals(saved, ReadingAppearance.decode(saved.encoded()))
        val restored = saved.saved(ReadingStyle.presets[0])
        assertEquals(personal, restored.custom)
        assertEquals(gray, restored.copy(selected = "gray").style)
        assertTrue(restored.imageReferences.contains("shared"))
        // Restoring/removing one reference never discards another theme's shared image.
        assertEquals(setOf("shared", "gray-only"), restored.imageReferences)
        assertEquals(ReadingStyle.presets[0], restored.style)
    }
    @Test fun oldConfigurationKeepsCustomAndMissingOverridesDefaultsToEmpty() {
        val old = ReadingAppearance().saved(ReadingStyle.customTemplate.copy(light = ReadingStyle.customTemplate.light.copy(image = "old-image")))
        val oldJson = old.encoded().replace(",\"overrides\":{}", "")
        val decoded = requireNotNull(ReadingAppearance.decode(oldJson))
        assertEquals(old.custom, decoded.custom)
        assertEquals("custom", decoded.selected)
        assertTrue(decoded.overrides.isEmpty())
        assertEquals(6, decoded.choices.size)
        assertEquals("paper", ReadingAppearance.decode("{\"selected\":\"unknown\"}")?.selected)
        assertEquals("paper", ReadingAppearance.decode("{\"selected\":\"custom\"}")?.selected)
        val invalid = ReadingAppearance(overrides = mapOf("green" to ReadingStyle.presets[0].copy(light = ReadingStyle.presets[0].light.copy(strength = Float.NaN)), "unknown" to ReadingStyle.customTemplate)).normalized()
        assertEquals(setOf("green"), invalid.overrides.keys)
        assertEquals("green", invalid.overrides["green"]?.id)
        assertEquals(.18f, invalid.overrides["green"]!!.light.strength)
    }
    @Test fun ambientSwitchRequiresDwellAndHysteresis() {
        val sensor = AmbientReadingSwitch(false)
        assertFalse(sensor.sample(5f, 0)); assertFalse(sensor.sample(5f, 2999)); assertTrue(sensor.sample(5f, 3000))
        assertTrue(sensor.sample(30f, 6000)); assertTrue(sensor.sample(50f, 7000))
        assertTrue(sensor.sample(30f, 9000)); assertTrue(sensor.sample(50f, 10000))
        assertFalse(sensor.sample(50f, 13000)); assertFalse(sensor.sample(Float.NaN, 14000))
    }
    @Test fun manualModesOverrideAutomaticAndInvalidDataFallsBack() {
        assertFalse(ReadingDisplayMode.light.isDark(true)); assertTrue(ReadingDisplayMode.dark.isDark(false))
        assertTrue(ReadingDisplayMode.automatic.isDark(true)); assertNull(ReadingAppearance.decode("invalid"))
    }
}
