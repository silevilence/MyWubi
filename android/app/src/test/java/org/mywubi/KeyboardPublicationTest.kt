package org.mywubi

import org.junit.Assert.*
import org.junit.Test

class KeyboardPublicationTest {
    @Test fun touchAndUnfinishedInputDeferGeometryButColorPreservesInput() {
        assertFalse(canPublishKeyboard(1,"",false))
        assertFalse(canPublishKeyboard(2,"[jk]",true))
        assertFalse(canPublishKeyboard(0,"[jk]",false))
        assertTrue(canPublishKeyboard(0,"[jk]",true))
        assertTrue(canPublishKeyboard(0,"",false))
    }
    @Test fun editorRestrictionsAreCheckedBeforeCustomActions() {
        assertTrue(customKeyboardAllowed("Text","letters","Main",null))
        for(purpose in listOf("Password","Direct")) assertFalse(customKeyboardAllowed(purpose,"letters","Main",null))
        for(layout in listOf("number","phone","url","email")) assertFalse(customKeyboardAllowed("Text",layout,"Main",null))
        assertFalse(customKeyboardAllowed("Text","letters","English",null))
        assertFalse(customKeyboardAllowed("Text","letters","Main","Pinyin"))
    }
}
