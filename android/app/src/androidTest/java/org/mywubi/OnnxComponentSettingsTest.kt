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
            ModelCatalog(app).call("Import", "source" to ModelBackendTest().fixture().absolutePath)
            device.executeShellCommand("am start -W -f 0x10008000 -n ${app.packageName}/org.mywubi.MainActivity")
            selectSettingsTab(device, "预测")
            expandSettingsControl(device, "预测模型与资源（实验性）", modelRoot = true)
            expandSettingsControl(device, "导入与选择模型")
            val choice = "选择模型 instrumented-counts @ v1"
            if (!OnnxComponent.location(app).isFile) {
                assertFalse(findSettingsControl(device, By.desc(choice)).actionNode().isEnabled)
                clickSetting(device, "获取 ONNX 运行组件")
                val end = SystemClock.elapsedRealtime() + 120000
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
            device.pressBack()
        }
    }
}
