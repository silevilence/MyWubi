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
    @Test fun automaticDeviceFallbackCommitsAndDiscardsStalePredictions() = runHost(true, deviceFallback=true)
    private fun runHost(withModel:Boolean,rejectModel:Boolean=false,generated:Boolean=false,lifecycle:Boolean=false,deviceFallback:Boolean=false) {
        val instrumentation=InstrumentationRegistry.getInstrumentation()
        val context=instrumentation.targetContext
        if (deviceFallback) org.junit.Assume.assumeTrue(context.packageName == "org.mywubi.npuaccept")
        val device=UiDevice.getInstance(instrumentation)
        val store=EngineStore(context)
        val loaded=store.readConfig()
        val original=JSONObject(loaded.getJSONObject("config").toString())
        val fixture=File(context.filesDir,"mi-prediction-test.dict")
        // Use a normal dictionary with both a single character and its complete word.
        fixture.writeText("---\ndisplay_name: 预测验收\n---\nb\t北\t100\nbj\t北京\t50\nt\t天气\t100\np\t预报\t100\nf\t五一\t100\nm\t分钟\t100\nw\t明天去\t100\n")
        val config=JSONObject(original.toString())
        if(generated) fixture.appendText("v\t北京\t100\nz\t工作结束了。\t100\nd\t图书\t100\n")
        config.getJSONObject("dictionary").put("system_table",fixture.absolutePath).put("enable_user_dict",false)
        config.getJSONObject("basic").put("auto_commit_unique",false)
        config.put("prediction",JSONObject().put("enabled",true))
        if(withModel) {
            val features=JSONObject(NativeBridge.inference("{\"op\":\"Features\"}".toByteArray()).toString(Charsets.UTF_8)).getJSONObject("value")
            org.junit.Assume.assumeTrue(features.getBoolean(if(generated) "llama" else "onnx"))
            if (!generated) org.junit.Assume.assumeTrue("install the pinned ONNX component first", OnnxComponent.location(context).isFile)
            val source=if(generated) File(context.filesDir,"mi-generation-package") else ModelBackendTest().fixture(if(rejectModel) 0.0 else -100.0)
            org.junit.Assume.assumeTrue("provision licensed generation package",File(source,"manifest.json").isFile)
            ModelCatalog(context).call("Import","source" to source.absolutePath)
            config.getJSONObject("prediction").put("wait_ms",1000).put("max_candidates",if(generated) 1 else 8)
                .put("threads",if(generated) InstrumentationRegistry.getArguments().getString("model_threads","2").toInt().also {require(it in 1..4)} else 2)
                .put("loading",if(generated || lifecycle) "preload_keep" else "on_demand")
                .put("model",JSONObject().put("id",if(generated) "qwen25-words" else "instrumented-counts").put("version","v1"))
        }
        if (deviceFallback) {
            val prediction = config.getJSONObject("prediction").put("device","auto")
            val installed = File(store.configFile.parentFile,"models/instrumented-counts--v1")
            val deadline = android.os.SystemClock.elapsedRealtime() + 15000
            while (OnnxRuntime.status.value != OnnxRuntime.State.Ready && android.os.SystemClock.elapsedRealtime() < deadline)
                android.os.SystemClock.sleep(25)
            assertEquals(OnnxRuntime.State.Ready, OnnxRuntime.status.value)
            // Inject an unavailable accelerator while preserving the real CPU component.
            val detection = JSONObject(NpuDetection.detect(context, OnnxRuntime.State.Ready).toString()).put("backend", false)
            val report = NpuValidation.run(context, installed, prediction, detection)
            assertEquals("NNAPI_UNAVAILABLE", report.getString("reason"))
        }
        fun save(value:JSONObject,version:Long) {
            NativeHandle(version).use { v -> NativeHandle(store.saveConfig(value,v.value).getLong("version")).close() }
        }
        fun key(label:String) {
            requireNotNull(findFresh(device,By.desc(label).pkg(context.packageName).clickable(true),10000)) { "Missing key $label" }.click()
        }
        fun candidate(text:String) {
            val selector=By.desc(Pattern.compile("候选 ${Pattern.quote(text)}(?:，.*)?")).pkg(context.packageName)
            val item=findFresh(device,selector,10000)
            if (item==null) {
                val visible=device.findObjects(By.pkg(context.packageName).descStartsWith("候选 ")).map { it.contentDescription }
                error("Missing candidate $text. Visible candidates: $visible, editor: ${device.findObject(By.clazz("android.widget.EditText"))?.text}")
            }
            item.click()
        }
        val externalHost=InstrumentationRegistry.getArguments().getString("prediction_host")=="editor"
        val editorSelector=By.clazz("android.widget.EditText").apply { if(externalHost) pkg("org.mywubi.acceptancehost") }
        fun resetEditor(text:String="") {
            requireNotNull(findFresh(device,editorSelector,10000)).text=text
            if(externalHost) focusExternalEditor(device) else requireNotNull(findFresh(device,editorSelector,10000)).click()
        }
        fun expect(text:String) {
            assertTrue("host text $text",awaitHostText(device,editorSelector,text,10000))
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
            device.executeShellCommand("ime enable ${context.packageName}/org.mywubi.MyWubiService")
            device.executeShellCommand("ime set ${context.packageName}/org.mywubi.MyWubiService")
            if(externalHost) {
                device.executeShellCommand("am force-stop org.mywubi.acceptancehost")
                device.executeShellCommand("am start -n org.mywubi.acceptancehost/.EditorActivity")
            } else {
                device.executeShellCommand("am force-stop com.android.settings")
                device.executeShellCommand("am force-stop com.google.android.settings.intelligence")
                device.executeShellCommand("am start -a android.settings.SETTINGS")
                val search=device.wait(Until.findObject(By.res("com.android.settings","search_action_bar")),15000)
                assertNotNull("Settings search must be available before IME acceptance",search)
                search.click()
            }
            val field=requireNotNull(findFresh(device,editorSelector,15000))
            resetEditor()
            assertNotNull(findFresh(device,By.desc("b").pkg(context.packageName).enabled(true),15000))
            if(device.hasObject(By.desc("英 → 中"))) key("英 → 中")
            if(generated) {
                val readyDeadline=android.os.SystemClock.uptimeMillis()+30000
                var ready=false
                while(android.os.SystemClock.uptimeMillis()<readyDeadline) {
                    val status=JSONObject(NativeBridge.inference("{\"op\":\"Status\"}".toByteArray()).toString(Charsets.UTF_8))
                    if(status.optString("value")=="Ready") {ready=true;break}
                    android.os.SystemClock.sleep(50)
                }
                assertTrue("generation preload must finish before measuring prediction",ready)
                // Each attempt is a fresh acknowledged commit; late responses
                // from a cold load must not be used to pass this assertion.
                for(attempt in 0..2) {
                    resetEditor();key("v");candidate("北京");expect("北京")
                    if(!device.wait(Until.hasObject(By.desc("字词预测")),1500)) android.os.SystemClock.sleep(1000)
                    else break
                }
                expectPrediction(true)
                candidate("市");expect("北京市")
                key("b");expect("北京市b")
                expectPrediction(false)
                candidate("北");expect("北京市北")
                field.text="外部编辑";device.pressKeyCode(android.view.KeyEvent.KEYCODE_MOVE_END)
                expectPrediction(false)
                // Quantized decoding differs between CPU backends. Both fixtures require
                // two model-only selections and exact readback from the independent editor.
                val books = InstrumentationRegistry.getArguments().getString("generation_chain") == "books"
                val seed = if (books) "图书" else "天气"
                val first = if (books) "作者" else "晴朗"
                val second = if (books) "出版社" else "多云"
                resetEditor();key(if (books) "d" else "t");candidate(seed);expect(seed)
                expectPrediction(true);candidate(first);expect(seed + first)
                expectPrediction(true);candidate(second);expect(seed + first + second)
                device.takeScreenshot(File(context.filesDir,"generation-chain-host.png"))
                resetEditor();key("z");candidate("工作结束了。");expect("工作结束了。")
                android.os.SystemClock.sleep(1100);expectPrediction(false)
                key("空格");key("b");expect("工作结束了。 b")
                candidate("北");expect("工作结束了。 北")
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
            if(deviceFallback) {
                val last = requireNotNull(LastPredictionDevice.entry)
                assertEquals("CPU",last.getString("actual"))
                assertEquals("NNAPI_UNAVAILABLE",last.getString("reason"))
                key("退格");expect("北京天");expectPrediction(false)
                device.pressHome()
                device.executeShellCommand("am start -n org.mywubi.acceptancehost/.EditorActivity -f 0x20000000")
                focusExternalEditor(device)
                expect("北京天");expectPrediction(false)
                key("b");expect("北京天b");candidate("北");expect("北京天北")
                File(context.filesDir,"npu-host.json").writeText(last.toString())
                return
            }
            if(withModel) device.takeScreenshot(File(context.filesDir,"prediction-model-host.png"))
            expectPrediction(true)
            if(lifecycle) {
                device.pressBack()
                assertTrue(device.wait(Until.gone(By.desc("b").pkg(context.packageName)),5000))
                field.click()
                assertTrue(device.wait(Until.hasObject(By.desc("b").pkg(context.packageName)),5000))
                device.executeShellCommand("am send-trim-memory ${context.packageName} RUNNING_LOW")
                android.os.SystemClock.sleep(200)
            }
            key("b"); expect("北京天气b")
            expectPrediction(false)
            candidate("北"); expect("北京天气北")
            field.text="外部编辑";device.pressKeyCode(android.view.KeyEvent.KEYCODE_MOVE_END)
            expectPrediction(false)
            device.takeScreenshot(File(context.filesDir,"prediction-external-host.png"))
            if(!withModel) {
                field.text="";field.click()
                key("f");candidate("五一");candidate("期间");expect("五一期间")
                expectPrediction(false)
                key("空格");key("b");expect("五一期间 b")
                candidate("北");expect("五一期间 北")
                expectPrediction(true)
                device.setOrientationLeft()
                expectPrediction(false)
                expect("五一期间 北")
                device.setOrientationNatural()
                device.waitForIdle()
                val rotatedField=device.wait(Until.findObject(By.clazz("android.widget.EditText")),10000)
                rotatedField.text="";rotatedField.click()
                key("m");candidate("分钟");expect("分钟")
                expectPrediction(false)
            }
        } finally {
            device.setOrientationNatural();device.unfreezeRotation()
            val current=store.readConfig();save(original,current.getLong("version"))
            if(withModel) ModelCatalog(context).call("Remove","selection" to JSONObject().put("id",if(generated) "qwen25-words" else "instrumented-counts").put("version","v1"))
            device.pressBack();device.pressBack()
        }
    }
}
