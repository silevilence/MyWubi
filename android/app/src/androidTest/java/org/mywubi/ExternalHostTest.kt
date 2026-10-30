package org.mywubi

import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
import org.junit.Assert.*
import org.junit.Test

/** Uses the installed system Settings editor, not an EditText owned by this app. */
class ExternalHostTest {
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
            val k=device.wait(Until.findObject(By.text(label).pkg("org.mywubi").clickable(true)),15000)
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
