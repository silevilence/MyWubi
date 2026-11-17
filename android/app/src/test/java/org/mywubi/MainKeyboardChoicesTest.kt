package org.mywubi

import org.junit.Assert.*
import org.junit.Test

class MainKeyboardChoicesTest {
    private fun option(pack: String, version: String, component: String) =
        MainKeyboardOption(KeyboardSelection(pack, version, component), component)

    @Test fun selectedSkinLimitsChoicesToItsExactPackageVersion() {
        val first = option("skin", "2", "custom17")
        val last = option("skin", "2", "shape9")
        val others = listOf(option("skin", "1", "shape14"), option("another", "2", "full26"))
        assertEquals(listOf(first, last), mainKeyboardFamily(listOf(first) + others + last,
            KeyboardSelection("skin", "2", "paint"), others.last().selection))
    }

    @Test fun appearanceOnlySkinUsesCurrentKeyboardPackage() {
        val options = listOf(option("keyboard", "4", "full26"), option("keyboard", "4", "shape14"))
        assertEquals(options, mainKeyboardFamily(options, KeyboardSelection("paint", "1", "jade"), options.last().selection))
        assertTrue(mainKeyboardFamily(options, null, null).isEmpty())
    }

    @Test fun aSingleLayoutDoesNotBorrowOptionsFromOtherSkins() {
        val only = option("single", "1", "custom")
        assertEquals(listOf(only), mainKeyboardFamily(listOf(only, option("other", "1", "shape9")),
            KeyboardSelection("single", "1", "appearance"), null))
    }
}
