package org.mywubi

import android.view.KeyEvent
import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File

class PhysicalRuleEditorTest {
    @Test fun errorAndSaveAreReachableAboveTheNumericKeyboard() {
        val ins = InstrumentationRegistry.getInstrumentation()
        val app = ins.targetContext
        val device = UiDevice.getInstance(ins)
        val draft = File(app.filesDir, "settings-draft.json")
        val previous = draft.takeIf { it.isFile }?.readBytes()
        val store = EngineStore(app)
        val original = store.configFile.readBytes()
        val report = JSONObject().put("font_scale", app.resources.configuration.fontScale)
        val prevIme = device.executeShellCommand("settings get secure default_input_method").trim()
        fun find(selector: BySelector): UiObject2 {
            findFresh(device, selector, 10000)?.let { return it }
            device.dumpWindowHierarchy(File(app.filesDir, "rule-editor-failure.xml"))
            device.takeScreenshot(File(app.filesDir, "rule-editor-failure.png"))
            error("Missing $selector")
        }
        try {
            device.executeShellCommand("ime enable org.mywubi/.MyWubiService")
            device.executeShellCommand("ime set org.mywubi/.MyWubiService")
            ActivityScenario.launch(MainActivity::class.java).use { scenario ->
                selectSettingsTab(device, "特殊输入")
                find(By.desc("特殊输入项目列表"))
                scenario.onActivity {
                    val model = ViewModelProvider(it)[SettingsModel::class.java]
                    model.change("shortcuts", "rules", JSONArray().put(JSONObject()
                        .put("code", "qatest").put("position", 2).put("kind", "date_time")
                        .put("format", "yyyy-MM-dd").put("enabled", true)))
                    model.specialSelection = "rule:0"
                    model.specialDetail = true
                }
                val pos = By.pkg("org.mywubi").clazz("android.widget.EditText").text("2")
                for (i in 0 until 12) {
                    ins.uiAutomation.clearCache()
                    val area = find(By.desc("特殊输入项目详情")).visibleBounds
                    val input = device.findObject(pos)?.visibleBounds
                    if (input != null && input.intersect(area) &&
                        input.height() >= (36 * app.resources.displayMetrics.density).toInt()) break
                    if (area.height() < 40) break
                    device.swipe(area.centerX(), area.bottom - 10, area.centerX(), area.centerY(), 22)
                }
                val target = find(pos).visibleBounds
                check(target.intersect(find(By.desc("特殊输入项目详情")).visibleBounds)) { "Position outside detail panel" }
                device.click(target.centerX(), target.centerY())
                find(By.copy(pos).focused(true))
                val keyboard = find(By.pkg("org.mywubi").desc("1").enabled(true))
                val keyboardTop = keyboard.visibleBounds.top
                device.pressKeyCode(KeyEvent.KEYCODE_A, KeyEvent.META_CTRL_ON)
                device.pressKeyCode(KeyEvent.KEYCODE_DEL)
                val error = find(By.text("请输入 1～10000 的整数")).visibleBounds
                val save = find(By.text("保存设置")).visibleBounds
                report.put("save_visible", !save.isEmpty && save.bottom <= keyboardTop)
                    .put("error_visible", !error.isEmpty && error.bottom <= keyboardTop)
                assertTrue("Error is covered with IME open", report.getBoolean("error_visible"))
                assertTrue("Save is covered with IME open", report.getBoolean("save_visible"))
                find(By.pkg("org.mywubi").desc("3").enabled(true)).click()
                find(By.clazz("android.widget.EditText").text("3"))
                device.pressBack()
                find(By.text("保存设置")).click()
                find(By.text("没有未保存更改"))
                val record = store.readConfig()
                NativeHandle(record.getLong("version")).use {
                    val rule = record.getJSONObject("config").getJSONObject("shortcuts").getJSONArray("rules").getJSONObject(0)
                    assertEquals("qatest", rule.getString("code"))
                    assertEquals(3, rule.getInt("position"))
                }
                report.put("saved", true)
            }
        } finally {
            File(app.filesDir, "physical-rule-editor.json").writeText(report.toString())
            store.configFile.writeBytes(original)
            if (previous == null) draft.delete() else draft.writeBytes(previous)
            EngineUpdates.request(app)
            device.executeShellCommand("ime set $prevIme")
        }
    }
}
