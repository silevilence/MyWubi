package org.mywubi

import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
import org.junit.Assert.*
import org.junit.Test
import android.os.SystemClock

/** Uses the installed system Settings editor, not an EditText owned by this app. */
class ExternalHostTest {
    private var originalConfig: org.json.JSONObject? = null
    @org.junit.Before fun prepareRealTable() {
        val store=EngineStore(InstrumentationRegistry.getInstrumentation().targetContext)
        val loaded=store.readConfig()
        originalConfig=org.json.JSONObject(loaded.getJSONObject("config").toString())
        val config=org.json.JSONObject(originalConfig.toString())
        config.getJSONObject("dictionary").put("system_table","tables/wubi86.dict").put("enable_user_dict",false)
        config.getJSONObject("basic").put("auto_commit_unique",false)
        config.put("prediction",org.json.JSONObject().put("enabled",false))
        config.put("shortcuts",org.json.JSONObject())
        NativeHandle(loaded.getLong("version")).use { NativeHandle(store.saveConfig(config,it.value).getLong("version")).close() }
    }
    @org.junit.After fun restoreConfig() {
        val store=EngineStore(InstrumentationRegistry.getInstrumentation().targetContext)
        originalConfig?.let { config ->
            val loaded=store.readConfig()
            NativeHandle(loaded.getLong("version")).use { NativeHandle(store.saveConfig(config,it.value).getLong("version")).close() }
        }
    }
    private fun awaitHostText(device: UiDevice, text: String) {
        val deadline=SystemClock.uptimeMillis()+10000
        do {
            InstrumentationRegistry.getInstrumentation().uiAutomation.clearCache()
            val editor = device.findObject(By.clazz("android.widget.EditText"))
            if (editor != null && editor.text.orEmpty() == text) return
            SystemClock.sleep(100)
        } while(SystemClock.uptimeMillis()<deadline)
        throw AssertionError("Expected Settings text: $text")
    }

    private fun freshKey(device: UiDevice, selector: androidx.test.uiautomator.BySelector): androidx.test.uiautomator.UiObject2 {
        val deadline=SystemClock.uptimeMillis()+10000
        val keySelector = selector.pkg("org.mywubi").clickable(true)
        do {
            InstrumentationRegistry.getInstrumentation().uiAutomation.clearCache()
            device.findObject(keySelector)?.let {return it}
            SystemClock.sleep(100)
        } while(SystemClock.uptimeMillis()<deadline)
        throw AssertionError("Missing IME key: $selector")
    }

    @get:org.junit.Rule val failureEvidence = object : org.junit.rules.TestWatcher() {
        override fun failed(error: Throwable, description: org.junit.runner.Description) {
            val instrumentation=InstrumentationRegistry.getInstrumentation()
            val device=UiDevice.getInstance(instrumentation)
            device.takeScreenshot(java.io.File(instrumentation.targetContext.filesDir,"external-failure.png"))
            device.dumpWindowHierarchy(java.io.File(instrumentation.targetContext.filesDir,"external-failure.xml"))
        }
    }
    @Test fun composeExpansionAndRepeatStopsOnRelease() {
        val device=UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
        device.executeShellCommand("ime enable org.mywubi/.MyWubiService")
        device.executeShellCommand("ime set org.mywubi/.MyWubiService")
        if (InstrumentationRegistry.getArguments().getString("prediction_host") == "editor") {
            device.executeShellCommand("am force-stop org.mywubi.acceptancehost")
            device.executeShellCommand("am start -W -n org.mywubi.acceptancehost/.EditorActivity")
        } else {
            device.executeShellCommand("am force-stop com.android.settings")
            device.executeShellCommand("am force-stop com.google.android.settings.intelligence")
            device.executeShellCommand("am start -a android.settings.SETTINGS")
            device.wait(Until.findObject(By.res("com.android.settings","search_action_bar")),15000).click()
        }
        val field=device.wait(Until.findObject(By.clazz("android.widget.EditText")),15000)
        field.text="";field.click()
        fun button(label:String) = freshKey(device, By.desc(label))
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val keyBounds = button("g").visibleBounds
        device.findObject(By.desc("英 → 中").pkg("org.mywubi"))?.click()
        device.waitForIdle()
        val density = context.resources.displayMetrics.density
        java.io.File(context.filesDir,"keyboard-target.json").writeText(
            org.json.JSONObject().put("key", "g").put("width_dp", keyBounds.width()/density)
                .put("height_dp", keyBounds.height()/density).toString())
        button("g").click()
        awaitHostText(device,"g")
        button("展开").click()
        val candidate = device.wait(Until.findObject(By.descStartsWith("候选 一").pkg("org.mywubi")),5000)
        assertNotNull(candidate)
        val firstWord = freshKey(device, By.descStartsWith("候选 一")).visibleCenter
        device.click(firstWord.x, firstWord.y)
        awaitHostText(device,"一")
        // A successful commit clears composition and collapses the candidate grid.
        assertTrue(device.wait(Until.gone(By.descStartsWith("候选 一").pkg("org.mywubi")),5000))
        device.waitForIdle()
        button("小写").click();device.waitForIdle()
        assertNotNull(button("Q"))
        button("大写 · 本次").click();device.waitForIdle()
        button("大写 · 锁定").click();device.waitForIdle()
        field.text="abcdefghijklmnopqrstuvxyz"
        device.pressKeyCode(android.view.KeyEvent.KEYCODE_MOVE_END);device.waitForIdle()
        val backspace=button("退格")
        val p=backspace.visibleCenter
        device.swipe(p.x,p.y,p.x,p.y,180)
        SystemClock.sleep(400)
        val after=field.text.orEmpty()
        assertTrue("repeat removes multiple characters", after.length < 22)
        SystemClock.sleep(500)
        assertEquals("repeat stops after release",after,field.text.orEmpty())
        field.text="abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz"
        device.pressKeyCode(android.view.KeyEvent.KEYCODE_MOVE_END);device.waitForIdle()
        val held=button("退格").visibleCenter
        var downTime=SystemClock.uptimeMillis()
        fun touch(action:Int) {
            val event=android.view.MotionEvent.obtain(downTime,SystemClock.uptimeMillis(),action,held.x.toFloat(),held.y.toFloat(),0)
            event.source=android.view.InputDevice.SOURCE_TOUCHSCREEN
            try {InstrumentationRegistry.getInstrumentation().uiAutomation.injectInputEvent(event,true)} finally {event.recycle()}
        }
        val beforeCancel = field.text
        touch(android.view.MotionEvent.ACTION_DOWN)
        SystemClock.sleep(100)
        touch(android.view.MotionEvent.ACTION_CANCEL)
        SystemClock.sleep(450)
        assertEquals("cancelled press does not delete", beforeCancel, field.text)
        downTime=SystemClock.uptimeMillis()
        touch(android.view.MotionEvent.ACTION_DOWN)
        try {
            SystemClock.sleep(650)
            device.pressBack()
            SystemClock.sleep(200)
            val hiddenText=field.text
            SystemClock.sleep(600)
            assertEquals("hiding cancels a held repeat",hiddenText,field.text)
        } finally {touch(android.view.MotionEvent.ACTION_UP)}
        device.pressBack();device.pressBack()
    }

