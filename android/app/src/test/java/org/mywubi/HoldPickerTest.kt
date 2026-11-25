package org.mywubi

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import org.junit.Assert.*
import org.junit.Test

class HoldPickerTest {
    @Test fun choicesComeFromActionsAndMappingEvenWhenTheLabelIsDecorative() {
        val q = LayoutAction("Character", "q")
        val w = LayoutAction("Character", "w")
        val question = LayoutAction("InputSymbol", "？")
        val key = LayoutKey("pair", "自定义标签", 0f, 0f, 1f, action = LayoutAction("Group", "g0"),
            groupCharacters = listOf("q", "w"), directions = mapOf(0 to q, 1 to q, 4 to w))
        assertEquals(listOf(question, q, w), key.holdOptions("？"))
        assertEquals(listOf(q, w), key.holdOptions(null))
        val punctuation = LayoutAction("Character", "!")
        assertEquals(listOf(punctuation, q, w), key.copy(directions = mapOf(2 to punctuation)).holdOptions(null))
        assertEquals(listOf(w, q), key.copy(hold = w).holdOptions(null))
        assertEquals(listOf(LayoutAction("InputSymbol", "1"), q),
            BuiltinKeyboardLayouts.letters().keys.first().holdOptions("1"))
    }

    @Test fun everyVisibleCellIsReachableAtEitherEdgeAndInWrappedGroups() {
        val keyboard = Rect(8f, 10f, 368f, 300f)
        for (count in listOf(2, 3, 4, 9, 32)) for (x in listOf(12f, 46f, 170f, 355f)) {
            for (y in listOf(66f, 250f)) {
                val press = Offset(x, y)
                val key = Rect(x - 4, y - 23, x + 4, y + 23)
                val labels = List(count) { it.toString() }
                val picker = HoldPicker.place(labels, keyboard, key, press, 36f, 44f, 4f)
                assertTrue(keyboard.contains(picker.bounds.topLeft))
                assertTrue(picker.bounds.right <= keyboard.right && picker.bounds.bottom <= keyboard.bottom)
                assertEquals(0, picker.option(requireNotNull(picker.hit(press))))
                for (slot in labels.indices) {
                    val cell = Offset(picker.bounds.left + (slot % picker.columns + 0.5f) * picker.cellWidth,
                        picker.bounds.top + (picker.row(slot) + 0.5f) * picker.cellHeight)
                    assertEquals(slot, picker.hit(cell))
                }
                assertNull(picker.hit(Offset(-500f, y)))
            }
        }
    }
}
