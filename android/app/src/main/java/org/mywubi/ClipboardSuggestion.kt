package org.mywubi

import android.content.ClipData

/** One visible-keyboard session only. Never persists or coerces URIs into text. */
internal class ClipboardSuggestion {
    var text: String? = null
        private set

    fun offer(clip: ClipData?, allowed: Boolean) {
        text = if (!allowed || clip == null || clip.itemCount == 0 ||
            clip.description.extras?.getBoolean("android.content.extra.IS_SENSITIVE", false) == true) null
        else clip.getItemAt(0).text?.toString()?.takeIf { it.isNotBlank() && it.length <= 100_000 }
    }

    fun dismiss() { text = null }
    fun preview(): String? = text?.take(120)?.replace('\n', ' ')?.replace('\r', ' ')
}

internal data class PasteClipboard(val text: String)
