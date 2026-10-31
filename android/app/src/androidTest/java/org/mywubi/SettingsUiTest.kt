package org.mywubi

import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import android.os.SystemClock
import org.junit.Assert.*
import org.junit.Test

class SettingsUiTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val device = UiDevice.getInstance(instrumentation)
    private fun text(value: String): UiObject2 {
        instrumentation.uiAutomation.clearCache()
        device.findObject(By.text(value))?.let { return it }
        UiScrollable(UiSelector().scrollable(true)).setAsVerticalList().scrollToBeginning(20)
        repeat(20) {
            instrumentation.uiAutomation.clearCache()
            device.findObject(By.text(value))?.let { return it }
            device.swipe(device.displayWidth / 2, device.displayHeight * 4 / 5,
                device.displayWidth / 2, device.displayHeight / 3, 30)
            device.waitForIdle()
        }
        device.takeScreenshot(java.io.File(instrumentation.targetContext.filesDir, "settings-failure.png"))
        device.dumpWindowHierarchy(java.io.File(instrumentation.targetContext.filesDir, "settings-failure.xml"))
        error("Missing $value")
    }
    @Test fun draftSurvivesRotationExitAndSafCancellation() {
        device.executeShellCommand("am start -n org.mywubi/.MainActivity")
        val toggle = device.wait(Until.findObject(By.desc("唯一候选自动上屏")), 10000)
        assertNotNull(toggle); toggle.click()
        text("有未保存更改 · 草稿会保留")
        try {
            device.setOrientationLeft(); device.waitForIdle()
            text("有未保存更改 · 草稿会保留")
        } finally { device.setOrientationNatural(); device.unfreezeRotation() }
        device.pressBack(); text("保留并退出").click()
        device.executeShellCommand("am start -n org.mywubi/.MainActivity")
        text("已恢复未保存草稿")
        text("选择码表文件").click()
        assertTrue(device.wait(Until.hasObject(By.pkg("com.google.android.documentsui")), 10000) || device.hasObject(By.pkg("com.android.documentsui")))
        device.pressBack()
        text("有未保存更改 · 草稿会保留")
        text("重新载入").click()
        text("重新载入磁盘设置？")
        text("重新载入").click()
        text("没有未保存更改")
        device.takeScreenshot(java.io.File(instrumentation.targetContext.filesDir, "settings.png"))
    }

    @Test fun safImportsExplicitMainAndAuxiliaryAndApplies() {
        val store = EngineStore(instrumentation.targetContext)
        val original = store.readConfig()
        NativeHandle(original.getLong("version")).close()
        try {
            device.executeShellCommand("am start -n org.mywubi/.MainActivity")
            text("选择码表文件").click()
            device.wait(Until.findObject(By.desc("Show roots")), 5000)?.click()
            device.wait(Until.findObject(By.text("Downloads")), 5000)?.click()
            val main = device.wait(Until.findObject(By.text("CP19-main.dict")), 10000) ?: error("Missing SAF fixture")
            main.longClick()
            device.wait(Until.findObject(By.text("CP19-pinyin.dict")), 10000).click()
            (device.findObject(By.res("com.google.android.documentsui", "action_menu_select"))
                ?: device.findObject(By.res("com.android.documentsui", "action_menu_select"))
                ?: device.findObject(By.text("OPEN")) ?: error("Missing SAF confirm")).click()
            text("主码表（必选）：未选择").click()
            device.wait(Until.findObject(By.textContains("CP19-main.dict")), 10000).click()
            text("拼音辅助表：未选择").click()
            device.wait(Until.findObject(By.textContains("CP19-pinyin.dict")), 10000).click()
            text("校验并启用码表包").click()
            text("码表包已保存；等待输入法应用")
            val activated = store.readConfig()
            NativeHandle(activated.getLong("version")).close()
            assertTrue(activated.getJSONObject("config").getJSONObject("dictionary").getString("system_table").contains("packages/"))
            NativeHandle(NativeBridge.objectCall("BuildRuntime", "path" to store.configFile.path).getLong("handle")).close()
        } finally {
            val latest = store.readConfig()
            NativeHandle(latest.getLong("version")).use { expected ->
                val saved = store.saveConfig(original.getJSONObject("config"), expected.value)
                NativeHandle(saved.getLong("version")).close()
            }
        }
    }
}
