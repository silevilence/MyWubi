package org.mywubi

import android.os.SystemClock
import android.view.KeyEvent
import android.view.accessibility.AccessibilityNodeInfo
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.security.MessageDigest

class FrequencyHostTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val app = instrumentation.targetContext
    private val device = UiDevice.getInstance(instrumentation)
    private val field = By.clazz("android.widget.EditText").pkg("org.mywubi.acceptancehost")
    private val store = EngineStore(app)
    private fun waitFor(message: String, predicate: () -> Boolean) {
        val deadline = SystemClock.uptimeMillis() + 20000
        do {
            instrumentation.uiAutomation.clearCache()
            if (predicate()) return
            SystemClock.sleep(80)
        } while (SystemClock.uptimeMillis() < deadline)
        device.dumpWindowHierarchy(File(app.filesDir,"frequency-host-failure.xml"))
        fail(message)
    }
    private fun key(label: String) {
        val selector = By.desc(label).pkg("org.mywubi").clickable(true)
        waitFor("未找到按键 $label") { device.hasObject(selector) }
        device.findObject(selector).click()
    }
    private fun choose(word: String) {
        key("a"); key("b")
        val selector = By.descStartsWith("候选 $word").pkg("org.mywubi")
        waitFor("未找到候选 $word") { device.hasObject(selector) }
        device.findObject(selector).click()
    }
    private fun host(text: String) {
        waitFor("宿主正文应为 $text") { device.findObject(field)?.let { it.text.orEmpty() == text && it.isFocused } == true }
    }
    private fun save(config: JSONObject, version: Long) {
        NativeHandle(version).use { NativeHandle(store.saveConfig(config, it.value).getLong("version")).close() }
    }
    @Test fun correctSelectionsWithHostPreedit() = checkExternalEditor(false)
    @Test fun correctSelectionsWithLocalPreedit() = checkExternalEditor(true)

    private fun checkExternalEditor(localPreedit: Boolean) {
        val original = store.readConfig()
        NativeHandle(original.getLong("version")).close()
        device.wakeUp()
        device.executeShellCommand("wm dismiss-keyguard")
        val initial = original.getJSONObject("config")
        val prefs = app.getSharedPreferences("android-ui", 0)
        val hadLocal = prefs.contains("local_preedit")
        val previousLocal = prefs.getBoolean("local_preedit", false)
        prefs.edit().putBoolean("local_preedit",localPreedit).commit()
        val previousIme = device.executeShellCommand("settings get secure default_input_method").trim()
        val samples = File.createTempFile("frequency-host-", ".dict", app.cacheDir)
        val report = File(app.filesDir,"frequency-host-$localPreedit.jsonl")
        report.writeText("")
        val dictionaries = mutableListOf<File>()
        try {
            device.executeShellCommand("ime enable org.mywubi/.MyWubiService")
            device.executeShellCommand("ime set org.mywubi/.MyWubiService")
            var controlPreview = ""
            var controlFirst = ""
            for (scenario in listOf("control", "backspace", "undo", "replace")) {
                val table = File(samples.parentFile, samples.name + scenario)
                val user = File(samples.parentFile, samples.name + scenario + ".user")
                table.writeText("ab\t以及\t10\nab\t已经\t9\n")
                val config = JSONObject(initial.toString())
                config.getJSONObject("dictionary").put("system_table",table.path).put("user_table",user.path).put("enable_user_dict",true)
                config.getJSONObject("basic").put("auto_commit_unique",false)
                config.put("frequency",JSONObject().put("record",true).put("rank",true))
                config.put("personal",JSONObject()); config.put("prediction",JSONObject()); config.put("expressions",JSONObject())
                save(config,store.readConfig().getLong("version"))
                val name = MessageDigest.getInstance("SHA-256").digest(table.canonicalPath.toByteArray()).joinToString("") { "%02x".format(it) }
                val data = File(store.configFile.parentFile,"frequency/$name.json")
                dictionaries.addAll(listOf(table,user,data,File(data.path+".lock"),File(user.path+".lock")))
                device.executeShellCommand("am force-stop org.mywubi.acceptancehost")
                device.executeShellCommand("am start -W -n org.mywubi.acceptancehost/.EditorActivity")
                waitFor("外部编辑框未启动") {device.hasObject(field)}
                device.findObject(field).click()
                waitFor("新码表未应用") { EngineUpdates.preferences(app).let {it.getString("applied",null)==it.getString("requested",null)} }
                if (device.hasObject(By.desc("英 → 中"))) key("英 → 中")
                fun count(word: String) = if (data.isFile) JSONObject(data.readText()).getJSONObject("counts").optJSONObject("ab")?.optInt(word) ?: 0 else 0
                if (scenario != "control") {
                    choose("以及"); host("以及")
                    waitFor("$scenario 首次上屏未计数；统计文件 $data") { count("以及")==1 }
                    when (scenario) {
                        "backspace" -> { key("退格"); host("以"); key("退格"); host("") }
                        "undo" -> { device.pressKeyCode(KeyEvent.KEYCODE_Z,KeyEvent.META_CTRL_ON); host("") }
                        "replace" -> { device.pressKeyCode(KeyEvent.KEYCODE_A,KeyEvent.META_CTRL_ON); device.waitForIdle() }
                    }
                }
                choose("已经"); host("已经")
                waitFor("$scenario 纠正后计数：以及=${count("以及")} 已经=${count("已经")}") { count("以及")==0 && count("已经")==1 }
                val node = instrumentation.uiAutomation.rootInActiveWindow.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
                assertEquals("已经",node.text.toString()); assertEquals(2,node.textSelectionStart); assertEquals(2,node.textSelectionEnd)
                val preview = NativeBridge.objectCall("SaveFrequency","path" to store.configFile.path)
                assertEquals(1,preview.getInt("count"))
                val examples = preview.getJSONArray("examples").toString()
                if (scenario=="control") controlPreview=examples else assertEquals(controlPreview,examples)
                key("a"); key("b")
                val items=device.findObjects(By.descStartsWith("候选 ").pkg("org.mywubi"))
                assertTrue("两候选须可见",items.size>=2)
                val first=items.minBy {it.visibleBounds.left}.contentDescription
                if(scenario=="control") controlFirst = first
                else assertEquals(controlFirst,first)
                report.appendText(JSONObject().put("case",scenario).put("wrong",count("以及")).put("correct",count("已经"))
                    .put("text",node.text).put("selection","2,2").put("first",first).put("examples",examples).toString()+"\n")
                if (scenario == "replace") {
                    items.first { it.contentDescription.startsWith("候选 已经") }.longClick()
                    val move = By.text("移到第 1 位").pkg("org.mywubi")
                    waitFor("长按候选未打开调位菜单") { device.hasObject(move) }
                    device.pressBack()
                    waitFor("取消调位后候选未恢复") { device.hasObject(By.descStartsWith("候选 已经").pkg("org.mywubi")) && !device.hasObject(move) }
                    host(if (localPreedit) "已经" else "已经ab")
                    key("展开")
                    device.findObject(By.descStartsWith("候选 已经").pkg("org.mywubi")).longClick()
                    waitFor("重新长按未打开调位菜单") { device.hasObject(move) }
                    device.findObject(move).click()
                    waitFor("调位后编码或首选丢失") {
                        device.findObjects(By.descStartsWith("候选 ").pkg("org.mywubi")).minByOrNull {it.visibleBounds.left}
                            ?.contentDescription?.startsWith("候选 已经") == true
                    }
                    if (device.hasObject(By.desc("收起"))) key("收起")
                    key("空格")
                    host("已经已经")
                    val savedUser = user.readText()
                    key("a"); key("b")
                    device.findObject(By.descStartsWith("候选 以及").pkg("org.mywubi")).longClick()
                    waitFor("切换编辑框前未打开菜单") { device.hasObject(move) }
                    device.executeShellCommand("am start -W -f 0x18000000 -n org.mywubi.acceptancehost/.EditorActivity")
                    host("")
                    waitFor("新编辑框仍显示上一个候选的菜单") { !device.hasObject(move) }
                    assertEquals(savedUser, user.readText())
                }

            }
        } finally {
            device.executeShellCommand("am force-stop org.mywubi.acceptancehost")
            if (previousIme.contains('/')) device.executeShellCommand("ime set $previousIme")
            prefs.edit().apply {if(hadLocal) putBoolean("local_preedit",previousLocal) else remove("local_preedit")}.commit()
            save(initial,store.readConfig().getLong("version"))
            samples.delete(); dictionaries.forEach {it.delete()}
        }
    }
}
