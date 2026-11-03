package org.mywubi

import android.os.SystemClock
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.delay
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

class UpdateIsolationTest {
    @Test fun slowCheckAndDownloadPreserveEveryEditAndCandidateOrder() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val root = File(context.cacheDir,"au-isolation-${System.nanoTime()}").apply {mkdirs()}
        File(root,"main.dict").writeText("aa\t甲\t20\naa\t乙\t10\nab\t丙\t5\n")
        val config=File(root,"config.toml").apply {writeText("[basic]\nauto_commit_unique=false\n[dictionary]\nsystem_table=\"main.dict\"\nuser_table=\"user.dict\"\n")}
        val samples = JSONObject().put("device",android.os.Build.MODEL).put("api",android.os.Build.VERSION.SDK_INT)
        val gate = CountDownLatch(1)
        val calls = AtomicInteger()
        val update = AppUpdate("0.5.1",2_000_000_000,26,"test","${RELEASE_ROOT}v0.5.1/MyWubi-0.5.1-Android.apk","a".repeat(64),listOf("x86_64"))
        lateinit var checks: AppUpdates
        lateinit var downloads: UpdateDownload
        instrumentation.runOnMainSync {
            checks = AppUpdates(context,"isolation-${System.nanoTime()}") {
                calls.incrementAndGet(); gate.await(20,TimeUnit.SECONDS); throw java.net.SocketTimeoutException()
            }
            downloads = UpdateDownload(context,File(root,"download")) { _, target, progress ->
                target.outputStream().use { output ->
                    repeat(200) { index -> output.write(ByteArray(65536)); progress((index+1)*65536L,200*65536L); delay(20) }
                }
            }
        }
        while(downloads.state.value.busy) Thread.sleep(10)
        NativeHandle(NativeBridge.objectCall("BuildRuntime","path" to config.path).getLong("handle")).use { runtime ->
            fun trace(label: String): List<String> {
                val trace = mutableListOf<String>()
                val timings = mutableListOf<Long>()
                instrumentation.runOnMainSync {
                    for (purpose in listOf("Text","Direct","Password")) {
                        val host = JSONObject("""{"editor":1,"epoch":1,"supports_preedit":false,"supports_insert":true,"owns_preedit":false,"purpose":"$purpose","selection_available":true,"surrounding_available":true,"enter_action":"Newline"}""")
                        NativeHandle(NativeBridge.objectCall("CreateSession","runtime" to runtime.value,"context" to host).getLong("handle")).use { session ->
                            for (mode in if (purpose == "Text") listOf("Main","English") else listOf("Direct")) {
                                fun action(value: Any) {
                                    val start = SystemClock.elapsedRealtimeNanos()
                                    val plan = NativeBridge.objectCall("Prepare","session" to session.value,"action" to value)
                                    trace.add(plan.get("op").toString())
                                    NativeBridge.call("Ack","session" to session.value,"token" to plan.getJSONObject("token"),"outcome" to "Applied","owns_preedit" to false)
                                    val candidates = NativeBridge.objectCall("Candidates","session" to session.value,"offset" to 0,"count" to 256).getJSONArray("candidates")
                                    trace.add((0 until candidates.length()).joinToString { candidates.getJSONObject(it).getString("text") })
                                    timings.add(SystemClock.elapsedRealtimeNanos()-start)
                                }
                                if (purpose == "Text") action(JSONObject().put("SetBaseMode",mode))
                                repeat(100) {
                                    action(JSONObject().put("InputText",JSONArray().put("a").put("Normal")))
                                    action(JSONObject().put("InputText",JSONArray().put("a").put("Normal")))
                                    action("Space")
                                    action(JSONObject().put("InsertLiteral","1"))
                                }
                            }
                        }
                    }
                }
                timings.sort()
                samples.put(label,JSONObject().put("samples",timings.size).put("p50_us",timings[timings.size/2]/1000.0)
                    .put("p95_us",timings[(timings.size*.95).toInt()]/1000.0).put("p99_us",timings[(timings.size*.99).toInt()]/1000.0))
                return trace
            }
            val baseline = trace("baseline")
            try {
                instrumentation.runOnMainSync { checks.check(); downloads.start(update) }
                assertTrue(checks.state.value.checking)
                assertEquals(DownloadPhase.Downloading,downloads.state.value.phase)
                assertEquals(baseline,trace("checking_and_downloading"))
                instrumentation.runOnMainSync { repeat(100) { checks.check() } }
                assertEquals(1,calls.get())
            } finally {
                gate.countDown()
                instrumentation.runOnMainSync { downloads.cancel() }
            }
            val end = SystemClock.uptimeMillis()+15000
            while((checks.state.value.checking || downloads.state.value.busy) && SystemClock.uptimeMillis()<end) Thread.sleep(20)
            assertFalse(checks.state.value.checking)
            assertEquals(DownloadPhase.Cancelled,downloads.state.value.phase)
            assertEquals(baseline,trace("offline_and_cancelled"))
        }
        File(context.filesDir,"au-input-latency.json").writeText(samples.toString(2))
        root.deleteRecursively()
    }
}
