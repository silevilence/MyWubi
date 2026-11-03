package org.mywubi

import org.junit.Assert.*
import org.junit.Test

class KeyboardIdleTest {
    @Test fun directInputAndShortPausesNeverFlashAnUpdate() {
        val idle = KeyboardIdle()
        fun visible(now: Long) = idle.isIdle(now, "", false, false, false)
        idle.activity(10000)
        assertFalse(visible(10000))
        assertFalse(visible(11799))
        assertTrue(visible(11800))
        for (now in 12000L..20000L step 200) { idle.activity(now); assertFalse(visible(now + 199)) }
        assertTrue(visible(21800))
        idle.touch(true, 22000)
        assertFalse(visible(30000))
        idle.touch(false, 30000)
        assertFalse(visible(31000))
        assertTrue(visible(31800))
    }

    @Test fun compositionCandidateInteractionAndTemporarySessionRemainBusy() {
        val idle = KeyboardIdle()
        assertFalse(idle.isIdle(10000, "a", false, false, false))
        assertFalse(idle.isIdle(10000, "", true, false, false))
        assertFalse(idle.isIdle(10000, "", false, true, false))
        assertFalse(idle.isIdle(10000, "", false, false, true))
        assertTrue(idle.isIdle(10000, "", false, false, false))
    }
}
