package org.mywubi

import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.*
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/** MI-18/35 CPU prediction on a charged physical phone. Record failures as well as timely results. */
class PhysicalModelDurationTest {
    @Test fun thirtyMinutesOfCpuPredictionsWithResourceSamples() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("model_duration")=="true")
        val app=InstrumentationRegistry.getInstrumentation().targetContext
        androidx.test.uiautomator.UiDevice.getInstance(InstrumentationRegistry.getInstrumentation()).executeShellCommand("am start -W -n org.mywubi/.MainActivity")
        val root=File(EngineStore(app).configFile.parentFile,"models")
        require(File(root,"qwen25-words--q4km-v1/manifest.json").isFile)
        val report=File(app.filesDir,"physical-model-duration.jsonl")
        fun call(op:String, vararg fields:Pair<String,Any>):JSONObject {
            val input=JSONObject().put("op",op)
            fields.forEach { (k,v)->input.put(k,v) }
            val result=JSONObject(NativeBridge.inference(input.toString().toByteArray()).toString(Charsets.UTF_8))
            check(result.getBoolean("ok")) { result.toString() }
            return result
        }
        val config=JSONObject().put("enabled",true).put("loading","preload_keep").put("wait_ms",1000)
            .put("threads",2).put("max_candidates",1).put("device","cpu")
            .put("model",JSONObject().put("id","qwen25-words").put("version","q4km-v1"))
        val power=app.getSystemService(Context.POWER_SERVICE) as PowerManager
        val battery=app.getSystemService(Context.BATTERY_SERVICE) as BatteryManager
        val started=SystemClock.elapsedRealtime()
        var issued=0;var completed=0
        report.writeText("")
        try {
            call("Prepare","root" to root.absolutePath,"config" to config)
            while(call("Status").optString("value")!="Ready" && SystemClock.elapsedRealtime()-started<30000) SystemClock.sleep(50)
            assertEquals("Ready",call("Status").getString("value"))
            val ready=SystemClock.elapsedRealtime()
            report.appendText(JSONObject().put("load_ms",ready-started).toString()+"\n")
            while(SystemClock.elapsedRealtime()-ready < 30*60*1000) {
                val id=++issued
                val key=JSONObject("""{"edit":{"session":9183,"generation":1,"editor":1,"epoch":1,"runtime":1},"query":{"engine":1,"version":1},"view":1,"context":1,"model":1,"config":1}""").put("request",id)
                val q=JSONObject().put("protocol",1).put("key",key).put("task","Predict").put("language","zh")
                    .put("prefix","北京").put("code","").put("candidates",JSONArray())
                val sent=SystemClock.elapsedRealtime()
                call("Submit","request" to q)
                var value:JSONObject?=null
                while(SystemClock.elapsedRealtime()-sent<1100 && value==null) {
                    value=call("Poll","key" to key).optJSONObject("value")
                    if(value==null) SystemClock.sleep(4)
                }
                val latency=SystemClock.elapsedRealtime()-sent
                val match=value?.optJSONObject("Ok")?.optJSONArray("words")?.toString()==JSONArray().put("市").toString()
                if(match && latency<=1000) completed++
                val row=JSONObject().put("request",id).put("seconds",(sent-ready)/1000).put("response_ms",latency)
                    .put("expected_word",match).put("received",value!=null)
                if(id%10==1) {
                    val info=app.registerReceiver(null,IntentFilter(Intent.ACTION_BATTERY_CHANGED))
                    row.put("pss_kib",Debug.getPss()).put("cpu_ms",Process.getElapsedCpuTime())
                        .put("temp_tenths_c",info?.getIntExtra(BatteryManager.EXTRA_TEMPERATURE,-1))
                        .put("level",info?.getIntExtra(BatteryManager.EXTRA_LEVEL,-1))
                        .put("plug",info?.getIntExtra(BatteryManager.EXTRA_PLUGGED,-1))
                        .put("current_ua",battery.getIntProperty(BatteryManager.BATTERY_PROPERTY_CURRENT_NOW))
                    if(Build.VERSION.SDK_INT>=29)row.put("thermal_status",power.currentThermalStatus)
                }
                report.appendText(row.toString()+"\n")
                call("Invalidate")
                SystemClock.sleep((2000-(SystemClock.elapsedRealtime()-sent)).coerceAtLeast(0))
            }
            report.appendText(JSONObject().put("total",issued).put("correct_before_deadline",completed)
                .put("duration_ms",SystemClock.elapsedRealtime()-ready).toString()+"\n")
            assertEquals("Requests with expected word within 1000 ms",issued,completed)
        } finally { call("Close") }
    }
}
