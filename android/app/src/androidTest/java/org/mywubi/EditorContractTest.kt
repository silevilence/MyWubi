package org.mywubi

import android.text.InputType
import android.view.inputmethod.EditorInfo
import org.junit.Assert.*
import org.junit.Test

class EditorContractTest {
    @Test fun unicodeDeletionUsesWholeGraphemes() {
        for (text in listOf("汉", "😀", "e\u0301", "👩‍💻", "🇨🇳", "👍🏽"))
            assertEquals(text, text.length, UnicodeEditor.deletionLength("x$text"))
        assertEquals(0, UnicodeEditor.deletionLength(""))
        assertEquals(0, UnicodeEditor.deletionLength("\uD83D"))
    }
    @Test fun inputPurposeAndActionAreIndependentOfBaseMode() {
        assertEquals("Password",InputPolicy.from(InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD,EditorInfo.IME_ACTION_DONE).purpose)
        assertEquals("Password",InputPolicy.from(InputType.TYPE_CLASS_NUMBER or InputType.TYPE_NUMBER_VARIATION_PASSWORD,0).purpose)
        assertEquals("phone",InputPolicy.from(InputType.TYPE_CLASS_PHONE,0).layout)
        assertEquals("email",InputPolicy.from(InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS,0).layout)
        assertEquals("Send",InputPolicy.from(InputType.TYPE_CLASS_TEXT,EditorInfo.IME_ACTION_SEND).action)
        assertEquals("Newline",InputPolicy.from(InputType.TYPE_CLASS_TEXT,EditorInfo.IME_ACTION_SEND or EditorInfo.IME_FLAG_NO_ENTER_ACTION).action)
    }
}
