package com.zhongbai233.epub.reader.ui.reader

import androidx.compose.animation.*
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.*
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.TextToolbar
import androidx.compose.ui.platform.TextToolbarStatus
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.ui.theme.ReaderGeometry

// ─── 选区操作类型 ───

enum class SelectionAction {
    COPY,
    HIGHLIGHT,
    NOTE,
    DICTIONARY,
    TRANSLATE,
    CORRECT,
}

// ─── 自定义 TextToolbar 实现 ───

/**
 * 拦截系统原生的文本选中弹窗 (Copy / Paste / Select All)。 替换为 app 自定义的悬浮菜单，包含：复制、高亮、笔记、词典、翻译、更改为…
 *
 * @param onShowMenu 当系统请求显示菜单时触发（带选区坐标、copy 回调）
 * @param onHideMenu 当系统请求隐藏菜单时触发
 */
class CustomTextToolbar(
    private val onShowMenu: (rect: Rect, onCopy: (() -> Unit)?) -> Unit,
    private val onHideMenu: () -> Unit,
) : TextToolbar {

    private var _status = TextToolbarStatus.Hidden

    override val status: TextToolbarStatus
        get() = _status

    override fun showMenu(
        rect: Rect,
        onCopyRequested: (() -> Unit)?,
        onPasteRequested: (() -> Unit)?,
        onCutRequested: (() -> Unit)?,
        onSelectAllRequested: (() -> Unit)?,
    ) {
        _status = TextToolbarStatus.Shown
        onShowMenu(rect, onCopyRequested)
    }

    override fun hide() {
        _status = TextToolbarStatus.Hidden
        onHideMenu()
    }
}

// ─── 悬浮选区菜单 Compose UI ───

/** 自定义的文本选中悬浮操作栏。 显示在选区上方，提供：复制、高亮、笔记、词典、翻译、更改为… */
@Composable
fun SelectionFloatingMenu(
    visible: Boolean,
    selectionRect: Rect,
    isDarkMode: Boolean,
    onAction: (SelectionAction, String?) -> Unit,
    onDismiss: () -> Unit,
) {
    var showColorPicker by remember(visible) { mutableStateOf(false) }

    AnimatedVisibility(
        visible = visible,
        enter = fadeIn() + scaleIn(initialScale = 0.8f),
        exit = fadeOut() + scaleOut(targetScale = 0.8f),
        modifier = Modifier.fillMaxSize(),
    ) {
        val menuBg = if (isDarkMode) Color(0xFF2C2C30) else Color(0xFFFAFAFA)
        val textClr = if (isDarkMode) Color(0xFFE0E0E0) else Color(0xFF333333)
        val iconClr = if (isDarkMode) Color(0xFFB0B0B0) else Color(0xFF555555)

        val yOffset =
            with(androidx.compose.ui.platform.LocalDensity.current) {
                (selectionRect.top - 76.dp.toPx()).coerceAtLeast(8.dp.toPx())
            }

        Box(
            modifier = Modifier.fillMaxSize(),
            contentAlignment = Alignment.TopCenter,
        ) {
            Box(
                modifier =
                    Modifier.offset { IntOffset(0, yOffset.toInt()) }.padding(horizontal = 24.dp)
            ) {
                Surface(
                    modifier = Modifier.shadow(8.dp, ReaderGeometry.Shape),
                    shape = ReaderGeometry.Shape,
                    color = menuBg,
                    tonalElevation = 4.dp,
                ) {
                    if (showColorPicker) {
                        Row(
                            modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            IconButton(onClick = { showColorPicker = false }) {
                                Icon(
                                    Icons.AutoMirrored.Filled.ArrowBack,
                                    contentDescription = I18n.t("about.back"),
                                    tint = iconClr,
                                )
                            }
                            ColorBubble(color = Color(0xFFFFC107)) {
                                onAction(SelectionAction.HIGHLIGHT, "Yellow")
                            }
                            ColorBubble(color = Color(0xFF66BB6A)) {
                                onAction(SelectionAction.HIGHLIGHT, "Green")
                            }
                            ColorBubble(color = Color(0xFF42A5F5)) {
                                onAction(SelectionAction.HIGHLIGHT, "Blue")
                            }
                            ColorBubble(color = Color(0xFFEF5350)) {
                                onAction(SelectionAction.HIGHLIGHT, "Pink")
                            }
                        }
                    } else {
                        Row(
                            modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
                            horizontalArrangement = Arrangement.spacedBy(2.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            SelectionMenuItem(
                                icon = Icons.Default.ContentCopy,
                                label = I18n.t("selection.copy"),
                                tint = iconClr,
                                textColor = textClr,
                                onClick = { onAction(SelectionAction.COPY, null) },
                            )
                            SelectionMenuItem(
                                icon = Icons.Default.Highlight,
                                label = I18n.t("selection.highlight"),
                                tint = Color(0xFFFFC107),
                                textColor = textClr,
                                onClick = { showColorPicker = true },
                            )
                            SelectionMenuItem(
                                icon = Icons.AutoMirrored.Filled.StickyNote2,
                                label = I18n.t("selection.note"),
                                tint = Color(0xFF66BB6A),
                                textColor = textClr,
                                onClick = { onAction(SelectionAction.NOTE, null) },
                            )
                            SelectionMenuItem(
                                icon = Icons.AutoMirrored.Filled.MenuBook,
                                label = I18n.t("selection.dictionary"),
                                tint = iconClr,
                                textColor = textClr,
                                onClick = { onAction(SelectionAction.DICTIONARY, null) },
                            )
                            SelectionMenuItem(
                                icon = Icons.Default.Translate,
                                label = I18n.t("selection.translate"),
                                tint = Color(0xFF42A5F5),
                                textColor = textClr,
                                onClick = { onAction(SelectionAction.TRANSLATE, null) },
                            )
                            SelectionMenuItem(
                                icon = Icons.Default.Edit,
                                label = I18n.t("selection.correct"),
                                tint = Color(0xFFEF5350),
                                textColor = textClr,
                                onClick = { onAction(SelectionAction.CORRECT, null) },
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun SelectionMenuItem(
    icon: ImageVector,
    label: String,
    tint: Color,
    textColor: Color,
    onClick: () -> Unit,
) {
    Column(
        modifier =
            Modifier.clickable(onClick = onClick).padding(horizontal = 8.dp, vertical = 6.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Icon(
            imageVector = icon,
            contentDescription = label,
            tint = tint,
            modifier = Modifier.size(20.dp),
        )
        Spacer(modifier = Modifier.height(2.dp))
        Text(
            text = label,
            fontSize = 10.sp,
            color = textColor,
            maxLines = 1,
        )
    }
}

@Composable
private fun ColorBubble(color: Color, onClick: () -> Unit) {
    Box(
        modifier =
            Modifier.size(32.dp)
                .background(color, androidx.compose.foundation.shape.CircleShape)
                .clickable(onClick = onClick)
    )
}
