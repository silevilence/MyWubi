package org.mywubi

import android.os.Bundle
import android.os.SystemClock
import android.view.accessibility.AccessibilityNodeInfo
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File

/** MI-43 uses a separate app and compares the actual editor with new journal entries. */
class PhysicalEditCaptureTest {
    @Test fun selectionExternalReplacementAndReconnectionUseTheRealHost() {
        org.junit.Assume.assumeTrue(InstrumentationRegistry.getArguments().getString("physical_acceptance")=="true")
        val ins = InstrumentationRegistry.getInstrumentation()
        val app = ins.targetContext
        val dev = UiDevice.getInstance(ins)
        val host = "org.mywubi.acceptancehost"
        val store = EngineStore(app)
        val original = store.readConfig()
        NativeHandle(original.getLong("version")).close()
        val config = JSONObject(original.getJSONObject("config").toString())
        val table = File.createTempFile("edit-capture-", ".dict", app.cacheDir)
        table.writeText("---\nmax_code_len: 4\n---\nw\t测试\ng\t回读\n")
        config.getJSONObject("dictionary").put("system_table", table.absolutePath).put("enable_user_dict",false)
        config.getJSONObject("basic").put("auto_commit_unique",false)
        config.put("personal", JSONObject().put("collect",true).put("learn",false).put("predict",false))
        config.put("prediction", JSONObject().put("enabled",false))
        val previousIme = dev.executeShellCommand("settings get secure default_input_method").trim()
        val begin = System.currentTimeMillis()
        val report = JSONObject()
        fun save(value: JSONObject) {
            NativeHandle(store.readConfig().getLong("version")).use {
                NativeHandle(store.saveConfig(value,it.value).getLong("version")).close()
            }
        }
        fun newEvents() = File(store.configFile.parentFile,"personal").listFiles().orEmpty()
                .filter { it.name.startsWith("journal-") && it.extension=="jsonl" }
                .flatMap { file -> file.readLines().flatMap { line ->
                    val batch=JSONObject(line).getJSONArray("events")
                    (0 until batch.length()).map(batch::getJSONObject)
                } }.filter { it.getLong("time_ms")>=begin }
        fun field() = By.pkg(host).clazz("android.widget.EditText")
        fun open() {
            dev.executeShellCommand("am force-stop $host")
            dev.executeShellCommand("am start -W -n $host/.EditorActivity")
            requireNotNull(dev.wait(Until.findObject(field()),8000)).click()
        }
        fun press(label: String) {
            requireNotNull(dev.wait(Until.findObject(By.pkg("org.mywubi").desc(label).enabled(true)),5000)) { "Missing key $label" }.click()
        }
        fun expect(value: String) = assertTrue("Host expected $value", dev.wait(Until.hasObject(field().focused(true).text(value)),5000))
        fun select(start: Int, end: Int) {
            val automation=ins.uiAutomation
            val info=automation.serviceInfo
            info.flags=info.flags or android.accessibilityservice.AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS
            automation.serviceInfo=info
            val roots = automation.windows.mapNotNull { it.root }.filter { it.packageName == host }
            val queue = java.util.ArrayDeque(roots)
            while(queue.isNotEmpty()) {
                val n = queue.removeFirst()
                if(n.className == "android.widget.EditText") {
                    if (n.textSelectionStart == start && n.textSelectionEnd == end) return
                    val args = Bundle().apply {
                        putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_START_INT,start)
                        putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_END_INT,end)
                    }
                    assertTrue(n.performAction(AccessibilityNodeInfo.ACTION_SET_SELECTION,args))
                    val deadline = SystemClock.elapsedRealtime()+3000
                    while(SystemClock.elapsedRealtime()<deadline && n.refresh()) {
                        if(n.textSelectionStart==start && n.textSelectionEnd==end) return
                        SystemClock.sleep(30)
                    }
                    error("Host selection did not become $start..$end")
                }
                for(i in 0 until n.childCount) n.getChild(i)?.let(queue::add)
            }
            error("Host EditText unavailable for selection")
        }
        try {
            dev.executeShellCommand("ime enable org.mywubi/.MyWubiService")
            dev.executeShellCommand("ime set org.mywubi/.MyWubiService")
            save(config)
            open()
            requireNotNull(dev.wait(Until.findObject(By.pkg("org.mywubi").desc("w").enabled(true)),12000))
            val prefs=EngineUpdates.preferences(app)
            val deadline=SystemClock.elapsedRealtime()+25000
            while(prefs.getString("requested",null)!=prefs.getString("applied",null) && SystemClock.elapsedRealtime()<deadline) SystemClock.sleep(50)
            assertEquals(prefs.getString("error",null),prefs.getString("requested",null),prefs.getString("applied",null))
            if(dev.hasObject(By.pkg("org.mywubi").desc("英 → 中"))) press("英 → 中")
            val first="甲😀乙"
            requireNotNull(dev.findObject(field())).text=first
            expect(first)
            select(3,3)
            press("w"); press("空格"); expect("甲😀测试乙")
            report.put("insert",true)
            select(3,5)
            press("g"); press("空格"); expect("甲😀回读乙")
            report.put("replace",true)
            select(5,5)
            press("退格"); expect("甲😀回乙")
            requireNotNull(dev.findObject(field())).text="外部修改"
            expect("外部修改")
            // Let the replacement notification and the journal writer finish while
            // the original InputConnection is still available; reconnect separately.
            val untilObserved = SystemClock.elapsedRealtime() + 5000
            while (newEvents().none { it.toString().contains("外部修改") } &&
                SystemClock.elapsedRealtime() < untilObserved) SystemClock.sleep(80)
            open()
            press("w"); press("空格"); expect("测试")
            report.put("reconnect",true)
            SystemClock.sleep(1500)
            val events = newEvents()
            val commits=events.filter { it.getJSONObject("change").getString("kind")=="Commit" }
            val inserted=commits.firstOrNull { it.getJSONObject("before").optString("left")=="甲😀" &&
                it.getJSONObject("before").optString("right")=="乙" && it.getJSONObject("change").optString("text")=="测试" }
            val replacement=commits.firstOrNull { it.getJSONObject("change").optString("text")=="回读" }
            val last=commits.lastOrNull()
            report.put("commit_count",commits.size).put("insert_event",inserted).put("replace_event",replacement).put("final_event",last)
            report.put("external_observed",events.any { it.toString().contains("外部修改") })
            assertNotNull("Missing matching mid-string journal entry",inserted)
            val where=inserted!!.getJSONObject("before").getJSONObject("selection")
            assertEquals(3,where.getInt("start"));assertEquals("Utf16",where.getString("unit"))
            assertEquals("Confirmed",inserted.getJSONObject("change").getString("outcome"))
            assertEquals("测试",replacement?.getJSONObject("before")?.optString("selected"))
            assertNotEquals(inserted.getString("session"),last?.getString("session"))
            assertTrue("Missing external edit observation",report.getBoolean("external_observed"))
            assertEquals(events.map { it.getLong("sequence") }.sorted(),events.map { it.getLong("sequence") })
        } finally {
            File(app.filesDir,"physical-edit-capture.json").writeText(report.toString(2))
            save(original.getJSONObject("config"))
            dev.executeShellCommand("am force-stop $host")
            if(previousIme.isNotEmpty()) dev.executeShellCommand("ime set $previousIme")
            table.delete()
        }
    }
}
