package com.zhongbai233.epub.reader.util

import org.junit.Assert.*
import org.junit.Test

class ReleaseVersionTest {
    @Test fun stableVersionsOnlyMoveForward() {
        assertFalse(isNewerRelease("v1.0.8", "2.0.0"))
        assertFalse(isNewerRelease("v2.0.0", "2.0.0"))
        assertTrue(isNewerRelease("v2.0.1", "2.0.0"))
        assertTrue(isNewerRelease("v2.10.0", "2.9.0"))
        assertTrue(isNewerRelease("v2.0.0", "1.99.99"))
        for (tag in listOf("", "v2.0", "v2.0.1-beta.1", "v02.0.1", "v2.0.1/other", "v999999999999999999999.0.0")) {
            assertFalse(tag, isNewerRelease(tag, "2.0.0"))
        }
    }

    @Test fun apkSelectionUsesDeviceAbiInsteadOfReleaseOrdering() {
        val assets = listOf("x86_64", "armeabi-v7a", "arm64-v8a").map {
            "RustEpubReader-Android-$it-v2.0.0.apk" to "https://example.invalid/$it.apk"
        }
        assertEquals("https://example.invalid/arm64-v8a.apk", matchingApk(assets, listOf("arm64-v8a", "armeabi-v7a")))
        assertEquals("https://example.invalid/x86_64.apk", matchingApk(assets, listOf("x86_64")))
        assertNull(matchingApk(assets, listOf("unsupported")))
        assertNull(matchingApk(listOf("Other-Android-arm64-v8a.apk" to "https://example.invalid/other.apk"), listOf("arm64-v8a")))
    }
}
