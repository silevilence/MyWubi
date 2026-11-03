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
            InstrumentationRegistry.getInstrumentation().waitForIdleSync()
            InstrumentationRegistry.getInstrumentation().uiAutomation.clearCache()
        }
        fun text(value:String) {for(c in value)key(c.toString())}
        fun expect(value:String) {
            val deadline=android.os.SystemClock.uptimeMillis()+10000
            do {
                InstrumentationRegistry.getInstrumentation().uiAutomation.clearCache()
                if(device.hasObject(By.clazz("android.widget.EditText").text(value))) return
                android.os.SystemClock.sleep(100)
            } while(android.os.SystemClock.uptimeMillis()<deadline)
            throw AssertionError("Expected Settings text: $value")
        }
        if(device.hasObject(By.desc("英 → 中"))) key("英 → 中")
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val config = EngineStore(context).readConfig()
        NativeHandle(config.getLong("version")).close()
        val label = TableCatalog(context).label(config.getJSONObject("config").getJSONObject("dictionary").getString("system_table"))
        assertTrue("键盘应显示当前码表名称", device.wait(Until.hasObject(By.text(label)),5000))
        device.takeScreenshot(java.io.File(context.filesDir,"keyboard-current-table.png"))
        key("中 → 英")
        text("hello");key("空格");expect("hello ")
        key("拼音");key("空格");key("搜索");expect("hello ")
        text("ni");expect("hello ni")
        InstrumentationRegistry.getInstrumentation().uiAutomation.clearCache()
        device.wait(Until.findObject(By.descStartsWith("候选 你，").pkg("org.mywubi")),3000)?.click()
            ?: device.wait(Until.findObject(By.desc("候选 你").pkg("org.mywubi")),5000).click()
        expect("hello 你")
        assertTrue(device.hasObject(By.desc("英 → 中")))
        assertTrue(device.hasObject(By.desc("拼音")))
        key("拼音"); key("拼音 · 本次")
        device.takeScreenshot(java.io.File(context.filesDir,"keyboard-pinyin-locked.png"))
        repeat(2) { index ->
            text("ni")
            expect("hello 你" + "你".repeat(index) + "ni")
            InstrumentationRegistry.getInstrumentation().uiAutomation.clearCache()
            device.wait(Until.findObject(By.desc(java.util.regex.Pattern.compile("候选 你(?:，.*)?")).pkg("org.mywubi")),5000).click()
            expect("hello 你" + "你".repeat(index + 1))
            assertTrue("提交后保持拼音", device.hasObject(By.desc("拼音 · 锁定")))
        }
        key("拼音 · 锁定")
        assertTrue(device.hasObject(By.desc("拼音")))
        key("退格"); key("退格"); expect("hello 你")
        text("hi");key("123");expect("hello 你hi");key("1");expect("hello 你hi1")
        key("ABC");text("hi");key("英 → 中");expect("hello 你hi1hi")
        text("ggll");key("123");expect("hello 你hi1higgll");key("1");expect("hello 你hi1hi王1")
        key("ABC");key("中 → 英")
        text("hello")
        key("候选 hello")
        expect("hello 你hi1hi王1hello")
        key("拼音");key("退格")
        assertTrue(device.hasObject(By.desc("英 → 中")))
        val swipe = device.findObject(By.desc("w").pkg("org.mywubi")).visibleCenter
        device.swipe(swipe.x,swipe.y,swipe.x,swipe.y-(30*context.resources.displayMetrics.density).toInt(),8)
        expect("hello 你hi1hi王1hello2")
        val hold = device.findObject(By.desc("s").pkg("org.mywubi")).visibleCenter
        device.swipe(hold.x,hold.y,hold.x,hold.y,150)
        expect("hello 你hi1hi王1hello2!")
        key("英 → 中")
        device.pressBack();device.pressBack()
    }
}
