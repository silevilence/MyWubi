package org.mywubi
import org.junit.Assert.*
import org.junit.Test

class PredictionPrefixTest {
    @Test fun preservesEmojiAndRejectsUnpairedSurrogates() {
        assertEquals("中😀文", PredictionPrefix.bounded("中😀文"))
        assertEquals("中", PredictionPrefix.bounded("\uDE00中"))
        assertNull(PredictionPrefix.bounded("中\uD83D"))
        assertNull(PredictionPrefix.bounded("中\uDE00"))
        val result = PredictionPrefix.bounded("😀".repeat(300))!!
        assertEquals(256, result.codePointCount(0,result.length))
    }
}
