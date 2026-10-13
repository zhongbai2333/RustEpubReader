package com.zhongbai233.epub.reader.ui.theme

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.unit.dp

/** Chrome geometry, mirrored by desktop's design::Geometry in logical points. */
object ReaderGeometry {
    val Shape = RoundedCornerShape(12.dp)
    val DialogShape = RoundedCornerShape(16.dp)
    val PageInset = 24.dp
    val CardInset = 16.dp
    val Gap = 8.dp
    val BlockGap = 16.dp
    val SectionGap = 24.dp
    val TouchHeight = 48.dp
    val ButtonPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp)
}
