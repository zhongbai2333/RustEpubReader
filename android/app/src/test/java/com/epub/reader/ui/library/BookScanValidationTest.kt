package com.zhongbai233.epub.reader.ui.library

import org.junit.Assert.*
import org.junit.Test

class BookScanValidationTest {
    @Test fun unrelatedCodesAndUnboundedInputStayInTheCamera() {
        assertFalse(isBookShareQr("https://example.com"))
        assertFalse(isBookShareQr("javascript:alert(1)"))
        assertFalse(isBookShareQr("rustepubreader://share/v1/"))
        assertFalse(isBookShareQr("rustepubreader://share/v1/" + "x".repeat(16384)))
        assertTrue(isBookShareQr("rustepubreader://share/v1/" + "x".repeat(100)))
    }
}
