package org.mywubi

import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.junit.Assert.*
import org.junit.Test

class SettingsNavigationTest {
    private val ins = InstrumentationRegistry.getInstrumentation()
    private val device = UiDevice.getInstance(ins)

    private fun find(selector: BySelector): UiObject2 {
        val deadline = android.os.SystemClock.uptimeMillis() + 10000
        do {
            ins.uiAutomation.clearCache()
            device.findObject(selector)?.let { return it }
            android.os.SystemClock.sleep(100)
        } while (android.os.SystemClock.uptimeMillis() < deadline)
        error("Missing $selector")
    }

    private fun scrollToText(text: String): UiObject2 {
        for (down in listOf(true, false)) repeat(12) {
            device.waitForIdle()
            ins.waitForIdleSync()
            ins.uiAutomation.clearCache()
            val content = device.findObject(By.desc("设置内容"))?.visibleBounds
            device.findObject(By.text(text))?.let {
                val bounds = it.visibleBounds
                if (bounds.height() > 0 && content?.contains(bounds) == true) return it
            }
            val top = device.displayHeight / 3
            val bottom = device.displayHeight * 3 / 4
            device.swipe(device.displayWidth / 2, if (down) bottom else top,
                device.displayWidth / 2, if (down) top else bottom, 25)
            device.waitForIdle()
        }
        error("Text is not visible inside the settings viewport: $text")
    }

    @Test fun categoriesKeepDraftAndSaveVisibleAfterScrollAndRotation() {
        Configurator.getInstance().waitForIdleTimeout = 500
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            var baseline = false
            val toggle = device.wait(Until.findObject(By.desc("唯一候选自动上屏")), 10000) ?: error("missing setting")
            scenario.onActivity {
                val model = ViewModelProvider(it)[SettingsModel::class.java]
                baseline = model.config?.getJSONObject("basic")?.getBoolean("auto_commit_unique") ?: false
            }
            toggle.click()
            assertTrue(device.wait(Until.hasObject(By.text("有未保存更改 · 草稿会保留")), 5000))
            for (tab in listOf("键盘", "码表", "预测")) {
                device.findObject(By.text(tab).selected(false)).click()
                device.waitForIdle()
                assertTrue(device.findObject(By.text("保存设置")).isEnabled)
                device.swipe(device.displayWidth / 2, device.displayHeight * 2 / 3, device.displayWidth / 2, device.displayHeight / 3, 25)
                device.waitForIdle()
                assertTrue(device.findObject(By.text("保存设置")).isEnabled)
            }
            device.takeScreenshot(java.io.File(ins.targetContext.filesDir, "settings-prediction.png"))
            try {
                device.setOrientationLeft(); device.waitForIdle()
                assertTrue(device.wait(Until.hasObject(By.text("保存设置")), 5000))
                assertTrue(device.hasObject(By.text("有未保存更改 · 草稿会保留")))
            } finally { device.setOrientationNatural(); device.unfreezeRotation(); device.waitForIdle() }
            find(By.text("常用")).click()
            device.waitForIdle()
            scenario.onActivity {
                val model = ViewModelProvider(it)[SettingsModel::class.java]
                assertEquals(!baseline, model.config!!.getJSONObject("basic").getBoolean("auto_commit_unique"))
            }
            device.takeScreenshot(java.io.File(ins.targetContext.filesDir, "settings-common.png"))
            // Discard the test draft without touching the user's saved configuration.
            find(By.text("重新载入")).click()
            find(By.text("重新载入磁盘设置？"))
            find(By.text("重新载入")).click()
            assertTrue(device.wait(Until.hasObject(By.text("没有未保存更改")), 5000))
        }
    }

    @Test fun standardPackageExplainsModelLimitAndOffersWorkingDictionaryPath() {
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            device.wait(Until.findObject(By.text("预测")), 10000).click()
            scrollToText("预测模型与资源（实验性）").click()
            scrollToText("当前版本仅支持词典预测")
            assertTrue(device.hasObject(By.text("当前版本仅支持词典预测")))
            assertTrue(device.hasObject(By.text("保存设置")))
            device.takeScreenshot(java.io.File(ins.targetContext.filesDir, "settings-model-unavailable.png"))
            scrollToText("选用词典预测").click()
            device.waitForIdle()
            scenario.onActivity {
                val model = ViewModelProvider(it)[SettingsModel::class.java]
                assertTrue(model.config!!.getJSONObject("prediction").getBoolean("enabled"))
                assertTrue(model.config!!.getJSONObject("prediction").isNull("model"))
                model.reload(true)
            }
        }
    }
}
