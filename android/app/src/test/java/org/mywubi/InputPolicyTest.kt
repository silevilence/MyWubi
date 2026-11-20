package org.mywubi

import android.text.InputType
import android.view.inputmethod.EditorInfo
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class InputPolicyTest {
    @Test fun addressBarAllowsCompositionWithUrlKeysAndOriginalAction() {
        val type = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
        val policy = InputPolicy.from(type, EditorInfo.IME_ACTION_GO)
        assertEquals("Text", policy.purpose)
        assertEquals("url", policy.layout)
        assertEquals(EditorInfo.IME_ACTION_GO, policy.actionId)
        assertFalse(policy.allowsAssistance)
    }

    @Test fun restrictedFieldsAndPrivateTextKeepTheirLimits() {
        for (type in listOf(InputType.TYPE_NULL, InputType.TYPE_CLASS_NUMBER, InputType.TYPE_CLASS_PHONE,
            InputType.TYPE_CLASS_DATETIME, InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS)) {
            assertEquals("Direct", InputPolicy.from(type, 0).purpose)
        }
        val password = InputPolicy.from(InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD, 0)
        assertEquals("Password", password.purpose)
        assertFalse(password.allowsAssistance)
        assertTrue(InputPolicy.from(InputType.TYPE_CLASS_TEXT, 0).allowsAssistance)
        assertFalse(InputPolicy.from(InputType.TYPE_CLASS_TEXT, EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING).allowsAssistance)
    }
}
