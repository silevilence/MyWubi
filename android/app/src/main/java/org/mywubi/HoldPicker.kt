package org.mywubi

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.*
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.abs
import kotlin.math.roundToInt

/** Drawing and hit testing share root coordinates, including when an edge shifts the strip. */
internal class HoldPicker private constructor(
    val labels: List<String>, val bounds: Rect, val columns: Int, val rows: Int,
    val defaultSlot: Int, private val above: Boolean, private val anchor: Offset,
    private val key: Rect, private val slop: Float
) {
    val cellWidth get() = bounds.width / columns
    val cellHeight get() = bounds.height / rows
    fun row(slot: Int) = if (above) rows - 1 - slot / columns else slot / columns
    fun option(slot: Int) = when {
        slot == defaultSlot -> 0
        slot < defaultSlot -> slot + 1
        else -> slot
    }

    fun hit(point: Offset): Int? {
        if (point.x !in (bounds.left - cellWidth)..(bounds.right + cellWidth) ||
            point.y !in (minOf(key.top, bounds.top) - cellHeight)..(maxOf(key.bottom, bounds.bottom) + cellHeight)) return null
        val column = if (abs(point.x - anchor.x) < slop) defaultSlot
            else ((point.x - bounds.left) / cellWidth).toInt().coerceIn(0, columns - 1)
        val visualRow = ((point.y - bounds.top) / cellHeight).toInt().coerceIn(0, rows - 1)
        val index = (if (above) rows - 1 - visualRow else visualRow) * columns + column
        return index.takeIf { it in labels.indices }
    }

    companion object {
        fun place(labels: List<String>, keyboard: Rect, key: Rect, anchor: Offset,
                  preferredWidth: Float, preferredHeight: Float, slop: Float): HoldPicker {
            val columns = (keyboard.width / preferredWidth).toInt().coerceIn(1, labels.size)
            val rows = (labels.size + columns - 1) / columns
            val cellWidth = minOf(preferredWidth, keyboard.width / columns)
            val width = cellWidth * columns
            val left = (anchor.x - ((columns - 1) / 2 + 0.5f) * cellWidth)
                .coerceIn(keyboard.left, maxOf(keyboard.left, keyboard.right - width))
            val defaultSlot = ((anchor.x - left) / cellWidth).toInt().coerceIn(0, columns - 1)
            val roomAbove = key.top - keyboard.top
            val roomBelow = keyboard.bottom - key.bottom
            val above = roomAbove >= preferredHeight * rows || roomAbove >= roomBelow
            val height = minOf(preferredHeight * rows, (if (above) roomAbove else roomBelow).coerceAtLeast(1f))
            val top = if (above) key.top - height else key.bottom
            return HoldPicker(labels, Rect(left, top, left + width, top + height), columns, rows,
                defaultSlot, above, anchor, key, slop)
        }
    }
}

@Composable
internal fun HoldPickerBubble(picker: HoldPicker, selectedSlot: Int?, origin: Offset) {
    val colors = MaterialTheme.colorScheme
    val density = LocalDensity.current
    val shape = RoundedCornerShape(skinNumber("key_radius", 8f).dp)
    val box = picker.bounds
    Box(Modifier.absoluteOffset { IntOffset((box.left - origin.x).roundToInt(), (box.top - origin.y).roundToInt()) }
        .size(with(density) { box.width.toDp() }, with(density) { box.height.toDp() })
        .shadow(4.dp, shape).background(colors.surfaceContainerLowest, shape)
        .border(1.dp, colors.outline, shape).semantics { contentDescription = "长按选择" }) {
        val width = with(density) { picker.cellWidth.toDp() }
        val height = with(density) { picker.cellHeight.toDp() }
        val fontSize = with(density) { minOf(20.sp.toPx(), picker.cellWidth * 0.8f, picker.cellHeight * 0.65f).toSp() }
        for (slot in picker.labels.indices) {
            val chosen = slot == selectedSlot
            Box(Modifier.absoluteOffset(x = width * (slot % picker.columns), y = height * picker.row(slot))
                .size(width, height).padding(2.dp)
                .then(if (chosen) Modifier.background(colors.primary, shape) else Modifier), contentAlignment = Alignment.Center) {
                Text(picker.labels[picker.option(slot)], modifier = Modifier.semantics { selected = chosen },
                    color = if (chosen) colors.onPrimary else colors.onSurface,
                    fontSize = fontSize, lineHeight = fontSize, maxLines = 1)
            }
        }
    }
}
