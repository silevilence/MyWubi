package org.mywubi

import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.util.regex.Pattern

class PredictionHostTest {
    @Test fun confirmedWordsPredictAndNewCodesOrExternalEditsDiscardSuggestions() {
        val instrumentation=InstrumentationRegistry.getInstrumentation()
        val context=instrumentation.targetContext
        val device=UiDevice.getInstance(instrumentation)
        val store=EngineStore(context)
        val loaded=store.readConfig()
        val original=JSONObject(loaded.getJSONObject("config").toString())
        val fixture=File(context.filesDir,"mi-prediction-test.dict")
        // Use a normal dictionary with both a single character and its complete word.
        fixture.writeText("---\ndisplay_name: 预测验收\n---\nb\t北\t100\nbj\t北京\t50\nt\t天气\t100\np\t预报\t100\n")
        val config=JSONObject(original.toString())
        config.getJSONObject("dictionary").put("system_table",fixture.absolutePath).put("enable_user_dict",false)
        config.getJSONObject("basic").put("auto_commit_unique",false)
        config.put("prediction",JSONObject().put("enabled",true))
        fun save(value:JSONObject,version:Long) {
            NativeHandle(version).use { v -> NativeHandle(store.saveConfig(value,v.value).getLong("version")).close() }
        }
        fun key(label:String) {
            instrumentation.uiAutomation.clearCache()
            device.wait(Until.findObject(By.desc(label).pkg("org.mywubi").clickable(true)),10000).click()
        }
        fun candidate(text:String) {
            instrumentation.uiAutomation.clearCache()
            val selector=By.desc(Pattern.compile("候选 ${Pattern.quote(text)}(?:，.*)?")).pkg("org.mywubi")
            assertTrue("candidate $text",device.wait(Until.hasObject(selector),10000))
            device.findObject(selector).click()
        }
        fun expect(text:String) {
            assertTrue("host text $text",device.wait(Until.hasObject(By.clazz("android.widget.EditText").text(text)),10000))
        }
        try {
            save(config,loaded.getLong("version"))
            device.executeShellCommand("ime enable org.mywubi/.MyWubiService")
            device.executeShellCommand("ime set org.mywubi/.MyWubiService")
            device.executeShellCommand("am force-stop com.android.settings")
            device.executeShellCommand("am force-stop com.google.android.settings.intelligence")
            device.executeShellCommand("am start -a android.settings.SETTINGS")
            device.wait(Until.findObject(By.res("com.android.settings","search_action_bar")),15000).click()
            val field=device.wait(Until.findObject(By.clazz("android.widget.EditText")),15000)
            field.text="";field.click()
            if(device.hasObject(By.desc("英 → 中"))) key("英 → 中")
            assertTrue(device.wait(Until.hasObject(By.text("预测验收")),15000))
            key("b"); candidate("北"); expect("北")
            candidate("京"); expect("北京")
            candidate("天气"); expect("北京天气")
            assertTrue(device.wait(Until.hasObject(By.text("预测")),5000))
            key("b"); expect("北京天气b")
            assertTrue(device.wait(Until.gone(By.text("预测")),5000))
            candidate("北"); expect("北京天气北")
            field.text="外部编辑";device.pressKeyCode(android.view.KeyEvent.KEYCODE_MOVE_END)
            assertTrue(device.wait(Until.gone(By.text("预测")),5000))
            device.takeScreenshot(File(context.filesDir,"prediction-external-host.png"))
        } finally {
            val current=store.readConfig();save(original,current.getLong("version"))
            device.pressBack();device.pressBack()
        }
    }
}
