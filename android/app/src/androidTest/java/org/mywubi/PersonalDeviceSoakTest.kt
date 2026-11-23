package org.mywubi

import android.content.Intent
import android.content.IntentFilter
import android.os.BatteryManager
import android.os.Debug
import android.os.SystemClock
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

class PersonalDeviceSoakTest {
    @Test fun backgroundLearningDuringThirtyMinutesOfRealEditorInput() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("personal_soak") == "true")
        val ins = InstrumentationRegistry.getInstrumentation()
        val app = ins.targetContext
        val device = UiDevice.getInstance(ins)
        val host = "org.mywubi.acceptancehost"
        fun field() = By.pkg(host).clazz("android.widget.EditText")
        val store = EngineStore(app)
        val original = store.readConfig()
        val originalConfig = original.getJSONObject("config")
        val personal = File(store.configFile.parentFile, "personal")
        // This diagnostic is intended for a fresh test installation.
        assumeTrue("requires an isolated personal store", personal.listFiles().orEmpty().none {
            it.name.startsWith("journal-") || it.name == "model.json"
        })
        val table = File(app.cacheDir, "physical-soak.dict")
        val report = File(app.filesDir, "physical-soak.jsonl")
        val previousIme = device.executeShellCommand("settings get secure default_input_method").trim()
        fun save(config: JSONObject, version: Long) {
            NativeHandle(version).use {
                NativeHandle(store.saveConfig(config, it.value).getLong("version")).close()
            }
        }
        fun press(label: String) {
            val node = device.wait(Until.findObject(By.pkg("org.mywubi").desc(label).enabled(true)), 5000)
            requireNotNull(node) { "Missing keyboard action $label" }.click()
        }
        fun status(): JSONObject {
            val request = JSONObject().put("op", "Status").put("config", store.configFile.absolutePath)
            val reply = JSONObject(NativeBridge.personalManage(request.toString().toByteArray()).decodeToString())
            check(reply.getBoolean("ok")) { reply.toString() }
            return reply.getJSONObject("value")
        }
        val start = SystemClock.elapsedRealtime()
        var rounds = 0
        var commits = 0
        var expected = ""
        val versions = mutableSetOf<String>()
        fun sample(phase: String) {
            val battery = app.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))
            val data = status()
            data.optJSONObject("model")?.optString("version")?.let { versions.add(it) }
            val row = JSONObject().put("seconds", (SystemClock.elapsedRealtime() - start) / 1000)
                .put("phase", phase).put("commits", commits).put("pss_kib", Debug.getPss())
                .put("records", data.getJSONObject("data").getInt("records"))
                .put("learning", data.opt("learning")).put("model", data.opt("model"))
                .put("battery_level", battery?.getIntExtra(BatteryManager.EXTRA_LEVEL, -1))
                .put("temperature_tenths_c", battery?.getIntExtra(BatteryManager.EXTRA_TEMPERATURE, -1))
                .put("plug", battery?.getIntExtra(BatteryManager.EXTRA_PLUGGED, -1))
            report.appendText(row.toString() + "\n")
        }
        table.writeText("---\nmax_code_len: 4\n---\nw\t明天去\ng\t图书馆\n")
        val config = JSONObject(originalConfig.toString())
        config.getJSONObject("dictionary").put("system_table", table.absolutePath).put("enable_user_dict", false)
        config.getJSONObject("basic").put("auto_commit_unique", false)
        config.put("personal", JSONObject().put("collect", true).put("learn", true).put("predict", true))
        config.put("prediction", JSONObject().put("enabled", false))
        config.put("expressions", JSONObject())
        try {
            device.executeShellCommand("ime enable org.mywubi/.MyWubiService")
            device.executeShellCommand("ime set org.mywubi/.MyWubiService")
            device.executeShellCommand("am force-stop $host")
            device.executeShellCommand("am start -W -n $host/.EditorActivity")
            requireNotNull(device.wait(Until.findObject(field()), 10000)).click()
            save(config, original.getLong("version"))
            val prefs = EngineUpdates.preferences(app)
            val reloadDeadline = SystemClock.elapsedRealtime() + 10000
            while (prefs.getString("requested", null) != prefs.getString("applied", null) &&
                SystemClock.elapsedRealtime() < reloadDeadline) SystemClock.sleep(50)
            assertEquals(prefs.getString("error", null), prefs.getString("requested", null), prefs.getString("applied", null))
            report.writeText("")
            if (device.hasObject(By.pkg("org.mywubi").desc("英 → 中"))) press("英 → 中")
            val windowStart = SystemClock.elapsedRealtime()
            while (SystemClock.elapsedRealtime() - windowStart < 30 * 60 * 1000L) {
                assertTrue("Lost synthetic host", device.hasObject(field().focused(true)))
                val latencies = mutableListOf<Long>()
                repeat(6) {
                    for ((key, word) in listOf("w" to "明天去", "g" to "图书馆")) {
                        press(key)
                        val t = SystemClock.elapsedRealtimeNanos()
                        press("空格")
                        expected += word
                        assertTrue("Unexpected committed text at $commits", device.wait(Until.hasObject(field().text(expected)), 5000))
                        latencies.add((SystemClock.elapsedRealtimeNanos() - t) / 1000)
                        commits++
                    }
                }
                report.appendText(JSONObject().put("round", ++rounds)
                    .put("commit_to_observed_text_us", org.json.JSONArray(latencies)).toString() + "\n")
                sample("typing")
                // The production worker waits for 30 seconds of idle time.
                SystemClock.sleep(35000)
                sample("idle")
                requireNotNull(device.findObject(field())).text = ""
                expected = ""
            }
            assertTrue("No model published by the background worker", versions.isNotEmpty())
            assertTrue(status().getJSONObject("data").getInt("records") >= commits)
            sample("complete")
        } finally {
            save(originalConfig, store.readConfig().getLong("version"))
            device.executeShellCommand("am force-stop $host")
            if (previousIme.isNotEmpty() && previousIme != "null") device.executeShellCommand("ime set $previousIme")
            table.delete()
        }
    }
    @Test fun manualPauseAndResumeAfterCompletedSyntheticSoak() {
        org.junit.Assume.assumeTrue(InstrumentationRegistry.getArguments().getString("verify_learning_resume")=="true")
        val ins=InstrumentationRegistry.getInstrumentation()
        val app=ins.targetContext
        val dev=UiDevice.getInstance(ins)
        require(File(app.filesDir,"physical-soak.jsonl").isFile)
        val store=EngineStore(app)
        val prior=store.readConfig()
        NativeHandle(prior.getLong("version")).close()
        val cfg=JSONObject(prior.getJSONObject("config").toString())
        val host="org.mywubi.acceptancehost"
        val oldIme=dev.executeShellCommand("settings get secure default_input_method").trim()
        fun save(c:JSONObject) {
            NativeHandle(store.readConfig().getLong("version")).use { NativeHandle(store.saveConfig(c,it.value).getLong("version")).close() }
        }
        fun command(op:String)=JSONObject(NativeBridge.personalManage(JSONObject().put("op",op).put("config",store.configFile.path).toString().toByteArray()).toString(Charsets.UTF_8))
        fun version()=command("Status").getJSONObject("value").getJSONObject("model").getString("version")
        val report=JSONObject()
        try {
            cfg.getJSONObject("personal").put("collect",false).put("learn",false)
            save(cfg)
            val paused=command("Schedule")
            assertFalse(paused.toString(),paused.getBoolean("ok"))
            assertTrue(paused.getString("error").contains("开启并保存后台学习"))
            val old=version()
            report.put("paused_rejected",true).put("before",old)
            cfg.getJSONObject("personal").put("learn",true)
            save(cfg)
            dev.executeShellCommand("ime enable org.mywubi/.MyWubiService")
            dev.executeShellCommand("ime set org.mywubi/.MyWubiService")
            dev.executeShellCommand("am force-stop $host")
            dev.executeShellCommand("am start -W -n $host/.EditorActivity")
            requireNotNull(dev.wait(Until.findObject(By.pkg(host).clazz("android.widget.EditText")),8000)).click()
            assertTrue(command("Schedule").getBoolean("ok"))
            val deadline=SystemClock.elapsedRealtime()+65000
            var now=version()
            while(now==old && SystemClock.elapsedRealtime()<deadline) {SystemClock.sleep(1500);now=version()}
            report.put("after",now).put("status",command("Status").getJSONObject("value").opt("learning"))
            assertNotEquals("Resumed worker should consume the final synthetic records",old,now)
        } finally {
            File(app.filesDir,"personal-pause-resume.json").writeText(report.toString())
            save(prior.getJSONObject("config"))
            dev.executeShellCommand("am force-stop $host")
            if(oldIme.isNotEmpty())dev.executeShellCommand("ime set $oldIme")
        }
    }

}
