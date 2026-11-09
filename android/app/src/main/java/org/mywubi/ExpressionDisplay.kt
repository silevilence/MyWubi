package org.mywubi

import android.graphics.Paint
import android.graphics.Typeface
import android.icu.text.BreakIterator
import android.util.LruCache
import java.util.Locale

/** Display-only system font checks. Candidate IDs and committed strings stay untouched. */
object ExpressionDisplay {
    data class Display(val text: String, val fallback: Boolean)
    private class FontProbe {
        val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { typeface = Typeface.DEFAULT; textSize = 32f }
        val breaks = BreakIterator.getCharacterInstance(Locale.ROOT)
        val cache = LruCache<String, Boolean>(512)
    }
    private val probes = ThreadLocal.withInitial { FontProbe() }

    fun supported(text: String, emoji: Boolean): Boolean {
        val probe = probes.get()
        val key = (if (emoji) "e:" else "k:") + text
        probe.cache.get(key)?.let { return it }
        val supported = if (emoji) probe.paint.hasGlyph(text) else {
            probe.breaks.setText(text)
            var start = probe.breaks.first()
            var end = probe.breaks.next()
            var valid = true
            while (end != BreakIterator.DONE && valid) {
                val cluster = text.substring(start, end)
                val points = cluster.codePoints().toArray()
                val sequence = points.any { it == 0x200D || it == 0xFE0E || it == 0xFE0F || it == 0x20E3 || it in 0x1F3FB..0x1F3FF || it in 0x1F1E6..0x1F1FF }
                valid = if (sequence) probe.paint.hasGlyph(cluster) else points.all { cp ->
                    Character.isWhitespace(cp) || Character.isSpaceChar(cp) || Character.getType(cp) == Character.FORMAT.toInt() ||
                        probe.paint.hasGlyph(String(Character.toChars(cp)))
                }
                start = end; end = probe.breaks.next()
            }
            valid
        }
        probe.cache.put(key, supported)
        return supported
    }

    fun display(text: String, hint: String, kind: String?): Display {
        if (kind == null || supported(text, kind == "emoji")) return Display(text, false)
        val codes = text.codePoints().toArray().joinToString(" ") { "U+%04X".format(Locale.ROOT, it) }
        return Display("[${hint.ifEmpty { "表情" }} · 字体不支持：$codes]", true)
    }
}