    @Test fun shapeInputInSystemSettings() {
        val device=UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
        device.executeShellCommand("ime enable org.mywubi/.MyWubiService")
        device.executeShellCommand("ime set org.mywubi/.MyWubiService")
        device.executeShellCommand("am start -W -f 0x10008000 -n org.mywubi/.MainActivity")
        assertTrue(device.wait(Until.hasObject(By.pkg("org.mywubi").scrollable(true)),15000))
        androidx.test.uiautomator.UiScrollable(androidx.test.uiautomator.UiSelector().scrollable(true)).setAsVerticalList().scrollToBeginning(20)
        device.wait(Until.findObject(By.text("常用")),5000)?.click()
        assertTrue(device.wait(Until.hasObject(By.text("MyWubi 已就绪")),15000))
        device.pressBack()
        if (InstrumentationRegistry.getArguments().getString("prediction_host")=="editor") {
            device.executeShellCommand("am force-stop org.mywubi.acceptancehost")
            device.executeShellCommand("am start -W -n org.mywubi.acceptancehost/.EditorActivity")
        } else {
            device.executeShellCommand("am force-stop com.android.settings")
            device.executeShellCommand("am force-stop com.google.android.settings.intelligence")
            device.executeShellCommand("am start -a android.settings.SETTINGS")
            val search=device.wait(Until.findObject(By.res("com.android.settings","search_action_bar")),15000)
            assertNotNull("Settings search entry",search);search.click()
        }
        val field=device.wait(Until.findObject(By.clazz("android.widget.EditText")),15000)
        assertNotNull("Settings editor",field);field.text="";field.click()
        freshKey(device, By.desc("q"))
        val switcher = device.wait(Until.findObject(By.res("android", "input_method_nav_ime_switcher")),5000)
        // ColorOS exposes its navigation bar as a WindowManager inset even
        // when it omits the AOSP switch-button accessibility resource.
        val navigationBar = Regex("""type=navigationBars frame=\[(\d+),(\d+)\]\[(\d+),(\d+)\] visible=true""")
        val switcherTop = switcher?.visibleBounds?.top ?: navigationBar
            .findAll(device.executeShellCommand("dumpsys window displays"))
            .map { m -> m.groupValues.drop(1).map(String::toInt) }
            .filter { (left, top, right, bottom) -> top > 0 && right-left > bottom-top }
            .minOfOrNull { it[1] }
        val action = if (InstrumentationRegistry.getArguments().getString("prediction_host")=="editor") "换行" else "搜索"
        val bottomKey = freshKey(device, By.desc(action)).visibleBounds
        val safeGap = 32 * InstrumentationRegistry.getInstrumentation().targetContext.resources.displayMetrics.density

        device.takeScreenshot(java.io.File(InstrumentationRegistry.getInstrumentation().targetContext.filesDir,"keyboard-system-clearance.png"))
        device.dumpWindowHierarchy(java.io.File(InstrumentationRegistry.getInstrumentation().targetContext.filesDir,"keyboard-system-clearance.xml"))
        val emptyText = field.text.orEmpty()
        fun key(label:String) {
            val selector = if(label == "一") By.descStartsWith("候选 一") else By.desc(label)
            val k=freshKey(device,selector)
            assertNotNull("IME key $label",k);k.click();device.waitForIdle()
        }
        key("g");key("g");key("l");key("l")
        awaitHostText(device,"ggll")
        key("一")
        awaitHostText(device,"一")
        key("退格")
        awaitHostText(device,emptyText)
        device.pressBack();device.pressBack()
        assertNotNull("Host commit/delete passed; system did not expose the IME switch button for clearance measurement", switcherTop)
        assertTrue("Bottom keys must leave a safe band above the system IME controls", switcherTop!! - bottomKey.bottom >= safeGap)
    }
}
