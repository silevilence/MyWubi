package org.mywubi

import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Density
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.junit.Assert.*
import org.junit.Test
import java.io.File

class MainKeyboardMenuTest {
    private val ins = InstrumentationRegistry.getInstrumentation()
    private val device = UiDevice.getInstance(ins)
    private val context = ins.targetContext
    private fun node(label: String): UiObject2 {
        device.waitForIdle(); ins.waitForIdleSync(); ins.uiAutomation.clearCache()
        return (device.wait(Until.findObject(By.desc(label).pkg("org.mywubi")), 5000) ?: error("Missing $label")).actionNode()
    }

    @get:org.junit.Rule val evidence = object : org.junit.rules.TestWatcher() {
        override fun failed(error: Throwable, description: org.junit.runner.Description) {
            device.dumpWindowHierarchy(File(context.filesDir, "keyboard-menu-failure.xml"))
            device.takeScreenshot(File(context.filesDir, "keyboard-menu-failure.png"))
        }
    }

    @Test fun menuKeepsInputModeAndHandlesCancelUnavailableAndLastLayout() {
        val options = (0..9).map { MainKeyboardOption(KeyboardSelection("skin", "1", "custom$it"),
            "自定义布局 $it", if (it == 1) "当前设备不支持" else null) }
        var model by mutableStateOf(KeyboardModel(enabled=true, modesAvailable=true,
            mainKeyboards=options, mainKeyboardSelection=options.first().selection))
        val events = mutableListOf<Any>()
        fun stateLabel(value:KeyboardModel) = "menu-test:${value.mainKeyboards.size}:${value.base}:${value.temporary}:${value.privateInput}:${value.layout}:${value.spelling}"
        val scale = InstrumentationRegistry.getArguments().getString("font_scale")?.toFloat() ?: 1f
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            if (InstrumentationRegistry.getArguments().getString("layout_orientation") == "landscape") {
                scenario.onActivity { it.requestedOrientation = android.content.pm.ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE }
                assertTrue(device.wait(Until.findObject(By.pkg("org.mywubi")), 5000) != null)
                val deadline=android.os.SystemClock.uptimeMillis()+10000
                while(device.displayWidth <= device.displayHeight && android.os.SystemClock.uptimeMillis()<deadline)
                    android.os.SystemClock.sleep(100)
                assertTrue(device.displayWidth > device.displayHeight)
            }
            scenario.onActivity { it.setContent {
                val density = LocalDensity.current
                CompositionLocalProvider(LocalDensity provides Density(density.density, scale)) {
                    Box(Modifier.fillMaxSize().semantics {contentDescription=stateLabel(model)}, contentAlignment=Alignment.BottomCenter) {
                        Keyboard(model, { action -> events.add(action) }, {}, switchIme={})
                    }
                }
            } }
            node("切换形码键盘").click()
            val current=node("形码布局 custom0，当前布局")
            device.dumpWindowHierarchy(File(context.filesDir,"keyboard-menu-open.xml"))
            assertTrue("Current layout: checked=${current.isChecked}, selected=${current.isSelected}", current.isChecked || current.isSelected)
            assertFalse(node("形码布局 custom1").isEnabled)
            node("形码布局 custom0，当前布局").click()
            ins.waitForIdleSync()
            assertTrue(events.isEmpty())
            node("切换形码键盘").click()
            node("形码键盘布局列表")
            device.pressBack()
            assertTrue(events.isEmpty())
            node("切换形码键盘").click()
            // The final author-defined layout remains reachable at large font scales.
            repeat(12) {
                if (!device.hasObject(By.desc("形码布局 custom9"))) {
                    val bounds=node("形码键盘布局列表").visibleBounds
                    device.swipe(bounds.centerX(),bounds.bottom-12,bounds.centerX(),bounds.top+12,20)
                    device.waitForIdle()
                }
            }
            node("形码布局 custom9").click()
            ins.waitForIdleSync()
            assertEquals(listOf(SwitchMainKeyboard(options.last().selection)), events)
            assertEquals("Main", model.base)
            assertNull(model.temporary)
            scenario.onActivity { model=model.copy(switchingKeyboard=true) }
            device.waitForIdle(); ins.uiAutomation.clearCache()
            assertFalse(node("切换形码键盘").isEnabled)
            scenario.onActivity { model=model.copy(switchingKeyboard=false) }
            for (next in listOf(model.copy(mainKeyboards=emptyList(), mainKeyboardSelection=null), model.copy(base="English"),
                model.copy(temporary="Pinyin"), model.copy(privateInput=true), model.copy(layout="number"),
                model.copy(spelling="g"))) {
                scenario.onActivity { model=next }
                assertTrue(device.wait(Until.hasObject(By.desc(stateLabel(next))),5000))
                device.waitForIdle(); ins.uiAutomation.clearCache()
                assertFalse(device.hasObject(By.desc("切换形码键盘")))
            }
            scenario.onActivity {
                model=KeyboardModel(enabled=true, modesAvailable=true,
                    mainKeyboards=options.take(1), mainKeyboardSelection=options.first().selection)
            }
            node("切换形码键盘").click()
            requireNotNull(device.wait(Until.findObject(By.text("原始全键盘")),5000)).click()
            ins.waitForIdleSync()
            assertEquals("RestoreKeyboard",events.last())
            scenario.onActivity { model=KeyboardModel(enabled=true,modesAvailable=true,mainKeyboards=options) }
            node("切换形码键盘").click()
            node("形码键盘布局列表")
            device.takeScreenshot(File(context.filesDir, "keyboard-menu-${device.displayWidth}-$scale.png"))
            device.dumpWindowHierarchy(File(context.filesDir, "keyboard-menu-${device.displayWidth}-$scale.xml"))
            device.pressBack()
        }
    }
}
