package org.mywubi

import android.os.SystemClock
import android.view.InputDevice
import android.view.MotionEvent
import androidx.activity.compose.setContent
import androidx.compose.runtime.*
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.util.concurrent.CopyOnWriteArrayList

class KeyboardTouchTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val device = UiDevice.getInstance(instrumentation)
    private var downTime = 0L

    private fun key(label: String): UiObject2 {
        instrumentation.uiAutomation.clearCache()
        return device.wait(Until.findObject(By.desc(label).pkg("org.mywubi")),5000) ?: error("Missing $label")
    }
    private fun touch(action: Int, x: Int, y: Int) {
        if (action == MotionEvent.ACTION_DOWN) downTime = SystemClock.uptimeMillis()
        val event = MotionEvent.obtain(downTime, SystemClock.uptimeMillis(), action, x.toFloat(), y.toFloat(), 0)
        event.source = InputDevice.SOURCE_TOUCHSCREEN
        try { assertTrue(instrumentation.uiAutomation.injectInputEvent(event, true)) } finally { event.recycle() }
    }
    private fun settled() { device.waitForIdle(); instrumentation.waitForIdleSync(); instrumentation.uiAutomation.clearCache() }

    @Test fun tapPreviewSwipeHoldAndCancellationCommitExactlyOnce() {
        val events = CopyOnWriteArrayList<Any>()
        var model by mutableStateOf(KeyboardModel(enabled=true, modesAvailable=true, pinyinAvailable=true, status="测试码表"))
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { it.setContent { Keyboard(model, { events.add(it) }, {}, switchIme={}) } }
            val q = key("q").visibleCenter
            touch(MotionEvent.ACTION_DOWN, q.x, q.y)
            SystemClock.sleep(100)
            assertTrue(device.hasObject(By.desc("按键预览")))
            assertTrue("按住期间尚未提交", events.isEmpty())
            device.takeScreenshot(File(instrumentation.targetContext.filesDir,"keyboard-key-preview.png"))
            touch(MotionEvent.ACTION_CANCEL, q.x, q.y); settled()
            assertTrue(events.isEmpty())
            key("q").click(); settled()
            assertEquals(1, events.size)
            assertEquals("q", (events.last() as JSONObject).getJSONArray("InputText").getString(0))
            assertFalse(device.hasObject(By.desc("按键预览")))

            val density = instrumentation.targetContext.resources.displayMetrics.density
            val distance = (30 * density).toInt()
            val w = key("w").visibleCenter
            touch(MotionEvent.ACTION_DOWN, w.x, w.y)
            touch(MotionEvent.ACTION_MOVE, w.x, w.y-distance)
            touch(MotionEvent.ACTION_UP, w.x, w.y-distance); settled()
            assertEquals(2, events.size)
            assertEquals("2", (events.last() as JSONObject).getString("InputSymbol"))

            val s = key("s").visibleCenter
            touch(MotionEvent.ACTION_DOWN, s.x, s.y)
            SystemClock.sleep(650)
            assertEquals("长按只预选，松手才输入", 2, events.size)
            device.takeScreenshot(File(instrumentation.targetContext.filesDir,"keyboard-symbol-preview.png"))
            touch(MotionEvent.ACTION_UP, s.x, s.y); settled()
            assertEquals(3, events.size)
            assertEquals("！", (events.last() as JSONObject).getString("InputSymbol"))

            val d = key("d").visibleCenter
            touch(MotionEvent.ACTION_DOWN, d.x, d.y)
            SystemClock.sleep(650)
            touch(MotionEvent.ACTION_CANCEL, d.x, d.y); settled()
            assertEquals(3, events.size)
            assertFalse(device.hasObject(By.desc("按键预览")))

            touch(MotionEvent.ACTION_DOWN, d.x, d.y)
            touch(MotionEvent.ACTION_MOVE, d.x+distance*3, d.y)
            touch(MotionEvent.ACTION_UP, d.x+distance*3, d.y); settled()
            assertEquals("横向移出取消输入", 3, events.size)

            touch(MotionEvent.ACTION_DOWN, d.x, d.y)
            scenario.onActivity { model = model.copy(epoch=1) }
            SystemClock.sleep(650)
            touch(MotionEvent.ACTION_UP, d.x, d.y); settled()
            assertEquals("切换输入上下文后不能提交旧按键", 3, events.size)

            scenario.onActivity { model = model.copy(epoch=2, privateInput=true, base="English") }
            val password = key("s").visibleCenter
            touch(MotionEvent.ACTION_DOWN, password.x, password.y)
            SystemClock.sleep(100)
            instrumentation.uiAutomation.clearCache()
            assertFalse(device.hasObject(By.desc("按键预览")))
            touch(MotionEvent.ACTION_UP, password.x, password.y); settled()
            assertEquals(4, events.size)
        }
    }

    @Test fun modesUseFixedLabelAndSymbolsDoNotConsumeOneShotShift() {
        val events = CopyOnWriteArrayList<Any>()
        var mode by mutableStateOf(KeyMode.Off)
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { it.setContent {
                Keyboard(KeyboardModel(enabled=true, modesAvailable=true, pinyinAvailable=true, pinyinMode=mode),
                    { if(it == "CyclePinyin") mode=mode.next() else events.add(it) }, {}, switchIme={})
            } }
            for (label in listOf("拼音", "拼音 · 本次", "拼音 · 锁定")) {
                key(label).click(); settled()
                assertEquals(1, device.findObjects(By.descStartsWith("拼音")).size)
                assertFalse("视觉状态不使用文字后缀", device.hasObject(By.textContains("· 本次")))
                assertFalse(device.hasObject(By.textContains("锁定")))
                device.takeScreenshot(File(instrumentation.targetContext.filesDir,"keyboard-mode-${mode.name}.png"))
            }
            key("小写").click(); settled()
            val upper = key("Q").visibleCenter
            touch(MotionEvent.ACTION_DOWN, upper.x, upper.y)
            SystemClock.sleep(650)
            touch(MotionEvent.ACTION_UP, upper.x, upper.y); settled()
            assertEquals("1", (events.last() as JSONObject).getString("InputSymbol"))
            key("Q").click(); settled()
            assertEquals("Q", (events.last() as JSONObject).getJSONArray("InputText").getString(0))
            assertNotNull(key("q"))
        }
    }

    @Test fun previewDoesNotBlockRapidTypingAndHidingCancelsHeldKey() {
        val events = CopyOnWriteArrayList<Any>()
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { it.setContent {
                Keyboard(KeyboardModel(enabled=true), { events.add(it) }, {}, switchIme={})
            } }
            val a = key("a").visibleCenter
            val q = key("q").visibleCenter
            touch(MotionEvent.ACTION_DOWN, a.x, a.y)
            touch(MotionEvent.ACTION_UP, a.x, a.y)
            SystemClock.sleep(35)
            // The enlarged a preview sits over q; the next finger must reach q.
            touch(MotionEvent.ACTION_DOWN, q.x, q.y)
            touch(MotionEvent.ACTION_UP, q.x, q.y); settled()
            assertEquals(listOf("a", "q"), events.map { (it as JSONObject).getJSONArray("InputText").getString(0) })
            touch(MotionEvent.ACTION_DOWN, a.x, a.y)
            scenario.moveToState(androidx.lifecycle.Lifecycle.State.CREATED)
            SystemClock.sleep(650)
            touch(MotionEvent.ACTION_UP, a.x, a.y)
            scenario.moveToState(androidx.lifecycle.Lifecycle.State.RESUMED)
            settled()
            assertEquals(2, events.size)
            assertFalse(device.hasObject(By.desc("按键预览")))
        }
    }
}
