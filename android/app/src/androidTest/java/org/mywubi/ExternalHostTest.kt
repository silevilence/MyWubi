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
        fun button(label:String): androidx.test.uiautomator.UiObject2 {
            val deadline=SystemClock.uptimeMillis()+10000
            do {
                InstrumentationRegistry.getInstrumentation().uiAutomation.clearCache()
                device.findObject(By.desc(label).pkg("org.mywubi").clickable(true))?.let {return it}
                SystemClock.sleep(100)
            } while(SystemClock.uptimeMillis()<deadline)
            throw AssertionError("Missing IME key $label")
        }
        button("g").click()
        assertTrue(device.wait(Until.hasObject(By.clazz("android.widget.EditText").text("g")),5000))
        button("展开").click()
        val candidate = device.wait(Until.findObject(By.descStartsWith("候选 王").pkg("org.mywubi")),5000)
        assertNotNull(candidate);candidate.click()
        assertTrue(device.wait(Until.hasObject(By.clazz("android.widget.EditText").text("王")),5000))
        button("收起").click();device.waitForIdle()
        button("小写").click();device.waitForIdle()
        assertNotNull(button("Q"))
        button("大写").click();device.waitForIdle()
        field.text="abcdefghijklmnopqrstuvxyz";field.click()
        val backspace=device.wait(Until.findObject(By.desc("退格").pkg("org.mywubi").clickable(true)),5000)
        val p=backspace.visibleCenter
        device.swipe(p.x,p.y,p.x,p.y,180)
        SystemClock.sleep(400)
        val after=field.text
        assertTrue("repeat removes multiple characters", after.length < 22)
        SystemClock.sleep(500)
        assertEquals("repeat stops after release",after,field.text)
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
            val k=device.wait(Until.findObject(selector.pkg("org.mywubi").clickable(true)),15000)
            assertNotNull("IME key $label",k);k.click();device.waitForIdle()
        }
        key("g");key("g");key("l");key("l")
        assertTrue(device.wait(Until.hasObject(By.clazz("android.widget.EditText").text("ggll")),5000))
        key("王")
        assertTrue(device.wait(Until.hasObject(By.clazz("android.widget.EditText").text("王")),5000))
        key("退格")
        assertTrue(device.wait(Until.hasObject(By.clazz("android.widget.EditText").text(emptyText)),5000))
        device.pressBack();device.pressBack()
    }
}
