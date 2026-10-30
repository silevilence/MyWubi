package org.mywubi

import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
import org.junit.Assert.*
import org.junit.Test

class ModeHostTest {
    @Test fun englishTemporaryPinyinAndDigitsInExternalHost() {
        val device=UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
        device.executeShellCommand("ime enable org.mywubi/.MyWubiService")
        device.executeShellCommand("ime set org.mywubi/.MyWubiService")
        device.executeShellCommand("am force-stop com.android.settings")
        device.executeShellCommand("am force-stop com.google.android.settings.intelligence")
        device.executeShellCommand("am start -a android.settings.SETTINGS")
        device.wait(Until.findObject(By.res("com.android.settings","search_action_bar")),15000).click()
        val field=device.wait(Until.findObject(By.clazz("android.widget.EditText")),15000)
        field.text="";field.click()
        fun key(label:String) {
            val deadline=android.os.SystemClock.uptimeMillis()+10000
            var node: androidx.test.uiautomator.UiObject2? = null
            while(node==null && android.os.SystemClock.uptimeMillis()<deadline) {
                InstrumentationRegistry.getInstrumentation().uiAutomation.clearCache()
                node=device.findObject(By.desc(label).pkg("org.mywubi").clickable(true).enabled(true))
                if(node==null) android.os.SystemClock.sleep(100)
            }
            assertNotNull("key $label",node);node!!.click();device.waitForIdle()
        }
        fun text(value:String) {for(c in value)key(c.toString())}
        fun expect(value:String) {assertTrue("host text $value",device.wait(Until.hasObject(By.clazz("android.widget.EditText").text(value)),5000))}
        if(device.hasObject(By.desc("英 → 中"))) key("英 → 中")
        key("中 → 英")
        text("hello");key("空格");expect("hello ")
        key("拼音");key("空格");key("回车");expect("hello ")
        text("ni");key("拼音 · 本次");expect("hello ni")
        InstrumentationRegistry.getInstrumentation().uiAutomation.clearCache()
        device.wait(Until.findObject(By.descStartsWith("候选 你，").pkg("org.mywubi")),3000)?.click()
            ?: device.wait(Until.findObject(By.desc("候选 你").pkg("org.mywubi")),5000).click()
        expect("hello 你")
        assertTrue(device.hasObject(By.desc("英 → 中")))
        text("hi");key("123");expect("hello 你hi");key("1");expect("hello 你hi1")
        key("ABC");text("hi");key("英 → 中");expect("hello 你hi1hi")
        text("ggll");key("123");expect("hello 你hi1higgll");key("1");expect("hello 你hi1hi王1")
        key("ABC");key("中 → 英")
        text("hello")
        key("候选 hello")
        expect("hello 你hi1hi王1hello")
        key("拼音");key("退格")
        assertTrue(device.hasObject(By.desc("英 → 中")))
        key("英 → 中")
        device.pressBack();device.pressBack()
    }
}
