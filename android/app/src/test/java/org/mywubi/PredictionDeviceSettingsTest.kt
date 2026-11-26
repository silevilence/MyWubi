package org.mywubi

import org.junit.Assert.*
import org.junit.Test

class PredictionDeviceSettingsTest {
    @Test fun fallbackExplanationsIdentifyModelEvidenceAndRuntimeFailures() {
        assertTrue(npuReason("GGUF_CPU_ONLY").contains("GGUF"))
        assertTrue(npuReason("PLACEMENT_GATE").contains("未确认"))
        assertTrue(npuReason("NNAPI_LOAD: model loading failed").contains("CPU"))
        assertTrue(npuReason("NNAPI_TIMEOUT").contains("连续超时"))
    }
}
