package org.mywubi

import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Density
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.util.concurrent.CopyOnWriteArrayList

class KeyboardLayoutTest {
    @Test fun compositionKeepsKeyboardHeightAndKeyPositionsStable() = checkCompositionLayout()

    @Test fun largeFontCompositionKeepsKeyboardHeightAndKeyPositionsStable() = checkCompositionLayout(2f)

    private fun checkCompositionLayout(fontScale: Float? = null) {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val device = UiDevice.getInstance(instrumentation)
        val height = java.util.concurrent.atomic.AtomicInteger()
        val ready = KeyboardModel(enabled=true, modesAvailable=true, pinyinAvailable=true)
        var model by mutableStateOf(ready)
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            if (InstrumentationRegistry.getArguments().getString("layout_orientation") == "landscape") {
                scenario.onActivity { it.requestedOrientation = android.content.pm.ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE }
                val deadline = android.os.SystemClock.uptimeMillis() + 10000
                while (device.displayWidth <= device.displayHeight && android.os.SystemClock.uptimeMillis() < deadline) {
                    android.os.SystemClock.sleep(100)
                }
                device.waitForIdle()
                assertTrue("Device must really be in landscape", device.displayWidth > device.displayHeight)
            }
            scenario.onActivity { activity -> activity.setContent {
                val density = LocalDensity.current
                CompositionLocalProvider(LocalDensity provides Density(density.density, fontScale ?: density.fontScale)) {
                    Box(Modifier.fillMaxSize(), contentAlignment=Alignment.BottomCenter) {
                        Box(Modifier.onSizeChanged { height.set(it.height) }) {
                            Keyboard(model, {}, {}, switchIme={})
                        }
                    }
                }
            } }
            fun keyBounds(): android.graphics.Rect {
                device.waitForIdle()
                instrumentation.waitForIdleSync()
                instrumentation.uiAutomation.clearCache()
                return device.wait(Until.findObject(By.desc("q").pkg("org.mywubi")), 5000)
                    ?.visibleBounds ?: error("Missing q key")
            }
            val idleKey = keyBounds()
            val idleHeight = height.get()
            assertTrue(idleHeight > 0)
            if (fontScale == null) device.takeScreenshot(File(instrumentation.targetContext.filesDir, "keyboard-height-idle.png"))
            val candidates = listOf(CandidateItem(1, "是", "j"))
            val states = listOf(
                "clipboard" to ready.copy(clipboard="beancount 今日确认 ".repeat(15)),
                "composing" to ready.copy(spelling="j", candidates=candidates),
                "no matches" to ready.copy(spelling="zzzz"),
                "long spelling" to ready.copy(spelling="abcdefghijklmnopqrstuvwxyz".repeat(3)),
                "cleared" to ready,
                "prediction" to ready.copy(prediction=true, candidates=candidates),
                "private input" to ready.copy(privateInput=true),
                "idle again" to ready
            )
            for ((label, next) in states) {
                scenario.onActivity { model = next }
                val bounds = keyBounds()
                if (label == "composing" && fontScale == null) {
                    device.takeScreenshot(File(instrumentation.targetContext.filesDir, "keyboard-height-composing.png"))
                }
                assertEquals("Keyboard height changed in $label", idleHeight, height.get())
                assertEquals("Letter keys moved in $label", idleKey, bounds)
            }
        }
    }

    @Test fun hideKeyboardIsAvailableAcrossInputStatesAndDoesNotDispatchText() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val device = UiDevice.getInstance(instrumentation)
        val events = CopyOnWriteArrayList<Any>()
        val hidden = java.util.concurrent.atomic.AtomicInteger()
        val models = listOf(
            KeyboardModel(),
            KeyboardModel(enabled=true, modesAvailable=true, pinyinAvailable=true),
            KeyboardModel(enabled=true, layout="number"),
            KeyboardModel(enabled=true, privateInput=true),
            KeyboardModel(enabled=true, spelling="a", candidates=listOf(CandidateItem(1, "工", "a")))
        )
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            for ((index, model) in models.withIndex()) {
                scenario.onActivity { activity ->
                    activity.setContent { Keyboard(model.copy(epoch=index.toLong()), {events.add(it)}, {},
                        hideKeyboard={hidden.incrementAndGet()}, switchIme={}) }
                }
                device.waitForIdle()
                if (model.candidates.isNotEmpty()) {
                    device.wait(Until.findObject(By.desc("展开")), 5000).click()
                }
                val button = device.wait(Until.findObject(By.desc("收起键盘").pkg("org.mywubi")), 5000)
                    ?: error("Missing hide keyboard button in state $index")
                assertTrue(button.isEnabled)
                val minimum = (48 * instrumentation.targetContext.resources.displayMetrics.density).toInt()
                assertTrue(button.visibleBounds.width() >= minimum - 1)
                assertTrue(button.visibleBounds.height() >= minimum - 1)
                button.click(); device.waitForIdle(); instrumentation.waitForIdleSync()
                assertEquals(index + 1, hidden.get())
                assertTrue(events.isEmpty())
            }
        }
    }

    @Test fun staggeredLettersAndSeparatePanelsPreserveActions() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val device = UiDevice.getInstance(instrumentation)
        val events = CopyOnWriteArrayList<Any>()
        val landscape = InstrumentationRegistry.getArguments().getString("layout_orientation") == "landscape"
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            if (landscape) {
                scenario.onActivity { it.requestedOrientation = android.content.pm.ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE }
                val deadline = android.os.SystemClock.uptimeMillis() + 10000
                while (device.displayWidth <= device.displayHeight && android.os.SystemClock.uptimeMillis() < deadline) {
                    android.os.SystemClock.sleep(100)
                }
                device.waitForIdle()
                assertTrue("Device must really be in landscape", device.displayWidth > device.displayHeight)
            }
            scenario.onActivity { activity ->
                activity.setContent {
                    Keyboard(KeyboardModel(enabled=true, modesAvailable=true, pinyinAvailable=true, status="MyWubi"),
                        { events.add(it) }, {}, switchIme={})
                }
            }
            fun key(label: String): androidx.test.uiautomator.UiObject2 {
                val end = android.os.SystemClock.uptimeMillis()+5000
                do {
                    instrumentation.uiAutomation.clearCache()
                    device.findObject(By.desc(label).pkg("org.mywubi"))?.let { return it }
                    Thread.sleep(50)
                } while (android.os.SystemClock.uptimeMillis()<end)
                error("Missing key $label")
            }
            fun press(label: String) {
                key(label).click()
                device.waitForIdle()
                instrumentation.waitForIdleSync()
            }
            val q = key("q").visibleBounds
            val a = key("a").visibleBounds
            val z = key("z").visibleBounds
            device.takeScreenshot(File(instrumentation.targetContext.filesDir,"keyboard-letters.png"))
            assertTrue("Second row must be indented", a.left > q.left)
            assertTrue("Third row must be indented", z.left > a.left)
            assertTrue("Letter widths should remain equal", kotlin.math.abs(q.width()-z.width()) <= 2)
            assertEquals("Backspace belongs on the third row", z.centerY(), key("退格").visibleBounds.centerY())
            assertEquals("Shift belongs on the third row", z.centerY(), key("小写").visibleBounds.centerY())
            assertTrue("Space should be wide", key("空格").visibleBounds.width() > q.width()*2)
            device.takeScreenshot(File(instrumentation.targetContext.filesDir,"keyboard-letters.png"))
            press("小写"); press("Q")
            assertTrue(events.last().toString().contains("Q"))
            assertNotNull("单次大写输入一个字母后应回到小写", key("q"))
            press("小写"); press("大写 · 本次"); press("Q"); press("W")
            assertNotNull("锁定大写应保持", key("大写 · 锁定"))
            press("大写 · 锁定")
            assertNotNull(key("q"))
            press("123")
            assertEquals(key("1").visibleBounds.centerX(),key("7").visibleBounds.centerX())
            assertEquals(key("2").visibleBounds.centerX(),key("0").visibleBounds.centerX())
            press("1")
            assertTrue(events.last().toString().contains("1"))
            device.takeScreenshot(File(instrumentation.targetContext.filesDir,"keyboard-numbers.png"))
            press("符号"); press("！")
            assertTrue(events.last().toString().contains("！"))
            press("切换为英文符号"); press("(")
            assertEquals("(", (events.last() as org.json.JSONObject).getJSONArray("InputText").getString(0))
            press("退格")
            assertEquals("Backspace",events.last())
            press("ABC"); press("空格")
            assertEquals("Space",events.last())
            press("回车")
            assertEquals("Enter",events.last())
        }
    }

    @Test fun directFieldsUseAsciiSymbolsAndExpandedCandidatesKeepBackspace() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val device = UiDevice.getInstance(instrumentation)
        val events = CopyOnWriteArrayList<Any>()
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            fun render(model: KeyboardModel) {
                scenario.onActivity { activity ->
                    activity.setContent { Keyboard(model, {events.add(it)}, {}, switchIme={}) }
                }
                device.waitForIdle()
            }
            fun press(label: String) {
                val node = device.wait(Until.findObject(By.desc(label).pkg("org.mywubi")),5000)
                    ?: error("Missing key $label")
                node.click(); device.waitForIdle(); instrumentation.waitForIdleSync()
            }
            val ready = KeyboardModel(enabled=true,modesAvailable=true)
            for ((index, model) in listOf(ready.copy(base="English"), ready.copy(privateInput=true),
                    ready.copy(layout="url"), ready.copy(layout="email")).withIndex()) {
                render(model.copy(epoch=index.toLong()))
                press("符号")
                for (symbol in listOf("(", ")", "!", "\\")) {
                    press(symbol)
                    assertEquals(symbol,(events.last() as org.json.JSONObject).getJSONArray("InputText").getString(0))
                }
            }
            render(ready.copy(epoch=10,spelling="ab",candidates=listOf(CandidateItem(1,"测试","ab")),total=1))
            press("展开")
            assertFalse(device.hasObject(By.desc("q").pkg("org.mywubi")))
            press("退格")
            assertEquals("Backspace",events.last())
        }
    }
}
