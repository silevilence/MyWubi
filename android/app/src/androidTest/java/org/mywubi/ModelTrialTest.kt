package org.mywubi

import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import android.os.SystemClock
import org.json.JSONObject
import org.json.JSONArray
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.security.MessageDigest

class ModelTrialTest {
    private val ins=InstrumentationRegistry.getInstrumentation()
    private val context=ins.targetContext
    private fun trial(root:File)=JSONObject(NativeBridge.modelTrial(JSONObject().put("package",root.absolutePath)
        .put("prefix","北京").put("wait_ms",1000).put("threads",4).toString().toByteArray()).toString(Charsets.UTF_8))
    private fun hash(file:File)=MessageDigest.getInstance("SHA-256").digest(file.readBytes()).joinToString(""){"%02x".format(it)}
    @Test fun importAndTrialShareFailureDiagnosticsAndPreserveConfiguration() {
        Resources.prepare(context);val store=EngineStore(context);val before=store.configFile.readBytes()
        val root=File(context.cacheDir,"mi31-diagnostics").apply {mkdirs()}
        File(root,"input.dict").writeText("---\ncharset: 'a1'\n---\na1\t甲\n")
        val inputConfig=File(root,"config.toml").apply {writeText("[basic]\nauto_commit_unique = false\n[dictionary]\nsystem_table = \"input.dict\"\nuser_table = \"user.dict\"\n")}
        val runtime=NativeHandle(NativeBridge.objectCall("BuildRuntime","path" to inputConfig.path).getLong("handle"))
        val editor=JSONObject("""{"editor":2,"epoch":2,"supports_preedit":true,"supports_insert":true,"owns_preedit":false,"purpose":"Text","selection_available":true,"surrounding_available":true,"enter_action":"Send"}""")
        val session=NativeHandle(NativeBridge.objectCall("CreateSession","runtime" to runtime.value,"context" to editor).getLong("handle"))
        fun action(value:Any):JSONObject {
            val plan=NativeBridge.objectCall("Prepare","session" to session.value,"action" to value)
            NativeBridge.call("Ack","session" to session.value,"token" to plan.getJSONObject("token"),"outcome" to "Applied","owns_preedit" to (plan.optJSONObject("op")?.has("CompositionUpdate")==true))
            return plan
        }
        action(JSONObject().put("InputText",JSONArray().put("a").put("Normal")))
        try {
            for(case in listOf("bad","architecture","vocabulary","policy")) {
                val source=File(root,case).apply{mkdirs()}
                val arch=if(case=="architecture") "unsupported" else "qwen2"
                val key="general.architecture".toByteArray()
                val buffer=ByteBuffer.allocate(128).order(ByteOrder.LITTLE_ENDIAN)
                buffer.put("GGUF".toByteArray()).putInt(3).putLong(0).putLong(1).putLong(key.size.toLong()).put(key).putInt(8).putLong(arch.length.toLong()).put(arch.toByteArray())
                File(source,"model.gguf").writeBytes(buffer.array().copyOf(buffer.position()))
                val p=JSONObject(NativeBridge.modelDownloads("{\"op\":\"Available\"}".toByteArray()).toString(Charsets.UTF_8)).getJSONArray("value").getJSONObject(0).getJSONObject("manifest")
                p.put("id","mi31-fixture").put("version","v1").put("adapter","gguf-continuation-v1").remove("generation_policy")
                p.getJSONObject("capabilities").put("proposes_words",false)
                p.put("prediction_policy","policy.json")
                File(source,"policy.json").writeText(JSONObject().put("version",1).put("adapter","gguf-continuation-v1").put("model_sha256",hash(File(source,"model.gguf")))
                    .put("vocabulary_sha256",JSONObject.NULL).put("minimum_score",-1).put("minimum_margin",0).put("evidence","Diagnostic fixture only").toString())
                p.put("files",JSONArray().also {a->for(name in listOf("model.gguf","policy.json")) a.put(JSONObject().put("path",name).put("bytes",File(source,name).length()).put("sha256",hash(File(source,name))))})
                when(case) {
                    "bad" -> File(source,"model.gguf").writeText("broken")
                    "vocabulary" -> p.put("backend","OnnxRuntime").put("adapter","character-step-v1").put("vocabulary","missing.json")
                    "policy" -> File(source,"policy.json").delete()
                }
                File(source,"manifest.json").writeText(p.toString())
                val result=trial(source);assertFalse(result.toString(),result.getBoolean("ok"))
                val expected=when(case){"bad"->"损坏";"architecture"->"不支持的 GGUF 架构";"vocabulary"->"缺少分词器";else->"缺少预测策略"}
                assertTrue(result.toString(),result.getString("error").contains(expected))
                val import=JSONObject(NativeBridge.modelPackages(JSONObject().put("op","Import").put("config",store.configFile.absolutePath).put("source",source.absolutePath).toString().toByteArray()).toString(Charsets.UTF_8))
                assertEquals(result.getString("error"),import.getString("error"));assertArrayEquals(before,store.configFile.readBytes())
                assertEquals("a",NativeBridge.objectCall("State","session" to session.value).getJSONObject("view").getString("spelling"))
            }
            action(JSONObject().put("InputText",JSONArray().put("1").put("Normal")))
            assertEquals("甲",action("Space").getJSONObject("op").getJSONObject("CommitAndReplace").getString("text"))
        } finally {session.close();runtime.close();root.deleteRecursively()}
    }

