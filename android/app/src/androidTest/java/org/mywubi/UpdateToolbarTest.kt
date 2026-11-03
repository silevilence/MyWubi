package org.mywubi

import androidx.activity.compose.setContent
import androidx.compose.runtime.*
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.junit.Assert.*
import org.junit.Test

class UpdateToolbarTest {
    @Test fun idleBadgeIsStaticAndNeverReplacesCandidatesOrDirectInput() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val device = UiDevice.getInstance(instrumentation)
        var model by mutableStateOf(KeyboardModel(enabled=true, idle=true, updateAvailable=true))
        var opened = 0
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { activity -> activity.setContent {
                Keyboard(model, {}, {}, openSettings={opened++}, onTouch={
                    model = model.copy(idle=false)
                }, switchIme={})
            } }
            fun badge() = By.desc("设置，有可用更新")
            assertTrue(device.wait(Until.hasObject(badge()), 5000))
            device.findObject(badge()).click()
            instrumentation.waitForIdleSync()
            assertEquals("Hiding the dot on touch must not remove the settings button", 1, opened)
            for (direct in listOf("English", "number", "password")) {
                scenario.onActivity { model = model.copy(idle=false, spelling="", privateInput=direct=="password", base=if(direct=="English") "English" else "Main", layout=if(direct=="number") "number" else "letters") }
                device.waitForIdle()
                assertFalse(device.hasObject(badge()))
                assertNotNull(device.findObject(By.desc("设置")))
            }
            scenario.onActivity { model = model.copy(spelling="g", privateInput=false, candidates=listOf(CandidateItem(1,"王","")), total=1) }
            assertTrue(device.wait(Until.hasObject(By.descStartsWith("候选 王")),5000))
            assertFalse(device.hasObject(badge()))
            assertFalse(device.hasObject(By.desc("设置")))
            scenario.onActivity { model = model.copy(idle=true, spelling="", candidates=emptyList(), total=0) }
            assertTrue(device.wait(Until.hasObject(badge()),5000))
            scenario.onActivity { model = model.copy(updateAvailable=false) }
            assertTrue(device.wait(Until.hasObject(By.desc("设置")),5000))
            assertFalse(device.hasObject(badge()))
        }
    }
}
