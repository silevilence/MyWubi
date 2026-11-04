package org.mywubi

/** Reject malformed/truncated UTF-16 instead of passing replacement characters to Rust. */
internal object PredictionPrefix {
    fun bounded(value: String): String? {
        val text = if (value.firstOrNull()?.isLowSurrogate() == true) value.drop(1) else value
        var i = 0
        while (i < text.length) {
            val c = text[i++]
            if (c.isHighSurrogate()) {
                if (i >= text.length || !text[i].isLowSurrogate()) return null
                i++
            } else if (c.isLowSurrogate()) return null
        }
        val count = text.codePointCount(0, text.length)
        return if (count > 256) text.substring(text.offsetByCodePoints(0, count - 256)) else text
    }
}