    @Test fun explicitUiSampleRunsWithoutSelectingOrEnablingModel() {
        org.junit.Assume.assumeTrue("explicit real model acceptance",InstrumentationRegistry.getArguments().getString("model_trial_real")=="true")
        val store=EngineStore(context);Resources.prepare(context)
        val before=store.configFile.readBytes();val draft=File(context.filesDir,"settings-draft.json");val draftBytes=if(draft.exists())draft.readBytes() else null;draft.delete()
        val device=UiDevice.getInstance(ins);Configurator.getInstance().waitForIdleTimeout=500
        fun find(selector:BySelector,timeoutMs:Long=30000):UiObject2 {
            val deadline=SystemClock.uptimeMillis()+timeoutMs
            do {
                ins.uiAutomation.clearCache();device.findObject(selector)?.let{return it}
                for(up in listOf(true,false)) for(step in 0 until 30) {
                    if(SystemClock.uptimeMillis()>=deadline) break
                    ins.uiAutomation.clearCache();device.findObject(selector)?.let{return it}
                    if(up && device.hasObject(By.text("形码 · 拼音 · 英文"))) break
                    if(!up && device.hasObject(By.text("编辑码表与用户词库"))) break
                    val top=device.displayHeight/4;val bottom=device.displayHeight*3/4
                    device.swipe(device.displayWidth/2,if(up)top else bottom,device.displayWidth/2,if(up)bottom else top,40);device.waitForIdle(1000)
                }
                SystemClock.sleep(250)
            } while(SystemClock.uptimeMillis()<deadline)
            error("Missing $selector")
        }
        try {
            assertTrue(File(store.configFile.parentFile,"models/qwen25-words--q4km-v1/manifest.json").isFile)
            device.executeShellCommand("am start -W -f 0x10008000 -n org.mywubi/.MainActivity")
            find(By.text("预测模型与资源")).click();find(By.text("自定义模型指南与试运行")).click()
            find(By.text("接入指南（离线可读）")).click();assertTrue(device.wait(Until.hasObject(By.text("自定义模型接入")),5000))
            device.findObject(By.text("关闭指南")).click()
            find(By.desc("试运行模型")).click();device.wait(Until.findObject(By.text("qwen25-words--q4km-v1")),5000).click()
            // Accessibility setText does not open the IME. Back would finish
            // this clean settings activity instead of hiding a keyboard.
            val sampleField=By.clazz("android.widget.EditText").hasDescendant(By.desc("测试前文"))
            val field=find(sampleField);field.text="北京"
            find(By.text("试运行（1000 ms，4 线程）")).click()
            // Keep the UI still while measuring inference. Scrolling the entire
            // software-rendered settings page competes with the CPU model.
            lateinit var settings:SettingsModel
            ins.runOnMainSync {
                val activity=androidx.test.runner.lifecycle.ActivityLifecycleMonitorRegistry.getInstance()
                    .getActivitiesInStage(androidx.test.runner.lifecycle.Stage.RESUMED).filterIsInstance<MainActivity>().single()
                settings=androidx.lifecycle.ViewModelProvider(activity)[SettingsModel::class.java]
            }
            val completedBy=SystemClock.uptimeMillis()+90000
            var completed=false
            do {
                ins.runOnMainSync {completed=!settings.busy && settings.trialResult.isNotEmpty()}
                if(!completed) SystemClock.sleep(100)
            } while(!completed && SystemClock.uptimeMillis()<completedBy)
            assertTrue("trial completes",completed)
            val result=find(By.desc("模型试运行结果"))
            assertNotNull("trial result",result);assertTrue(result.text,result.text.contains("市"));assertTrue(result.text,result.text.contains("推理"))
            assertArrayEquals(before,store.configFile.readBytes());device.takeScreenshot(File(context.filesDir,"mi31-trial-ui.png"))
            File(context.filesDir,"mi31-trial-ui-result.txt").writeText(result.text)
            find(sampleField).text="北"
            assertTrue(device.wait(Until.hasObject(By.clazz("android.widget.EditText").text("北")),5000))
            ins.uiAutomation.clearCache()
            assertTrue("changing the sample clears the old result",device.wait(Until.gone(By.desc("模型试运行结果")),5000))
            assertArrayEquals(before,store.configFile.readBytes())
        } catch(e:Throwable) {
            device.takeScreenshot(File(context.filesDir,"mi31-trial-failure.png"))
            device.dumpWindowHierarchy(File(context.filesDir,"mi31-trial-failure.xml"));throw e
        } finally {if(draftBytes==null) draft.delete() else draft.writeBytes(draftBytes)}
    }
}
