package org.mywubi

import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import android.os.SystemClock
import org.junit.Assert.*
import org.junit.Test

class SettingsUiTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val device = UiDevice.getInstance(instrumentation)
    @get:org.junit.Rule val failureEvidence = object : org.junit.rules.TestWatcher() {
        override fun failed(error: Throwable, description: org.junit.runner.Description) {
            device.takeScreenshot(java.io.File(instrumentation.targetContext.filesDir, "settings-failure.png"))
            device.dumpWindowHierarchy(java.io.File(instrumentation.targetContext.filesDir, "settings-failure.xml"))
        }
    }
    private fun text(value: String): UiObject2 {
        instrumentation.uiAutomation.clearCache()
        device.wait(Until.findObject(By.text(value)), 1500)?.let { return it }
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
    private fun chooseRole(label: String, filename: String) {
        text(label) // Scroll the role into view before waiting for layout to settle.
        device.waitForIdle()
        instrumentation.uiAutomation.clearCache()
        device.findObject(By.text(label)).click()
        device.waitForIdle()
        val choice = By.text(java.util.regex.Pattern.compile("\\d+\\. " + java.util.regex.Pattern.quote(filename)))
        device.wait(Until.findObject(choice), 10000).click()
        device.waitForIdle()
    }
    private fun clickDocumentRoot() {
        val selector = By.res("android", "title").text("Downloads")
        val deadline = SystemClock.uptimeMillis() + 10000
        while (SystemClock.uptimeMillis() < deadline) {
            instrumentation.uiAutomation.clearCache()
            try {
                device.findObject(selector)?.let { it.click(); device.waitForIdle(); return }
            } catch (_: StaleObjectException) {
                // DocumentsUI can replace the drawer node while providers load.
            }
            SystemClock.sleep(100)
        }
        error("Downloads root did not become available")
    }
    private fun selectImported(store: EngineStore, original: org.json.JSONObject, name: String) {
        text("已导入；当前码表未改变，请选择码表后保存设置")
        val read = store.readConfig()
        NativeHandle(read.getLong("version")).close()
        val before = original.getJSONObject("config").getJSONObject("dictionary").getString("system_table")
        assertEquals(before, read.getJSONObject("config").getJSONObject("dictionary").getString("system_table"))
        text("选择当前码表")
        device.waitForIdle(); instrumentation.uiAutomation.clearCache()
        device.wait(Until.findObject(By.desc("选择当前码表")),5000).click()
        device.waitForIdle()
        device.wait(Until.findObject(By.text(name)),5000).click()
        text("保存设置").click()
        text("已保存；输入法启动或重载后应用")
    }
    @Test fun draftSurvivesRotationExitAndSafCancellation() {
        device.executeShellCommand("am start -W -f 0x10008000 -n org.mywubi/.MainActivity")
        text("唯一候选自动上屏")
        device.waitForIdle(); instrumentation.uiAutomation.clearCache()
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
        text("从手机导入码表").click()
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
            device.executeShellCommand("am start -W -f 0x10008000 -n org.mywubi/.MainActivity")
            text("从手机导入码表").click()
            device.waitForIdle()
            device.wait(Until.findObject(By.desc("Show roots")), 5000)?.click()
            device.waitForIdle()
            // The toolbar and breadcrumb can also say Downloads. Select the
            // drawer's root row, not whichever matching text is returned first.
            clickDocumentRoot()
            val main = device.wait(Until.findObject(By.text("CP19-main.dict")), 10000) ?: error("Missing SAF fixture")
            main.longClick()
            device.wait(Until.findObject(By.text("CP19-pinyin.dict")), 10000).click()
            (device.findObject(By.res("com.google.android.documentsui", "action_menu_select"))
                ?: device.findObject(By.res("com.android.documentsui", "action_menu_select"))
                ?: device.findObject(By.text("OPEN")) ?: error("Missing SAF confirm")).click()
            chooseRole("主码表（必选）：未选择", "CP19-main.dict")
            chooseRole("拼音辅助表：未选择", "CP19-pinyin.dict")
            text("校验并导入").click()
            selectImported(store, original, "CP19-main.dict")
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

    @Test fun safSingleFileAutomaticallySelectsMainTable() {
        val store = EngineStore(instrumentation.targetContext)
        val original = store.readConfig()
        NativeHandle(original.getLong("version")).close()
        try {
            device.executeShellCommand("am start -W -f 0x10008000 -n org.mywubi/.MainActivity")
            text("从手机导入码表").click()
            device.wait(Until.findObject(By.desc("Show roots")), 5000)?.click()
            device.waitForIdle()
            clickDocumentRoot()
            device.waitForIdle()
            device.wait(Until.findObject(By.text("Standalone-main.dict")), 10000).click()
            text("主码表（必选）：Standalone-main.dict")
            text("校验并导入").click()
            selectImported(store, original, "Standalone-main.dict")
            text("当前码表：Standalone-main.dict")
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
