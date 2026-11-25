package org.mywubi

import android.os.SystemClock
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test

/** Explicit isolated-package acceptance; may fetch the pinned AAR when no cache is provisioned. */
class OnnxComponentSettingsTest {
    @Test fun acquireSelectAndSave() {
        val ins = InstrumentationRegistry.getInstrumentation()
        val app = ins.targetContext
        assumeTrue(app.packageName == "org.mywubi.miaccept")
        val device = UiDevice.getInstance(ins)
        Configurator.getInstance().waitForIdleTimeout = 400
        val store = EngineStore(app)
        val old = store.readConfig()
        NativeHandle(old.getLong("version")).close()
        val original = old.getJSONObject("config")
        fun savedPrediction() = store.readConfig().let {
            NativeHandle(it.getLong("version")).close()
            it.getJSONObject("config").getJSONObject("prediction")
        }
        try {
            val catalog = ModelCatalog(app)
            val entries = catalog.call("List")
            if ((0 until entries.length()).none { entries.getJSONObject(it).getString("id") == "instrumented-counts" }) {
                catalog.call("Import", "source" to ModelBackendTest().fixture().absolutePath)
            }
            device.executeShellCommand("am start -W -f 0x10008000 -n ${app.packageName}/org.mywubi.MainActivity")
            selectSettingsTab(device, "预测")
            expandSettingsControl(device, "预测模型与资源（实验性）", modelRoot = true)
            expandSettingsControl(device, "导入与选择模型")
            val choice = "选择模型 instrumented-counts @ v1"
            if (!OnnxComponent.location(app).isFile) {
                assertFalse(findSettingsControl(device, By.desc(choice)).actionNode().isEnabled)
                clickSetting(device, "获取 ONNX 运行组件")
                val end = SystemClock.elapsedRealtime() + 20 * 60_000
                while (OnnxRuntime.status.value != OnnxRuntime.State.Ready && SystemClock.elapsedRealtime() < end) {
                    val transfer = OnnxTransfers.status.value
                    assertNotEquals(transfer.toString(), "failed", transfer.phase)
                    SystemClock.sleep(100)
                }
            }
            assertEquals(OnnxRuntime.State.Ready, OnnxRuntime.status.value)
            assertTrue(savedPrediction().isNull("model"))
            assertFalse(savedPrediction().getBoolean("enabled"))
            clickSetting(device, choice, description = true)
            clickSetting(device, "上屏后显示字词预测（实验性）", description = true)
            clickSetting(device, "保存设置")
            findSettingsControl(device, By.text("没有未保存更改"))
            val saved = savedPrediction()
            assertTrue(saved.getBoolean("enabled"))
            assertEquals("instrumented-counts", saved.getJSONObject("model").getString("id"))
        } finally {
            val version = store.readConfig().getLong("version")
            NativeHandle(version).use { NativeHandle(store.saveConfig(original, it.value).getLong("version")).close() }
            ModelCatalog(app).call("Remove", "selection" to org.json.JSONObject().put("id", "instrumented-counts").put("version", "v1"))
            device.pressBack()
        }
    }

    @Test fun switchBackendsReopenAndTrialOnnx() {
        val ins = InstrumentationRegistry.getInstrumentation()
        val app = ins.targetContext
        assumeTrue(app.packageName == "org.mywubi.miaccept")
        val device = UiDevice.getInstance(ins)
        Configurator.getInstance().waitForIdleTimeout = 400
        val catalog = ModelCatalog(app)
        val source = java.io.File(app.filesDir, "mi-test-package")
        assumeTrue(source.isDirectory)
        fun importFixture(sourceDir: java.io.File) {
            val manifest = org.json.JSONObject(java.io.File(sourceDir, "manifest.json").readText())
            val current = catalog.call("List")
            if ((0 until current.length()).none { current.getJSONObject(it).getString("id") == manifest.getString("id") }) {
                catalog.call("Import", "source" to sourceDir.absolutePath)
            }
        }
        importFixture(source)
        importFixture(ModelBackendTest().fixture())
        val store = EngineStore(app)
        val original = store.readConfig().also { NativeHandle(it.getLong("version")).close() }.getJSONObject("config")
        fun open() {
            device.executeShellCommand("am start -W -f 0x10008000 -n ${app.packageName}/org.mywubi.MainActivity")
            selectSettingsTab(device, "预测")
            expandSettingsControl(device, "预测模型与资源（实验性）", modelRoot = true)
        }
        fun select(id: String) {
            clickSetting(device, "选择模型 $id @ v1", description = true)
            clickSetting(device, "保存设置")
            findSettingsControl(device, By.text("没有未保存更改"))
            val saved = store.readConfig()
            NativeHandle(saved.getLong("version")).close()
            assertEquals(id, saved.getJSONObject("config").getJSONObject("prediction").getJSONObject("model").getString("id"))
        }
        try {
            open()
            expandSettingsControl(device, "导入与选择模型")
            select("qwen25-score")
            select("instrumented-counts")
            device.pressBack()
            open()
            expandSettingsControl(device, "导入与选择模型")
            findSettingsControl(device, By.text("当前选择：instrumented-counts @ v1"))
            findSettingsControl(device, By.text("导入与选择模型")).actionNode().click()
            val trialPanel = findSettingsControl(device, By.text("自定义模型指南与试运行（实验性）")).actionNode()
            if (!trialPanel.hasObject(By.text("收起"))) trialPanel.click()
            clickSetting(device, "试运行模型", description = true)
            findSettingsControl(device, By.text("instrumented-counts--v1")).click()
            clickSetting(device, "试运行（1000 ms，4 线程）")
            val result = findSettingsControl(device, By.desc("模型试运行结果")).text
            assertTrue(result, result.contains("候选：") && result.contains("天气"))
        } finally {
            NativeHandle(store.readConfig().getLong("version")).use {
                NativeHandle(store.saveConfig(original, it.value).getLong("version")).close()
            }
            device.pressBack()
        }
    }
}
