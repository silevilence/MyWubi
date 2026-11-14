package org.mywubi

import android.os.Debug
import android.os.SystemClock
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Test
import java.io.File

/** Same APK, dictionaries and character sequences; excludes touch/display and host IPC. */
class KeyboardBenchmarkTest {
    @Test fun compareExactAndGroupedJniOnLocalRealTables() {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val defaults=Resources.prepare(context)
        val main=File(defaults.parentFile,"tables/wubi86.dict")
        val root=File(context.cacheDir,"ak-jni-benchmark").apply {mkdirs()}
        val config=File(root,"config.toml").apply {writeText("[basic]\nauto_commit_unique=false\n[dictionary]\nsystem_table=\"${main.path}\"\nenable_user_dict=false\n")}
        val host=JSONObject("""{"editor":3,"epoch":1,"supports_preedit":false,"supports_insert":true,"owns_preedit":false,"purpose":"Text","selection_available":true,"surrounding_available":true,"enter_action":"Newline"}""")
        val identity=JSONObject("""{"id":"benchmark.groups","version":"1","revision":1}""")
        val definition=JSONObject("""{"charset":"abcdefghijklmnopqrstuvwxyz","groups":{"g":"ghi","l":"jkl","n":"mno","i":"ghi"}}""")
        fun quantiles(values:List<Long>):JSONObject {val sorted=values.sorted();return JSONObject().put("count",sorted.size)
            .put("p50_us",sorted[sorted.size/2]/1000.0).put("p95_us",sorted[sorted.size*95/100]/1000.0).put("p99_us",sorted[sorted.size*99/100]/1000.0)}
        val result=JSONObject().put("model",android.os.Build.MODEL).put("api",android.os.Build.VERSION.SDK_INT)
            .put("pss_before_kib",Debug.getPss()).put("scope","JNI Prepare/Ack + candidate fetch; no screen latency")
        NativeHandle(NativeBridge.objectCall("BuildRuntime","path" to config.path).getLong("handle")).use {runtime ->
            for(mode in listOf("Main","Pinyin")) for(grouped in listOf(false,true)) {
                val elapsed=mutableListOf<Long>()
                NativeHandle(NativeBridge.objectCall("CreateSession","runtime" to runtime.value,"context" to host).getLong("handle")).use {session ->
                    fun action(value:Any) {val plan=NativeBridge.objectCall("Prepare","session" to session.value,"action" to value)
                        NativeBridge.call("Ack","session" to session.value,"token" to plan.getJSONObject("token"),"outcome" to "Applied","owns_preedit" to false)}
                    if(grouped) action(JSONObject().put("BindMapping",JSONArray().put(identity).put(definition)))
                    repeat(550) { iteration ->
                        if(mode=="Pinyin") action(JSONObject().put("BeginTemporary","Pinyin"))
                        val begin=SystemClock.elapsedRealtimeNanos()
                        for(c in if(mode=="Main") "ggll" else "ni") {
                            action(if(grouped) JSONObject().put("InputGroup",JSONArray().put(identity).put(c.toString()))
                                else JSONObject().put("InputText",JSONArray().put(c.toString()).put("Normal")))
                        }
                        val candidates=NativeBridge.objectCall("Candidates","session" to session.value,"offset" to 0,"count" to 50)
                        val duration=SystemClock.elapsedRealtimeNanos()-begin
                        org.junit.Assert.assertTrue("Real table candidates must be queried",candidates.toString().contains(if(mode=="Main") "王" else "你"))
                        if(iteration>=50) elapsed.add(duration)
                        action("Cancel")
                    }
                }
                result.put(mode+(if(grouped) "_groups" else "_exact"),quantiles(elapsed))
            }
        }
        result.put("pss_after_kib",Debug.getPss())
        File(context.filesDir,"ak-jni-benchmark.json").writeText(result.toString(2))
    }
}
