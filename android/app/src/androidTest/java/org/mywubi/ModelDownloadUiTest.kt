package org.mywubi

import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import android.os.SystemClock
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File

/** Opt-in real network acceptance. Never substitutes local model bytes for a download. */
class ModelDownloadUiTest {
    @Test fun downloadChooseSaveReopenAndPredictInExternalEditor() {
        val ins=InstrumentationRegistry.getInstrumentation()
        val reuse=InstrumentationRegistry.getArguments().getString("model_download_reuse")=="true"
        org.junit.Assume.assumeTrue("explicit network or installed-model diagnostic acceptance",reuse || InstrumentationRegistry.getArguments().getString("model_download_network")=="true")
        val context=ins.targetContext;val device=UiDevice.getInstance(ins);val store=EngineStore(context)
        Configurator.getInstance().waitForIdleTimeout=500
        Resources.prepare(context)
        val original=store.readConfig();NativeHandle(original.getLong("version")).close()
        val draft=File(context.filesDir,"settings-draft.json")
        val originalDraft=if(draft.exists()) draft.readBytes() else null
        draft.delete()
        val fixture=File(context.filesDir,"mi30-input.dict")
        fixture.writeText("---\ndisplay_name: 下载验收\n---\nv\t北京\t100\nb\t北\t100\n")
        fun save(config:JSONObject) {
            val old=store.readConfig()
            NativeHandle(old.getLong("version")).use {version->NativeHandle(store.activateConfig(config,version.value).getLong("version")).close()}
        }
        fun scrollFind(selector:BySelector):UiObject2 {
            // Reopening validates installed packages asynchronously. Reaching the
            // bottom of the loading page does not mean the control is absent.
            val deadline=SystemClock.uptimeMillis()+30000
            do {
                ins.uiAutomation.clearCache()
                device.findObject(selector)?.let {return it}
                for(up in listOf(true,false)) for(step in 0 until 30) {
                    ins.uiAutomation.clearCache();device.findObject(selector)?.let {return it}
                    if(up && device.hasObject(By.text("形码 · 拼音 · 英文"))) break
                    if(!up && device.hasObject(By.text("编辑码表与用户词库"))) break
                    val top=device.displayHeight/4;val bottom=device.displayHeight*3/4
                    device.swipe(device.displayWidth/2,if(up) top else bottom,device.displayWidth/2,if(up) bottom else top,40)
                    device.waitForIdle(1000)
                }
                SystemClock.sleep(250)
            } while(SystemClock.uptimeMillis()<deadline)
            error("Missing $selector")
        }
        fun text(value:String)=scrollFind(By.text(value))
        fun description(value:String)=scrollFind(By.desc(value))
        fun candidate(value:String,timeoutMs:Long=10000):UiObject2 {
            val selector=By.desc(java.util.regex.Pattern.compile("候选 ${java.util.regex.Pattern.quote(value)}(?:，.*)?")).pkg("org.mywubi")
            val deadline=SystemClock.uptimeMillis()+timeoutMs
            do {
                ins.uiAutomation.clearCache()
                device.findObject(selector)?.let {return it}
                SystemClock.sleep(100)
            } while(SystemClock.uptimeMillis()<deadline)
            error("Missing candidate $value")
        }
        try {
            val config=JSONObject(original.getJSONObject("config").toString())
            config.getJSONObject("dictionary").put("system_table",fixture.absolutePath).put("enable_user_dict",false)
            config.getJSONObject("basic").put("auto_commit_unique",false)
            config.put("prediction",JSONObject().put("enabled",false).put("wait_ms",1000).put("threads",4).put("max_candidates",1))
            save(config)
            // Remove only this acceptance model if a previous interrupted run installed it.
            val selection=JSONObject().put("id","qwen25-words").put("version","q4km-v1")
            val packages=ModelCatalog(context).call("List")
            if(!reuse && (0 until packages.length()).any {packages.getJSONObject(it).optString("id")=="qwen25-words" && packages.getJSONObject(it).optString("version")=="q4km-v1"})
                ModelCatalog(context).call("Remove","selection" to selection)
            device.executeShellCommand("am start -W -f 0x10008000 -n org.mywubi/.MainActivity")
            text("预测模型与资源").click()
            if(!reuse) {
            description("下载源").click()
            device.wait(Until.findObject(By.text("ModelScope")),5000).click()
            description("下载模型 qwen25-words @ q4km-v1").click()
            val deadline=SystemClock.uptimeMillis()+12*60*1000
            var phase=""
            var seenDownload=false
            while(SystemClock.uptimeMillis()<deadline) {
                val reply=JSONObject(NativeBridge.modelDownloads(JSONObject().put("op","Status").put("config",store.configFile.absolutePath).toString().toByteArray()).toString(Charsets.UTF_8))
                val status=reply.getJSONObject("value");phase=status.getString("phase")
                if(phase=="downloading") seenDownload=true
                if(phase=="ready") break
                check(phase!="failed") {status.toString()}
                SystemClock.sleep(500)
            }
            assertEquals("ready",phase);assertTrue("observed real download state",seenDownload)
            }
            assertFalse("no duplicate completed download cache",File(store.configFile.parentFile,"models/.downloads/qwen25-words--q4km-v1/package").exists())
            val downloaded=store.readConfig();NativeHandle(downloaded.getLong("version")).close()
            assertFalse(downloaded.getJSONObject("config").getJSONObject("prediction").getBoolean("enabled"))
            assertTrue(downloaded.getJSONObject("config").getJSONObject("prediction").isNull("model"))
            if(!reuse) text("校验通过，可选择；总开关未改变")
            device.takeScreenshot(File(context.filesDir,"mi30-download-ready.png"))
            val selectable=description("选择模型 qwen25-words @ q4km-v1")
            assertTrue("validated model can be selected",selectable.isEnabled)
            device.pressBack()
            device.executeShellCommand("am start -W -n org.mywubi/.MainActivity")
            text("预测模型与资源").click()
            description("选择模型 qwen25-words @ q4km-v1").click()
            description("提前加载模型（隐藏或资源紧张时释放）").click()
            description("上屏后显示字词预测").click()
            text("保存设置").click();text("没有未保存更改")
            val saved=store.readConfig();NativeHandle(saved.getLong("version")).close()
            assertEquals("q4km-v1",saved.getJSONObject("config").getJSONObject("prediction").getJSONObject("model").getString("version"))
            val prediction=saved.getJSONObject("config").getJSONObject("prediction")
            File(context.filesDir,"mi30-saved-prediction.json").writeText(prediction.toString(2))
            assertTrue(prediction.toString(),prediction.getBoolean("enabled"))
            assertEquals("preload_keep",prediction.getString("loading"))
            assertEquals(1000,prediction.getInt("wait_ms"))
            assertEquals(4,prediction.getInt("threads"))
            device.pressBack()
            device.executeShellCommand("am start -W -f 0x10008000 -n org.mywubi/.MainActivity")
            assertTrue("saved master switch survives reopening",description("上屏后显示字词预测").isChecked)
            text("预测模型与资源").click()
            text("当前选择：qwen25-words @ q4km-v1")
            assertTrue("saved preload survives reopening",description("提前加载模型（隐藏或资源紧张时释放）").isChecked)
            device.executeShellCommand("am force-stop org.mywubi.acceptancehost")
            device.executeShellCommand("ime enable org.mywubi/.MyWubiService")
            device.executeShellCommand("ime set org.mywubi/.MyWubiService")
            device.executeShellCommand("am start -W -f 0x10008000 -n org.mywubi.acceptancehost/.EditorActivity")
            val field=device.wait(Until.findObject(By.clazz("android.widget.EditText")),10000)
            field.text="";field.click()
            val readyBy=SystemClock.uptimeMillis()+60000
            var ready=false
            val phases=org.json.JSONArray()
            var previous=""
            while(SystemClock.uptimeMillis()<readyBy) {
                val status=JSONObject(NativeBridge.inference("{\"op\":\"Status\"}".toByteArray()).toString(Charsets.UTF_8))
                if(status.toString()!=previous) {previous=status.toString();phases.put(status);File(context.filesDir,"mi30-ready-phases.json").writeText(phases.toString(2))}
                if(status.optString("value")=="Ready") {ready=true;break};SystemClock.sleep(100)
            }
            assertTrue("downloaded model ready: $phases",ready)
            val attempts=org.json.JSONArray()
            // A resource-constrained device may discard a timed-out prediction.
            // Retry only with a fresh acknowledged host commit, as in MI-23;
            // never extend a request's 1000 ms budget or accept a late response.
            for(attempt in 1..3) {
                field.text="";field.click();ins.uiAutomation.clearCache()
                device.wait(Until.findObject(By.desc("v").pkg("org.mywubi")),10000).click()
                candidate("北京").click()
                assertTrue(device.wait(Until.hasObject(By.clazz("android.widget.EditText").text("北京")),5000))
                val next=runCatching {candidate("市",1500)}.getOrNull()
                attempts.put(JSONObject().put("attempt",attempt).put("candidate_visible",next!=null)
                    .put("status",JSONObject(NativeBridge.inference("{\"op\":\"Status\"}".toByteArray()).toString(Charsets.UTF_8))))
                File(context.filesDir,"mi30-prediction-attempts.json").writeText(attempts.toString(2))
                if(next!=null) {next.click();break}
                SystemClock.sleep(1000)
            }
            assertTrue(device.wait(Until.hasObject(By.clazz("android.widget.EditText").text("北京市")),5000))
            device.takeScreenshot(File(context.filesDir,"mi30-downloaded-prediction.png"))
        } catch(e:Throwable) {
            device.takeScreenshot(File(context.filesDir,"mi30-failure.png"));device.dumpWindowHierarchy(File(context.filesDir,"mi30-failure.xml"));throw e
        } finally {
            save(original.getJSONObject("config"));fixture.delete()
            if(originalDraft==null) draft.delete() else draft.writeBytes(originalDraft)
        }
    }
}
