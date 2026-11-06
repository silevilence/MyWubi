package org.mywubi

import android.view.inputmethod.InputConnection

/** Explicit editor actions, independent of the active dictionary and its code alphabet. */
internal object EditorShortcuts {
    fun clear(connection: InputConnection, current: () -> Boolean): Boolean = batch(connection, current) {
        current() && connection.finishComposingText() &&
            current() && connection.performContextMenuAction(android.R.id.selectAll) &&
            current() && connection.commitText("", 1) && current()
    }

    fun paste(connection: InputConnection, text: String, current: () -> Boolean): Boolean = batch(connection, current) {
        current() && connection.commitText(text, 1) && current()
    }

    private inline fun batch(connection: InputConnection, current: () -> Boolean, action: () -> Boolean): Boolean {
        if (!current()) return false
        connection.beginBatchEdit()
        return try { action() } finally { connection.endBatchEdit() }
    }
}
