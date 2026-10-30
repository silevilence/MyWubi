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
    private fun awaitHostText(device: UiDevice, text: String) {
        val deadline=SystemClock.uptimeMillis()+10000
        do {
            InstrumentationRegistry.getInstrumentation().uiAutomation.clearCache()
            if(device.hasObject(By.clazz("android.widget.EditText").text(text))) return
            SystemClock.sleep(100)
        } while(SystemClock.uptimeMillis()<deadline)
        throw AssertionError("Expected Settings text: $text")
    }

    private fun freshKey(device: UiDevice, selector: androidx.test.uiautomator.BySelector): androidx.test.uiautomator.UiObject2 {
        val deadline=SystemClock.uptimeMillis()+10000
        do {
            InstrumentationRegistry.getInstrumentation().uiAutomation.clearCache()
            device.findObject(selector.pkg("org.mywubi").clickable(true))?.let {return it}
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
        device.executeShellCommand("am force-stop com.android.settings")
        device.executeShellCommand("am force-stop com.google.android.settings.intelligence")
        device.executeShellCommand("am start -a android.settings.SETTINGS")
        device.wait(Until.findObject(By.res("com.android.settings","search_action_bar")),15000).click()
        val field=device.wait(Until.findObject(By.clazz("android.widget.EditText")),15000)
        field.text="";field.click()
        fun button(label:String) = freshKey(device, By.desc(label))
        button("g").click()
        awaitHostText(device,"g")
        button("展开").click()
        val candidate = device.wait(Until.findObject(By.descStartsWith("候选 王").pkg("org.mywubi")),5000)
        assertNotNull(candidate);candidate.click()
        awaitHostText(device,"王")
        button("收起").click();device.waitForIdle()
        button("小写").click();device.waitForIdle()
        assertNotNull(button("Q"))
        button("大写").click();device.waitForIdle()
        field.text="abcdefghijklmnopqrstuvxyz"
        device.pressKeyCode(android.view.KeyEvent.KEYCODE_MOVE_END);device.waitForIdle()
        val backspace=button("退格")
        val p=backspace.visibleCenter
        device.swipe(p.x,p.y,p.x,p.y,180)
        SystemClock.sleep(400)
        val after=field.text
        assertTrue("repeat removes multiple characters", after.length < 22)
        SystemClock.sleep(500)
        assertEquals("repeat stops after release",after,field.text)
        field.text="abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz"
        device.pressKeyCode(android.view.KeyEvent.KEYCODE_MOVE_END);device.waitForIdle()
        val held=button("退格").visibleCenter
        val downTime=SystemClock.uptimeMillis()
        fun touch(action:Int) {
            val event=android.view.MotionEvent.obtain(downTime,SystemClock.uptimeMillis(),action,held.x.toFloat(),held.y.toFloat(),0)
            event.source=android.view.InputDevice.SOURCE_TOUCHSCREEN
            try {InstrumentationRegistry.getInstrumentation().uiAutomation.injectInputEvent(event,true)} finally {event.recycle()}
        }
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
        device.executeShellCommand("am start -n org.mywubi/.MainActivity")
        assertTrue(device.wait(Until.hasObject(By.text("1. 启用 MyWubi")),15000))
        device.pressBack()
        device.executeShellCommand("am force-stop com.android.settings")
        device.executeShellCommand("am force-stop com.google.android.settings.intelligence")
        device.executeShellCommand("am start -a android.settings.SETTINGS")
        val search=device.wait(Until.findObject(By.res("com.android.settings","search_action_bar")),15000)
        assertNotNull("Settings search entry",search);search.click()
        val field=device.wait(Until.findObject(By.clazz("android.widget.EditText")),15000)
        assertNotNull("Settings editor",field);field.text="";field.click()
        val emptyText = field.text
        fun key(label:String) {
            val selector = if(label == "王") By.descStartsWith("候选 王") else By.desc(label)
            val k=freshKey(device,selector)
            assertNotNull("IME key $label",k);k.click();device.waitForIdle()
        }
        key("g");key("g");key("l");key("l")
        awaitHostText(device,"ggll")
        key("王")
        awaitHostText(device,"王")
        key("退格")
        awaitHostText(device,emptyText)
        device.pressBack();device.pressBack()
    }
}
