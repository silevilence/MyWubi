package org.mywubi

import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.junit.Assert.*
import org.junit.Test
import java.io.File

class LibraryUiTest {
    @Test fun metadataUsesLabeledRadioChoicesAndAuxiliaryPickers() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val device = UiDevice.getInstance(instrumentation)
        ActivityScenario.launch(LibraryActivity::class.java).use {
            device.wait(Until.findObject(By.text("请选择").enabled(true)),10000).click()
            device.wait(Until.findObject(By.textStartsWith("当前码表：")),5000).click()
            assertTrue(device.wait(Until.hasObject(By.text("码表类型")),15000))
            val shape = device.findObject(By.text("形码"))
            assertNotNull(shape)
            assertFalse(device.hasObject(By.textContains("· 已选")))
            device.takeScreenshot(File(instrumentation.targetContext.filesDir,"library-metadata.png"))
            UiScrollable(UiSelector().scrollable(true)).scrollTextIntoView("拼音辅助码表")
            assertTrue(device.hasObject(By.text("拼音辅助码表")))
            assertFalse(device.hasObject(By.text("桌面临时入口键")))
            val pinyin = device.wait(Until.findObject(By.desc("拼音辅助码表")),5000)
            assertNotNull("引用应显示码表名称", pinyin)
            pinyin.click()
            assertTrue(device.wait(Until.hasObject(By.text("不引用")),5000))
            assertTrue(device.hasObject(By.text("内置拼音")))
            assertTrue(device.hasObject(By.text("内置英文")))
            device.takeScreenshot(File(instrumentation.targetContext.filesDir,"library-reference-picker.png"))
            device.pressBack()
        }
    }
}
