package org.mywubi

import android.os.SystemClock
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.UiDevice
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File
import java.security.MessageDigest

class ModelBackendTest {
    private val instrumentation=InstrumentationRegistry.getInstrumentation()
    private val context=instrumentation.targetContext
    private fun command(op:String,vararg fields:Pair<String,Any>):JSONObject {
        val r=JSONObject().put("op",op);fields.forEach{(k,v)->r.put(k,v)}
        val result=JSONObject(NativeBridge.inference(r.toString().toByteArray()).toString(Charsets.UTF_8))
        assertTrue(result.toString(),result.getBoolean("ok"));return result
    }
    private fun caps()=JSONObject("""{"protocol":1,"tasks":["Rank","Predict"],"languages":["zh"],"max_context_chars":256,"max_candidates":64,"max_candidate_chars":16,"max_code_chars":64,"proposes_words":false,"incremental":true,"cooperative_cancel":true}""")
    internal fun fixture(minimumScore:Double=-100.0):File {
        val root=File(context.cacheDir,"onnx-backend-source").apply{mkdirs()}
        for(name in listOf("counts.onnx","vocabulary.json")) instrumentation.context.assets.open(name).use{input->File(root,name).outputStream().use{input.copyTo(it)}}
        fun digest(name:String)=MessageDigest.getInstance("SHA-256").digest(File(root,name).readBytes()).joinToString(""){"%02x".format(it)}
        File(root,"prediction-policy.json").writeText(JSONObject().put("version",1).put("adapter","character-step-v1")
            .put("model_sha256",digest("counts.onnx")).put("vocabulary_sha256",digest("vocabulary.json"))
            .put("minimum_score",minimumScore).put("minimum_margin",0)
            .put("evidence","Authored instrumentation fixture, permissive thresholds for plumbing tests only; not calibrated for product quality.").toString())
        val manifest=JSONObject().put("protocol",1).put("id","instrumented-counts").put("version","v1").put("backend","OnnxRuntime")
            .put("adapter","character-step-v1").put("model","counts.onnx").put("vocabulary","vocabulary.json").put("source","MyWubi authored MI01 train").put("license","MIT").put("capabilities",caps())
        manifest.put("prediction_policy","prediction-policy.json")
        manifest.put("files",JSONArray().also{a->for(name in listOf("counts.onnx","vocabulary.json","prediction-policy.json")){val f=File(root,name);a.put(JSONObject().put("path",name).put("bytes",f.length()).put("sha256",digest(name)))}})
        File(root,"manifest.json").writeText(manifest.toString());return root
    }
    private fun score(source:File) {
        UiDevice.getInstance(instrumentation).pressHome()
        instrumentation.waitForIdleSync()
        NativeBridge.runtimeDirectory(context.applicationInfo.nativeLibraryDir)
        val manifest=JSONObject(File(source,"manifest.json").readText())
        val root=File(context.cacheDir,"backend-store-${System.nanoTime()}").apply{mkdirs()}
        try {
            val import=JSONObject().put("op","Import").put("config",File(root,"config.toml").absolutePath).put("source",source.absolutePath)
            val imported=JSONObject(NativeBridge.modelPackages(import.toString().toByteArray()).toString(Charsets.UTF_8))
            assertTrue(imported.toString(),imported.getBoolean("ok"))
            val config=JSONObject().put("enabled",true).put("wait_ms",1000).put("threads",2).put("max_candidates",16).put("idle_seconds",10)
                .put("model",JSONObject().put("id",manifest.getString("id")).put("version",manifest.getString("version")))
            command("Configure","root" to File(root,"models").absolutePath,"config" to config)
            command("Pause","paused" to false)
            val end=SystemClock.uptimeMillis()+30000
            var success=false;var serial=0L
            while(SystemClock.uptimeMillis()<end && !success) {
                serial++
                val key=JSONObject("""{"edit":{"session":9182,"generation":$serial,"editor":1,"epoch":1,"runtime":1},"query":{"engine":0,"version":$serial},"view":$serial,"context":1,"model":1,"config":1,"request":$serial}""")
                val request=JSONObject().put("protocol",1).put("key",key).put("task","Predict").put("language","zh").put("prefix","北京").put("code","")
                    .put("candidates",JSONArray("""[{"id":7,"text":"天气"},{"id":9,"text":"大学"}]"""))
                command("Submit","request" to request)
                val deadline=SystemClock.uptimeMillis()+1100
                while(SystemClock.uptimeMillis()<deadline) {
                    val value=command("Poll","key" to key).optJSONObject("value")
                    if(value!=null) {
                        assertTrue(value.toString(),value.has("Ok"))
                        val scores=value.getJSONObject("Ok").getJSONArray("scores")
                        assertTrue(value.getJSONObject("Ok").getJSONArray("prediction_accepted").length()>0)
                        assertEquals(2,scores.length())
                        assertEquals(setOf(7,9),(0 until scores.length()).map{scores.getJSONObject(it).getInt("id")}.toSet())
                        for(i in 0 until scores.length())assertTrue(scores.getJSONObject(i).getDouble("value").isFinite())
                        success=true;break
                    }
                    SystemClock.sleep(10)
                }
            }
            assertTrue("backend did not produce valid scores",success)
            if(manifest.getString("backend")=="OnnxRuntime") controllerPublishesAndCancels(root,manifest)
            command("Pause","paused" to true)
            val stop=SystemClock.uptimeMillis()+3000
            while(command("Status").optString("value")!="Paused" && SystemClock.uptimeMillis()<stop)SystemClock.sleep(10)
            assertEquals("Paused",command("Status").getString("value"))
        } finally {command("Close");root.deleteRecursively()}
    }
    private fun controllerPublishesAndCancels(root:File,manifest:JSONObject) {
        File(root,"main.dict").writeText("b\t北京\t100\n")
        val config=File(root,"config.toml").apply {writeText("""
            [basic]
            auto_commit_unique = false
            [dictionary]
            system_table = "main.dict"
            [prediction]
            enabled = true
            wait_ms = 1000
            loading = "preload_keep"
            [prediction.model]
            id = "${manifest.getString("id")}"
            version = "${manifest.getString("version")}"
        """.trimIndent())}
        NativeHandle(NativeBridge.objectCall("BuildRuntime","path" to config.path).getLong("handle")).use {runtime ->
            val host=JSONObject("""{"editor":772,"epoch":1,"supports_preedit":true,"supports_insert":true,"owns_preedit":false,"purpose":"Text","selection_available":true,"surrounding_available":true,"enter_action":"Newline"}""")
            NativeHandle(NativeBridge.objectCall("CreateSession","runtime" to runtime.value,"context" to host).getLong("handle")).use {session ->
                fun action(value:Any) {
                    val plan=NativeBridge.objectCall("Prepare","session" to session.value,"action" to value)
                    NativeBridge.call("Ack","session" to session.value,"token" to plan.getJSONObject("token"),"outcome" to "Applied","owns_preedit" to (plan.optJSONObject("op")?.has("CompositionUpdate")==true))
                }
                lateinit var controller:PredictionController
                instrumentation.runOnMainSync {controller=PredictionController(context) {}}
                try {
                    // No commit/request exists yet: readiness proves preparation
                    // starts from lifecycle/configuration rather than first input.
                    val measurements=JSONArray()
                    fun ready(label:String,started:Long) {
                        val end=SystemClock.uptimeMillis()+10000
                        while(command("Status").optString("value")!="Ready" && SystemClock.uptimeMillis()<end)SystemClock.sleep(20)
                        assertEquals("Ready",command("Status").getString("value"))
                        measurements.put(JSONObject().put("event",label).put("ready_wait_ms",SystemClock.uptimeMillis()-started).put("process_pss_kib",android.os.Debug.getPss()))
                    }
                    var started=SystemClock.uptimeMillis()
                    instrumentation.runOnMainSync {controller.update(session.value,true)}
                    ready("cold_before_input",started)
                    assertFalse(NativeBridge.objectCall("State","session" to session.value).getBoolean("prediction_waiting"))
                    instrumentation.runOnMainSync {controller.update(session.value,false)}
                    val stop=SystemClock.uptimeMillis()+3000
                    while(command("Status").optString("value")!="Paused" && SystemClock.uptimeMillis()<stop)SystemClock.sleep(10)
                    assertEquals("Paused",command("Status").getString("value"))
                    started=SystemClock.uptimeMillis()
                    instrumentation.runOnMainSync {controller.update(session.value,true)}
                    ready("reshow",started)
                    // Memory callback uses pause; recreate the controller to
                    // exercise service-owner recreation without preserving KV.
                    started=SystemClock.uptimeMillis()
                    instrumentation.runOnMainSync {controller.pause();controller.close();controller=PredictionController(context) {};controller.update(session.value,true)}
                    ready("recreated_after_release",started)
                    File(context.filesDir,"preload-lifecycle.json").writeText(measurements.toString(2))
                    var published=false
                    // Configuration changes can cold-load even after the scoring
                    // probe. Retry with a new commit, never accept an expired result.
                    for(attempt in 0..1) {
                        action(JSONObject().put("InputText",JSONArray().put("b").put("Normal")));action("Space")
                        assertTrue(NativeBridge.objectCall("State","session" to session.value).getBoolean("prediction_waiting"))
                        instrumentation.runOnMainSync {controller.update(session.value,true)}
                        val end=SystemClock.uptimeMillis()+2000
                        while(NativeBridge.objectCall("State","session" to session.value).getBoolean("prediction_waiting") && SystemClock.uptimeMillis()<end)SystemClock.sleep(10)
                        published=NativeBridge.objectCall("State","session" to session.value).getJSONObject("view").getBoolean("prediction")
                        if(published) break
                    }
                    assertTrue("controller did not publish scored candidates; worker=${command("Status")}",published)
                    instrumentation.runOnMainSync {controller.cancel()}
                    action(JSONObject().put("InputText",JSONArray().put("b").put("Normal")))
                    assertFalse(NativeBridge.objectCall("State","session" to session.value).getJSONObject("view").getBoolean("prediction"))
                } finally {instrumentation.runOnMainSync {controller.close()}}
            }
        }
        // The lifecycle assertions below operate on a fresh explicitly configured worker.
        val configJson=JSONObject().put("enabled",true).put("model",JSONObject().put("id",manifest.getString("id")).put("version",manifest.getString("version")))
        command("Configure","root" to File(root,"models").absolutePath,"config" to configJson)
    }
    @Test fun onnxCpuLoadsScoresAndReleasesThroughNativeWorker() {
        assumeTrue(command("Features").getJSONObject("value").getBoolean("onnx"));score(fixture())
    }
    @Test fun catalogUsesTheEngineDirectoryAndProtectsSavedSelection() {
        val store=EngineStore(context)
        val loaded=store.readConfig()
        val original=loaded.getJSONObject("config")
        NativeHandle(loaded.getLong("version")).close()
        val catalog=ModelCatalog(context)
        val selection=JSONObject().put("id","instrumented-counts").put("version","v1")
        fun save(config:JSONObject) {
            NativeHandle(store.readConfig().getLong("version")).use {v->NativeHandle(store.saveConfig(config,v.value).getLong("version")).close()}
        }
        try {
            catalog.call("Import","source" to fixture().absolutePath)
            assertTrue(File(store.configFile.parentFile,"models/instrumented-counts--v1/manifest.json").isFile)
            val selected=JSONObject(original.toString()).apply {getJSONObject("prediction").put("model",selection)}
            save(selected)
            try {catalog.call("Remove","selection" to selection);fail("saved model was removed")}
            catch(expected:IllegalStateException) {assertTrue(File(store.configFile.parentFile,"models/instrumented-counts--v1/manifest.json").isFile)}
        } finally {save(original);catalog.call("Remove","selection" to selection)}
    }
    @Test fun llamaCpuScoresAnExplicitlyProvisionedLicensedPackage() {
        assumeTrue(command("Features").getJSONObject("value").getBoolean("llama"))
        val source=File(context.filesDir,"mi-test-package")
        assumeTrue("provision the pinned test package explicitly",File(source,"manifest.json").isFile)
        assumeTrue("provision an explicit prediction policy for the test package",!JSONObject(File(source,"manifest.json").readText()).isNull("prediction_policy"))
        score(source)
    }
}
