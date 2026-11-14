package org.mywubi

import org.junit.Assert.*
import org.junit.Test
import kotlin.math.*

class DirectionalGestureTest {
    @Test fun eightDirectionsAndReleaseAreExclusive() {
        for(direction in 0..7) {
            val gesture=DirectionalGesture((0..7).toSet(),true)
            val angle=direction*PI/4
            gesture.move((sin(angle)*50).toFloat(),(-cos(angle)*50).toFloat())
            gesture.hold()
            assertEquals(direction,gesture.release())
            assertNull(gesture.release())
        }
    }
    @Test fun returnToCenterUnboundAndSystemCancellationNeverClick() {
        val returning=DirectionalGesture(setOf(0),true)
        returning.move(0f,-40f);returning.move(0f,-2f);returning.move(0f,-40f)
        assertNull(returning.release())
        val unbound=DirectionalGesture(setOf(0),false);unbound.move(50f,0f)
        assertNull(unbound.release())
        val cancelled=DirectionalGesture(setOf(0),true);cancelled.hold();cancelled.cancel()
        assertNull(cancelled.release())
    }
    @Test fun boundaryJitterAndHoldCompetitionAreStable() {
        val gesture=DirectionalGesture((0..7).toSet(),true)
        gesture.move(0f,-50f)
        for(degrees in listOf(21.0,24.0,22.0,26.0)) {
            gesture.move((sin(degrees*PI/180)*50).toFloat(),(-cos(degrees*PI/180)*50).toFloat())
            assertEquals(0,gesture.direction)
        }
        val held=DirectionalGesture(setOf(0),true);held.hold();held.move(0f,-40f)
        assertEquals(-2,held.release())
        val tap=DirectionalGesture(setOf(0),false);tap.move(0f,-10f)
        assertEquals(-1,tap.release())
    }
}
