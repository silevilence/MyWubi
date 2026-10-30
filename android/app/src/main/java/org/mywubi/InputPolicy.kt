package org.mywubi

import android.text.InputType
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.icu.text.BreakIterator
import java.util.Locale

data class InputPolicy(val layout: String, val purpose: String, val action: String, val actionId: Int) {
    companion object {
        fun from(inputType: Int, imeOptions: Int): InputPolicy {
            val type = inputType and InputType.TYPE_MASK_CLASS
            val variation = inputType and InputType.TYPE_MASK_VARIATION
            val password = (type == InputType.TYPE_CLASS_TEXT && variation in listOf(
                InputType.TYPE_TEXT_VARIATION_PASSWORD, InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD,
                InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD)) ||
                (type == InputType.TYPE_CLASS_NUMBER && variation == InputType.TYPE_NUMBER_VARIATION_PASSWORD)
            val layout = when (type) {
                InputType.TYPE_CLASS_NUMBER, InputType.TYPE_CLASS_DATETIME -> "number"
                InputType.TYPE_CLASS_PHONE -> "phone"
                else -> when (variation) {
                    InputType.TYPE_TEXT_VARIATION_URI -> "url"
                    InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS, InputType.TYPE_TEXT_VARIATION_WEB_EMAIL_ADDRESS -> "email"
                    else -> "letters"
                }
            }
            val purpose = if (password) "Password" else if (layout != "letters" || inputType == InputType.TYPE_NULL) "Direct" else "Text"
            val id = if (imeOptions and EditorInfo.IME_FLAG_NO_ENTER_ACTION != 0) EditorInfo.IME_ACTION_NONE else imeOptions and EditorInfo.IME_MASK_ACTION
            val action = when(id) {
                EditorInfo.IME_ACTION_SEND -> "Send"
                EditorInfo.IME_ACTION_SEARCH -> "Search"
                EditorInfo.IME_ACTION_NEXT -> "Next"
                EditorInfo.IME_ACTION_DONE, EditorInfo.IME_ACTION_GO -> "Done"
                else -> "Newline"
            }
            return InputPolicy(layout,purpose,action,id)
        }
    }

    fun enter(connection: InputConnection): Boolean = if (action == "Newline") connection.commitText("\n",1) else connection.performEditorAction(actionId)

    val enterLabel: String get() = when (action) {
        "Send" -> "发送"; "Search" -> "搜索"; "Next" -> "下一项"; "Done" -> "完成"; else -> "换行"
    }
}

object UnicodeEditor {
    /** ICU character boundaries use UTF-16 offsets, matching InputConnection. */
    fun deletionLength(before: String): Int {
        if (before.isEmpty() || Character.isHighSurrogate(before.last())) return 0
        val iterator = BreakIterator.getCharacterInstance(Locale.ROOT)
        iterator.setText(before)
        val start = iterator.preceding(before.length)
        if (start == BreakIterator.DONE || (start == 0 && before.length >= 2048)) return 0
        if (Character.isLowSurrogate(before[start])) return 0
        return before.length - start
    }

    fun backspace(connection: InputConnection, hasSelection: Boolean = false): Boolean {
        if (hasSelection) return connection.commitText("",1)
        val selected = connection.getSelectedText(0)
        if (!selected.isNullOrEmpty()) return connection.commitText("",1)
        val before = connection.getTextBeforeCursor(2048,0)?.toString() ?: return false
        val count = deletionLength(before)
        return count > 0 && connection.deleteSurroundingText(count,0)
    }
}
