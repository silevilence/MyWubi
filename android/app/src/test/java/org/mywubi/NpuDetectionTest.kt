package org.mywubi

import org.junit.Assert.assertEquals
import org.junit.Test

class NpuDetectionTest {
    @Test fun hardwareAndComponentEvidenceAreIndependent() {
        with(NpuDetection) {
            assertEquals(NpuDetection.State.Available, classify(true, true, true))
            assertEquals(NpuDetection.State.Hardware, classify(true, false, true))
            assertEquals(NpuDetection.State.Unknown, classify(false, true, false))
            assertEquals(NpuDetection.State.Unknown, classify(false, true, true))
            assertEquals(NpuDetection.State.Unavailable, classify(false, false, false))
        }
    }
}
