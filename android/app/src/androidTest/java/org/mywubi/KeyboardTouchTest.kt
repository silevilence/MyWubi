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
    private val delivered = CopyOnWriteArrayList<String>()

    private fun observeTouchWindow(activity: MainActivity) {
        val original = activity.window.callback
        activity.window.callback = object : android.view.Window.Callback by original {
            override fun dispatchTouchEvent(event: MotionEvent): Boolean {
                delivered.add("${event.actionMasked}/${event.actionIndex} @${event.eventTime - event.downTime}")
                return original.dispatchTouchEvent(event)
            }
        }
    }


    @Test fun eightDirectionsMergedUpAndReturnToCenterUseOneReleaseAction() {
        val events=CopyOnWriteArrayList<Any>()
        val mergedApplied=java.util.concurrent.CountDownLatch(1)
        val bindings=(0..7).associateWith { LayoutAction("Character",('a'+it).toString()) }
        var model by mutableStateOf(KeyboardModel(enabled=true, customLayout=KeyboardLayout("test.directions",3f,3f,
            listOf(LayoutKey("direction","方向键",1f,1f,1f,action=LayoutAction("Character","x"),directions=bindings)))))
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { it.setContent {
                Keyboard(model,{events.add(it)},{},switchIme={})
                SideEffect { if(model.customLayout?.id=="test.merged") mergedApplied.countDown() }
            } }
            val point=key("方向键").visibleCenter
            val length=40*instrumentation.targetContext.resources.displayMetrics.density
            for(direction in 0..7) {
                val angle=direction*Math.PI/4
                val x=point.x+(kotlin.math.sin(angle)*length).toInt()
                val y=point.y-(kotlin.math.cos(angle)*length).toInt()
                touch(MotionEvent.ACTION_DOWN,point.x,point.y)
                touch(MotionEvent.ACTION_MOVE,x,y)
                touch(MotionEvent.ACTION_UP,x,y)
            }
            settled()
            assertEquals(('a'..'h').map(Char::toString),events.map {(it as JSONObject).getJSONArray("InputText").getString(0)})
            touch(MotionEvent.ACTION_DOWN,point.x,point.y)
            touch(MotionEvent.ACTION_MOVE,point.x,point.y-length.toInt())
            touch(MotionEvent.ACTION_MOVE,point.x,point.y)
            touch(MotionEvent.ACTION_UP,point.x,point.y);settled()
            assertEquals(8,events.size)
            val old=model.customLayout!!
            val merged=old.keys.single().copy(directions=bindings+(listOf(7,0,1).associateWith {LayoutAction("Character","u")}))
            scenario.onActivity {model=model.copy(customLayout=old.copy(id="test.merged",keys=listOf(merged)))}
            assertTrue("Merged bindings must be composed before injecting the next gesture",
                mergedApplied.await(5,java.util.concurrent.TimeUnit.SECONDS))
            settled()
            for(direction in listOf(7,0,1)) {
                val angle=direction*Math.PI/4
                val x=point.x+(kotlin.math.sin(angle)*length).toInt()
                val y=point.y-(kotlin.math.cos(angle)*length).toInt()
                touch(MotionEvent.ACTION_DOWN,point.x,point.y);touch(MotionEvent.ACTION_MOVE,x,y);touch(MotionEvent.ACTION_UP,x,y)
            }
            settled()
            assertEquals(List(3){"u"},events.takeLast(3).map {(it as JSONObject).getJSONArray("InputText").getString(0)})
        }
    }

    private fun touchInsideKey(scenario: ActivityScenario<MainActivity>, label: String): android.graphics.Point {
        val bounds = key(label).visibleBounds
        val point = android.graphics.Point(bounds.centerX(), bounds.centerY())
        if (android.os.Build.VERSION.SDK_INT >= 30) scenario.onActivity { activity ->
            val insets = activity.window.decorView.rootWindowInsets.getInsets(android.view.WindowInsets.Type.systemGestures())
            val left = maxOf(bounds.left + 1, insets.left + 3)
            val right = minOf(bounds.right - 1, device.displayWidth - insets.right - 3)
            if (left <= right) point.x = point.x.coerceIn(left, right)
        }
        return point
    }

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

    @Test fun holdPickerFollowsTheCurrentBindingsOfTheSameKey() {
        val events = CopyOnWriteArrayList<Any>()
        val pair = LayoutKey("pair", "QW", 2f, 1f, 1f, action = LayoutAction("Group", "pair"),
            groupCharacters = listOf("q", "w"))
        val symbol = LayoutAction("Character", "☆")
        var layout by mutableStateOf(KeyboardLayout("custom", 3f, 3f,
            listOf(pair.copy(directions = mapOf(0 to symbol, 1 to symbol)))))
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { it.setContent {
                Keyboard(KeyboardModel(customLayout = layout, enabled = true), { events.add(it) }, {}, switchIme = {})
            } }
            fun pick(options: List<String>, target: String, slide: Boolean) {
                val key = key("QW").visibleCenter
                touch(MotionEvent.ACTION_DOWN, key.x, key.y)
                val strip = requireNotNull(findFresh(device, By.desc("长按选择"), 5000))
                val items = strip.findObjects(By.text(java.util.regex.Pattern.compile(".+")))
                assertEquals(options, items.sortedBy { it.visibleCenter.x }.map { it.text })
                val x = if (slide) items.single { it.text == target }.visibleCenter.x else key.x
                if (slide) touch(MotionEvent.ACTION_MOVE, x, key.y)
                val beforeRelease = events.size
                touch(MotionEvent.ACTION_UP, x, key.y); settled()
                assertEquals(beforeRelease + 1, events.size)
                assertEquals(target, (events.last() as JSONObject).getJSONArray("InputText").getString(0))
            }
            pick(listOf("q", "☆", "w"), "☆", slide = false)
            pick(listOf("q", "☆", "w"), "w", slide = true)
            scenario.onActivity { layout = layout.copy(keys = listOf(pair.copy(groupCharacters = listOf("j", "k")))) }
            settled()
            pick(listOf("j", "k"), "k", slide = true)
        }
    }

    private fun fingers(action: Int, vararg points: Pair<Int, android.graphics.Point>) {
        // Pace individual contact reports at 120 Hz. Both fingers still overlap
        // and every release must be delivered and produce exactly one action.
        SystemClock.sleep(8)
        if (action == MotionEvent.ACTION_DOWN) downTime = SystemClock.uptimeMillis()
        val properties = points.map { (id, _) -> MotionEvent.PointerProperties().apply {
            this.id = id; toolType = MotionEvent.TOOL_TYPE_FINGER
        } }.toTypedArray()
        val coordinates = points.map { (_, point) -> MotionEvent.PointerCoords().apply {
            x = point.x.toFloat(); y = point.y.toFloat(); pressure = 1f; size = 1f
        } }.toTypedArray()
        val event = MotionEvent.obtain(downTime, SystemClock.uptimeMillis(), action, points.size,
            properties, coordinates, 0, 0, 1f, 1f, 0, 0, InputDevice.SOURCE_TOUCHSCREEN, 0)
        try { assertTrue(instrumentation.uiAutomation.injectInputEvent(event, true)) } finally { event.recycle() }
    }

    @Test fun overlappingAlternatingFingersDoNotDropRepeatedKeys() {
        val events = CopyOnWriteArrayList<Any>()
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { observeTouchWindow(it); it.setContent { Keyboard(KeyboardModel(enabled=true), { events.add(it) }, {}, switchIme={}) } }
            val q = touchInsideKey(scenario, "q")
            val w = key("w").visibleCenter
            settled()
            repeat(10) {
                fingers(MotionEvent.ACTION_DOWN, 0 to q)
                fingers(MotionEvent.ACTION_POINTER_DOWN or (1 shl MotionEvent.ACTION_POINTER_INDEX_SHIFT), 0 to q, 1 to w)
                fingers(MotionEvent.ACTION_POINTER_UP, 0 to q, 1 to w)
                // q is tapped again while w is still held, as during two-thumb typing.
                fingers(MotionEvent.ACTION_POINTER_DOWN or (1 shl MotionEvent.ACTION_POINTER_INDEX_SHIFT), 1 to w, 2 to q)
                fingers(MotionEvent.ACTION_POINTER_UP, 1 to w, 2 to q)
                fingers(MotionEvent.ACTION_UP, 2 to q)
            }
            settled()
            assertEquals("Every injected event must reach the activity: $delivered", 60, delivered.size)
            assertEquals("Every release must commit once even while the other finger stays down",
                List(10) { listOf("q", "w", "q") }.flatten(),
                events.map { (it as JSONObject).getJSONArray("InputText").getString(0) })
        }
    }

    @Test fun visualKeyGuttersStillAcceptTaps() {
        val events = CopyOnWriteArrayList<Any>()
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { it.setContent { Keyboard(KeyboardModel(enabled=true), { events.add(it) }, {}, switchIme={}) } }
            val q = key("q").visibleCenter
            val w = key("w").visibleCenter
            val boundary = (q.x + w.x) / 2
            // Both sides of the visual gutter belong to the nearest key column.
            for (x in listOf(boundary - 1, boundary + 1)) {
                touch(MotionEvent.ACTION_DOWN, x, q.y)
                touch(MotionEvent.ACTION_UP, x, q.y)
            }
            settled()
            assertEquals(listOf("q", "w"), events.map { (it as JSONObject).getJSONArray("InputText").getString(0) })
        }
    }

    @Test fun deleteHoldSwipeReleaseReturnAndContextCancellation() {
        val events = CopyOnWriteArrayList<Any>()
        var model by mutableStateOf(KeyboardModel(enabled = true))
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { it.setContent { Keyboard(model, { events.add(it) }, {}, switchIme = {}) } }
            key("退格").click(); settled()
            assertEquals(listOf("Backspace"), events.toList())
            val key = key("退格").visibleCenter
            val rise = (70 * instrumentation.targetContext.resources.displayMetrics.density).toInt()
            touch(MotionEvent.ACTION_DOWN, key.x, key.y)
            SystemClock.sleep(550)
            touch(MotionEvent.ACTION_MOVE, key.x, key.y - rise)
            SystemClock.sleep(100)
            assertTrue(device.hasObject(By.desc("松手清空 · 下滑取消")))
            val beforeRelease = events.size
            SystemClock.sleep(150)
            assertEquals("上滑后停止逐字删除", beforeRelease, events.size)
            touch(MotionEvent.ACTION_UP, key.x, key.y - rise); settled()
            assertEquals("ClearEditor", events.last())
            assertEquals(1, events.count { it == "ClearEditor" })
            device.takeScreenshot(File(instrumentation.targetContext.filesDir, "keyboard-delete-complete.png"))

            touch(MotionEvent.ACTION_DOWN, key.x, key.y)
            SystemClock.sleep(450)
            touch(MotionEvent.ACTION_MOVE, key.x, key.y - rise)
            touch(MotionEvent.ACTION_MOVE, key.x, key.y)
            touch(MotionEvent.ACTION_UP, key.x, key.y); settled()
            assertEquals("滑回取消清空", 1, events.count { it == "ClearEditor" })

            touch(MotionEvent.ACTION_DOWN, key.x, key.y)
            SystemClock.sleep(450)
            touch(MotionEvent.ACTION_MOVE, key.x, key.y - rise)
            scenario.onActivity { model = model.copy(epoch = 1) }
            SystemClock.sleep(100)
            val afterContext = events.size
            touch(MotionEvent.ACTION_UP, key.x, key.y - rise); settled()
            assertEquals(afterContext, events.size)
            assertFalse(device.hasObject(By.desc("松手清空 · 下滑取消")))

            touch(MotionEvent.ACTION_DOWN, key.x, key.y)
            SystemClock.sleep(450)
            touch(MotionEvent.ACTION_MOVE, key.x, key.y - rise)
            touch(MotionEvent.ACTION_CANCEL, key.x, key.y - rise); settled()
            assertEquals(1, events.count { it == "ClearEditor" })
        }
    }

    @Test fun tapPreviewSwipeHoldAndCancellationCommitExactlyOnce() {
        val events = CopyOnWriteArrayList<Any>()
        var model by mutableStateOf(KeyboardModel(enabled=true, modesAvailable=true, pinyinAvailable=true, status="测试码表"))
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { observeTouchWindow(it); it.setContent { Keyboard(model, { events.add(it) }, {}, switchIme={}) } }
            settled()
            val q = touchInsideKey(scenario, "q")
            touch(MotionEvent.ACTION_DOWN, q.x, q.y)
            SystemClock.sleep(100)
            instrumentation.uiAutomation.clearCache()
            // Read the tap preview before the long-press deadline changes it into a picker.
            val config = Configurator.getInstance()
            val idleTimeout = config.waitForIdleTimeout
            val visible = try {
                config.waitForIdleTimeout = 0
                device.hasObject(By.desc("按键预览"))
            } finally { config.waitForIdleTimeout = idleTimeout }
            if (!visible) {
                device.takeScreenshot(File(instrumentation.targetContext.filesDir, "preview-down-failure.png"))
                device.dumpWindowHierarchy(File(instrumentation.targetContext.filesDir, "preview-down-failure.xml"))
            }
            assertTrue("Preview missing at $q; activity events=$delivered", visible)
            assertTrue("按住期间尚未提交", events.isEmpty())
            device.takeScreenshot(File(instrumentation.targetContext.filesDir,"keyboard-key-preview.png"))
            touch(MotionEvent.ACTION_CANCEL, q.x, q.y); settled()
            assertTrue(events.isEmpty())
            device.click(q.x,q.y); settled()
            assertEquals(1, events.size)
            assertEquals("q", (events.last() as JSONObject).getJSONArray("InputText").getString(0))
            // Successful releases intentionally retain the bubble briefly. Compose
            // idleness does not wait for the delayed clear coroutine.
            val previewDeadline=SystemClock.uptimeMillis()+2000
            do {
                instrumentation.uiAutomation.clearCache()
                if(!device.hasObject(By.desc("按键预览"))) break
                SystemClock.sleep(25)
            } while(SystemClock.uptimeMillis()<previewDeadline)
            assertFalse("Release preview did not clear",device.hasObject(By.desc("按键预览")))

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
            SystemClock.sleep(android.view.ViewConfiguration.getLongPressTimeout().toLong() + 175)
            assertEquals("长按只预选，松手才输入", 2, events.size)
            device.takeScreenshot(File(instrumentation.targetContext.filesDir,"keyboard-symbol-preview.png"))
            touch(MotionEvent.ACTION_UP, s.x, s.y); settled()
            assertEquals(3, events.size)
            assertEquals("！", (events.last() as JSONObject).getString("InputSymbol"))

            val d = key("d").visibleCenter
            touch(MotionEvent.ACTION_DOWN, d.x, d.y)
            SystemClock.sleep(android.view.ViewConfiguration.getLongPressTimeout().toLong() + 175)
            touch(MotionEvent.ACTION_CANCEL, d.x, d.y); settled()
            assertEquals(3, events.size)
            assertFalse(device.hasObject(By.desc("按键预览")))

            touch(MotionEvent.ACTION_DOWN, d.x, d.y)
            touch(MotionEvent.ACTION_MOVE, d.x+distance*3, d.y)
            touch(MotionEvent.ACTION_UP, d.x+distance*3, d.y); settled()
            assertEquals("横向移出取消输入", 3, events.size)

            touch(MotionEvent.ACTION_DOWN, d.x, d.y)
            scenario.onActivity { model = model.copy(epoch=1) }
            SystemClock.sleep(android.view.ViewConfiguration.getLongPressTimeout().toLong() + 175)
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
                val target = (48 * instrumentation.targetContext.resources.displayMetrics.density).toInt()
                assertTrue(kotlin.math.abs(key(label).visibleBounds.width() - target) <= 1)
                assertFalse("Toolbar actions use icons, not visible labels", device.hasObject(By.text("拼音")))
                key(label).click(); settled()
                assertEquals(1, device.findObjects(By.descStartsWith("拼音")).size)
                assertFalse("视觉状态不使用文字后缀", device.hasObject(By.textContains("· 本次")))
                assertFalse(device.hasObject(By.textContains("锁定")))
                device.takeScreenshot(File(instrumentation.targetContext.filesDir,"keyboard-mode-${mode.name}.png"))
            }
            assertFalse("Toolbar must not have an expandable second row", device.hasObject(By.desc("键盘工具")))
            assertEquals("Settings must have only one entry", 1,
                device.findObjects(By.descStartsWith("设置").pkg("org.mywubi")).size)
            assertEquals("Pinyin must have only one entry", 1,
                device.findObjects(By.descStartsWith("拼音").pkg("org.mywubi")).size)
            val toolbarY = key("设置").visibleBounds.centerY()
            for (label in listOf("切换输入法", "特殊输入", "拼音", "收起键盘")) {
                assertEquals("All toolbar actions must share one row", toolbarY, key(label).visibleBounds.centerY())
                assertFalse("Tool panel actions use icons", device.hasObject(By.text(label)))
            }
            device.takeScreenshot(File(instrumentation.targetContext.filesDir,"keyboard-tool-icons.png"))
            key("小写").click(); settled()
            val upper = key("Q").visibleCenter
            touch(MotionEvent.ACTION_DOWN, upper.x, upper.y)
            SystemClock.sleep(android.view.ViewConfiguration.getLongPressTimeout().toLong() + 175)
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
            // A translucent ActivityScenario trampoline only pauses the activity on ColorOS.
            // Home actually hides the keyboard window while the finger is still held.
            assertTrue(device.pressHome())
            val deadline = SystemClock.uptimeMillis() + 5000
            while (scenario.state != androidx.lifecycle.Lifecycle.State.CREATED && SystemClock.uptimeMillis() < deadline) {
                SystemClock.sleep(50)
            }
            assertEquals(androidx.lifecycle.Lifecycle.State.CREATED, scenario.state)
            SystemClock.sleep(android.view.ViewConfiguration.getLongPressTimeout().toLong() + 175)
            touch(MotionEvent.ACTION_UP, a.x, a.y)
            val pkg = instrumentation.targetContext.packageName
            device.executeShellCommand("am start -W --activity-reorder-to-front -n $pkg/org.mywubi.MainActivity")
            assertEquals(androidx.lifecycle.Lifecycle.State.RESUMED, scenario.state)
            settled()
            assertEquals(2, events.size)
            assertFalse(device.hasObject(By.desc("按键预览")))
        }
    }
}
