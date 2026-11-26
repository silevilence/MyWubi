package org.mywubi

import android.os.Build
import android.os.SystemClock
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import java.io.File
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class NpuHardwareTest {
    private val ins = InstrumentationRegistry.getInstrumentation()
    private val app = ins.targetContext

    @Test fun fixedModelsAndSystemDrivers() {
        org.junit.Assume.assumeTrue("Use the isolated acceptance APK with local fixtures", app.packageName == "org.mywubi.npuaccept")
        val output = File(app.filesDir, "npu-validation.json")
        val report = JSONObject().put("model", Build.MODEL).put("system", Build.FINGERPRINT)
            .put("abi", NativeBridge.runtimeAbi()).put("runtime", OnnxComponent.version)
        try {
            fun awaitRuntime() {
                val end = SystemClock.elapsedRealtime() + 15000
                while (OnnxRuntime.status.value == OnnxRuntime.State.Checking && SystemClock.elapsedRealtime() < end) SystemClock.sleep(25)
            }
            awaitRuntime()
            val installed = OnnxComponent.installed(app)
            if (!installed.isFile) {
                assertEquals(OnnxRuntime.State.Missing, OnnxRuntime.status.value)
                val missing = NpuDetection.detect(app, OnnxRuntime.State.Missing)
                assertFalse(missing.getBoolean("backend"))
                report.put("missing", missing)
                installed.parentFile!!.mkdirs()
                File(app.getExternalFilesDir(null), OnnxComponent.library).copyTo(installed)
                OnnxRuntime.refresh(app)
            }
            val deadline = SystemClock.elapsedRealtime() + 15000
            while (OnnxRuntime.status.value != OnnxRuntime.State.Ready && SystemClock.elapsedRealtime() < deadline) SystemClock.sleep(25)
            assertEquals(OnnxRuntime.State.Ready, OnnxRuntime.status.value)
            val found = NpuDetection.detect(app, OnnxRuntime.State.Ready)
            assertEquals(found.toString(), NpuDetection.cached(app, OnnxRuntime.State.Ready).toString())
            report.put("found", found)
            val budget = JSONObject().put("threads", 2).put("wait_ms", 1000).put("max_candidates", 8)
            val counts = ModelBackendTest().fixture()
            val result = NpuValidation.run(app, counts, budget, found)
            report.put("scoring", result)
            if (result.optString("reason").startsWith("NNAPI_LOAD")) assertEquals("CPU", result.getString("actual"))
            assertTrue(result.toString(), result.getJSONObject("cpu").getJSONArray("words").getJSONArray(0).length() > 0)
            val gguf = File(app.filesDir, "mi-generation-package")
            val generation = NpuValidation.run(app, gguf, budget.put("max_candidates", 1), found)
            report.put("generation", generation)
            assertEquals("GGUF_CPU_ONLY", generation.getString("reason"))
            assertEquals("generation", generation.getString("task"))
        } finally { output.writeText(report.toString(2)) }
    }

    @Test fun settingsShowFixedSampleAndPrivateDiagnostics() {
        org.junit.Assume.assumeTrue(app.packageName == "org.mywubi.npuaccept")
        val store = EngineStore(app)
        val catalog = ModelCatalog(app)
        val identity = JSONObject("""{"id":"instrumented-counts","version":"v1"}""")
        val baseline = store.readConfig()
        val original = baseline.getJSONObject("config")
        val config = JSONObject(original.toString())
        config.getJSONObject("prediction").put("model", identity).put("wait_ms", 1000).put("threads", 2).put("max_candidates", 8)
        val device = UiDevice.getInstance(ins)
        try {
            catalog.call("Import", "source" to ModelBackendTest().fixture().absolutePath)
            NativeHandle(baseline.getLong("version")).use { NativeHandle(store.saveConfig(config,it.value).getLong("version")).close() }
            device.executeShellCommand("am start -W -f 0x10008000 -n ${app.packageName}/org.mywubi.MainActivity")
            selectSettingsTab(device,"预测")
            expandSettingsControl(device,"预测模型与资源（实验性）", modelRoot = true)
            expandSettingsControl(device,"运行状态与性能")
            clickSetting(device,"预测设备",description = true)
            clickSetting(device,"自动")
            clickSetting(device,"预测设备",description = true)
            clickSetting(device,"仅 CPU")
            clickSetting(device,"重新检测加速器")
            clickSetting(device,"用固定样例验证模型")
            val placement = findSettingsControl(device,By.textStartsWith("固定样例执行设备：")).text
            assertTrue(placement.isNotEmpty())
            val result = findSettingsControl(device,By.textStartsWith("「北京」的预测：")).text
            assertFalse(result.endsWith("无候选"))
            clickSetting(device,"复制诊断信息")
            assertNotNull(findSettingsControl(device,By.text("已复制诊断信息")))
            var copied = ""
            ins.runOnMainSync {
                copied = app.getSystemService(android.content.ClipboardManager::class.java)
                    .primaryClip?.getItemAt(0)?.text.toString()
            }
            assertTrue(copied, copied.contains(Build.MODEL) && copied.contains("instrumented-counts") && copied.contains("固定样例"))
            File(app.filesDir,"npu-settings.txt").writeText("$placement\n$result\n$copied")
        } finally {
            device.pressBack()
            val current = store.readConfig()
            NativeHandle(current.getLong("version")).use { NativeHandle(store.saveConfig(original,it.value).getLong("version")).close() }
            catalog.call("Remove","selection" to identity)
        }
    }
}
