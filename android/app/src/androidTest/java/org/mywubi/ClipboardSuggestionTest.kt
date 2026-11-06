package org.mywubi

import android.content.ClipData
import android.net.Uri
import android.os.PersistableBundle
import org.junit.Assert.*
import org.junit.Test

class ClipboardSuggestionTest {
    @Test fun previewIsBoundedButPasteKeepsFullTextAndSessionDismisses() {
        val suggestion = ClipboardSuggestion()
        val text = "中文 abc\n😀".repeat(100)
        suggestion.offer(ClipData.newPlainText("", text), true)
        assertEquals(text, suggestion.text)
        assertTrue(suggestion.preview()!!.length <= 120)
        assertFalse(suggestion.preview()!!.contains('\n'))
        suggestion.dismiss()
        assertNull(suggestion.text)
        suggestion.offer(ClipData.newPlainText("", "新内容"), true)
        assertEquals("新内容", suggestion.text)
    }

    @Test fun sensitiveDisabledEmptyAndNonTextClipsNeverPreview() {
        val suggestion = ClipboardSuggestion()
        val sensitive = ClipData.newPlainText("password", "secret").apply {
            description.extras = PersistableBundle().apply { putBoolean("android.content.extra.IS_SENSITIVE", true) }
        }
        for (clip in listOf(null, sensitive, ClipData.newRawUri("image", Uri.parse("content://test/image")),
            ClipData.newPlainText("", "  \n"), ClipData.newPlainText("", "a".repeat(100_001)))) {
            suggestion.offer(ClipData.newPlainText("", "old"), true)
            suggestion.offer(clip, true)
            assertNull(suggestion.text)
        }
        suggestion.offer(ClipData.newPlainText("", "hidden"), false)
        assertNull(suggestion.text)
    }
}
