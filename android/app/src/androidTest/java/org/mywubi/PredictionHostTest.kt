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
    @Test fun generatedDictionaryExternalWordsAppendInTheRealHost() = runHost(true, generated=true)
    @Test fun preloadedModelRecoversAfterImeHideAndMemoryTrim() = runHost(true, lifecycle=true)
    @Test fun confirmedWordsPredictAndNewCodesOrExternalEditsDiscardSuggestions() = runHost(false)
    @Test fun modelScoredPredictionsUseTheConfirmedHostCommitPath() = runHost(true)
    @Test fun lowConfidenceModelPredictionsStayHiddenAndSpaceRemainsLiteral() = runHost(true,true)
    private fun runHost(withModel:Boolean,rejectModel:Boolean=false,generated:Boolean=false,lifecycle:Boolean=false) {
        val instrumentation=InstrumentationRegistry.getInstrumentation()
        val context=instrumentation.targetContext
        val device=UiDevice.getInstance(instrumentation)
        val store=EngineStore(context)
        val loaded=store.readConfig()
        val original=JSONObject(loaded.getJSONObject("config").toString())
        val fixture=File(context.filesDir,"mi-prediction-test.dict")
        // Use a normal dictionary with both a single character and its complete word.
        fixture.writeText("---\ndisplay_name: 预测验收\n---\nb\t北\t100\nbj\t北京\t50\nt\t天气\t100\np\t预报\t100\nf\t五一\t100\nm\t分钟\t100\nw\t明天去\t100\n")
        val config=JSONObject(original.toString())
        if(generated) fixture.appendText("v\t北京\t100\n")
        config.getJSONObject("dictionary").put("system_table",fixture.absolutePath).put("enable_user_dict",false)
        config.getJSONObject("basic").put("auto_commit_unique",false)
        config.put("prediction",JSONObject().put("enabled",true))
        if(withModel) {
            val features=JSONObject(NativeBridge.inference("{\"op\":\"Features\"}".toByteArray()).toString(Charsets.UTF_8)).getJSONObject("value")
            org.junit.Assume.assumeTrue(features.getBoolean(if(generated) "llama" else "onnx"))
            val source=if(generated) File(context.filesDir,"mi-generation-package") else ModelBackendTest().fixture(if(rejectModel) 0.0 else -100.0)
            org.junit.Assume.assumeTrue("provision licensed generation package",File(source,"manifest.json").isFile)
            ModelCatalog(context).call("Import","source" to source.absolutePath)
            config.getJSONObject("prediction").put("wait_ms",1000).put("max_candidates",if(generated) 1 else 8)
                .put("loading",if(generated || lifecycle) "preload_keep" else "on_demand")
                .put("model",JSONObject().put("id",if(generated) "qwen25-words" else "instrumented-counts").put("version","v1"))
        }
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
        fun expectPrediction(visible:Boolean) {
            val end=android.os.SystemClock.uptimeMillis()+10000
            var actual=false
            do {
                instrumentation.uiAutomation.clearCache()
                actual=device.hasObject(By.desc("字词预测"))
                if(actual==visible) return
                android.os.SystemClock.sleep(100)
            } while(android.os.SystemClock.uptimeMillis()<end)
            device.takeScreenshot(File(context.filesDir,"prediction-failed-${if(withModel) "model" else "lexicon"}.png"))
            device.dumpWindowHierarchy(File(context.filesDir,"prediction-failed-${if(withModel) "model" else "lexicon"}.xml"))
            val status=if(withModel) NativeBridge.inference("{\"op\":\"Status\"}".toByteArray()).toString(Charsets.UTF_8) else "lexicon"
            assertEquals("prediction visibility; worker=$status",visible,actual)
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
            assertTrue(device.wait(Until.hasObject(By.desc("b").pkg("org.mywubi").enabled(true)),15000))
            if(generated) {
                android.os.SystemClock.sleep(3000)
                // Each attempt is a fresh acknowledged commit; late responses
                // from a cold load must not be used to pass this assertion.
                for(attempt in 0..2) {
                    field.text="";field.click();key("v");candidate("北京");expect("北京")
                    if(!device.wait(Until.hasObject(By.desc("字词预测")),1500)) android.os.SystemClock.sleep(1000)
                    else break
                }
                expectPrediction(true)
                candidate("市");expect("北京市")
                key("b");expect("北京市b")
                assertTrue(device.wait(Until.gone(By.desc("字词预测")),5000))
                candidate("北");expect("北京市北")
                field.text="外部编辑";device.pressKeyCode(android.view.KeyEvent.KEYCODE_MOVE_END)
                assertTrue(device.wait(Until.gone(By.desc("字词预测")),5000))
                device.takeScreenshot(File(context.filesDir,"generation-host.png"))
                return
            }
            key("b"); candidate("北"); expect("北")
            if(rejectModel) {
                android.os.SystemClock.sleep(1500)
                expectPrediction(false)
                val status=JSONObject(NativeBridge.inference("{\"op\":\"Status\"}".toByteArray()).toString(Charsets.UTF_8))
                assertEquals("Ready",status.getString("value"))
                key("空格");key("b");expect("北 b");candidate("北");expect("北 北")
                android.os.SystemClock.sleep(1100);expectPrediction(false)
                return
            }
            // Cold loading can exceed the input budget. A later fresh commit can
            // use the loaded worker; a late result must never be displayed.
            if(withModel && !device.wait(Until.hasObject(By.desc("字词预测")),1500)) {
                field.text="";field.click();key("b");candidate("北");expect("北")
            }
            expectPrediction(true)
            assertFalse(device.hasObject(By.text("续接")))
            assertFalse(device.hasObject(By.text("联想")))
            key("空格"); expect("北京")
            candidate("天气"); expect("北京天气")
            if(withModel) device.takeScreenshot(File(context.filesDir,"prediction-model-host.png"))
            assertTrue(device.wait(Until.hasObject(By.desc("字词预测")),5000))
            if(lifecycle) {
                device.pressBack()
                assertTrue(device.wait(Until.gone(By.desc("b").pkg("org.mywubi")),5000))
                field.click()
                assertTrue(device.wait(Until.hasObject(By.desc("b").pkg("org.mywubi")),5000))
                device.executeShellCommand("am send-trim-memory org.mywubi RUNNING_LOW")
                android.os.SystemClock.sleep(200)
            }
            key("b"); expect("北京天气b")
            assertTrue(device.wait(Until.gone(By.desc("字词预测")),5000))
            candidate("北"); expect("北京天气北")
            field.text="外部编辑";device.pressKeyCode(android.view.KeyEvent.KEYCODE_MOVE_END)
            assertTrue(device.wait(Until.gone(By.desc("字词预测")),5000))
            device.takeScreenshot(File(context.filesDir,"prediction-external-host.png"))
            if(!withModel) {
                field.text="";field.click()
                key("f");candidate("五一");candidate("期间");expect("五一期间")
                expectPrediction(false)
                key("空格");key("b");expect("五一期间 b")
                candidate("北");expect("五一期间 北")
                assertTrue(device.wait(Until.hasObject(By.desc("字词预测")),5000))
                device.setOrientationLeft()
                assertTrue(device.wait(Until.gone(By.desc("字词预测")),10000))
                expect("五一期间 北")
                device.setOrientationNatural()
                device.waitForIdle()
                val rotatedField=device.wait(Until.findObject(By.clazz("android.widget.EditText")),10000)
                rotatedField.text="";rotatedField.click()
                key("m");candidate("分钟");expect("分钟")
                assertTrue(device.wait(Until.gone(By.desc("字词预测")),5000))
            }
        } finally {
            device.setOrientationNatural();device.unfreezeRotation()
            val current=store.readConfig();save(original,current.getLong("version"))
            if(withModel) ModelCatalog(context).call("Remove","selection" to JSONObject().put("id",if(generated) "qwen25-words" else "instrumented-counts").put("version","v1"))
            device.pressBack();device.pressBack()
        }
    }
}
