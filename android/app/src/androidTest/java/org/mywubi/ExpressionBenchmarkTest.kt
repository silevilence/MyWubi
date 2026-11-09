package org.mywubi

import android.os.Build
import android.os.SystemClock
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test
import java.io.File

/** Same table/input with both append switches off/on. Timings include JNI, not UI/host work. */
class ExpressionBenchmarkTest {
    @Test fun compareAppendSwitchesWithFullLocalLibraries() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val defaults = Resources.prepare(context)
        val root = File(context.cacheDir, "expression-benchmark").apply { mkdirs() }
        File(root, "main.dict").writeText("a\t开心\n")
        val report = JSONObject().put("sdk", Build.VERSION.SDK_INT).put("fingerprint", Build.FINGERPRINT)
            .put("scope", "BuildRuntime; InputCode+Ack+Candidates through JNI, excludes UI and host")
        val host = JSONObject("""{"editor":11,"epoch":1,"supports_preedit":false,"supports_insert":true,"owns_preedit":false,"purpose":"Text","selection_available":true,"surrounding_available":true,"enter_action":"Newline"}""")
        for (enabled in listOf(false, true)) {
            val config = File(root, "config.toml").apply {
                writeText("""[basic]
auto_commit_unique = false
[dictionary]
system_table = "main.dict"
user_table = "user.dict"
[expressions.emoji]
append = $enabled
search = false
resource = ${JSONObject.quote(File(defaults.parentFile,"expressions/emoji.mwe").path).replace("\\/", "/")}
[expressions.kaomoji]
append = $enabled
search = false
resource = ${JSONObject.quote(File(defaults.parentFile,"expressions/kaomoji.mwe").path).replace("\\/", "/")}
""")
            }
            val started = SystemClock.elapsedRealtimeNanos()
            NativeHandle(NativeBridge.objectCall("BuildRuntime", "path" to config.path).getLong("handle")).use { runtime ->
                val loadMs = (SystemClock.elapsedRealtimeNanos() - started) / 1_000_000.0
                NativeHandle(NativeBridge.objectCall("CreateSession", "runtime" to runtime.value, "context" to host).getLong("handle")).use { session ->
                    fun action(value: Any) {
                        val plan = NativeBridge.objectCall("Prepare", "session" to session.value, "action" to value)
                        NativeBridge.call("Ack", "session" to session.value, "token" to plan.getJSONObject("token"),
                            "outcome" to "Applied", "owns_preedit" to false)
                    }
                    val samples = ArrayList<Long>()
                    var total = 0
                    repeat(110) { iteration ->
                        action("Cancel")
                        val begin = SystemClock.elapsedRealtimeNanos()
                        action(JSONObject().put("InputCode", "a"))
                        val candidates = NativeBridge.objectCall("Candidates", "session" to session.value, "offset" to 0, "count" to 256)
                        val elapsed = SystemClock.elapsedRealtimeNanos() - begin
                        total = candidates.getInt("total")
                        assertEquals(if(enabled) 858 else 1, total)
                        if(iteration >= 10) samples.add(elapsed)
                    }
                    samples.sort()
                    report.put(if(enabled) "enabled" else "disabled", JSONObject().put("runtime_build_ms", loadMs)
                        .put("candidates", total).put("samples", samples.size).put("warmup", 10)
                        .put("p50_us", samples[50] / 1000.0).put("p95_us", samples[95] / 1000.0))
                }
            }
        }
        File(context.getExternalFilesDir(null), "expression-benchmark.json").writeText(report.toString(2))
    }
}
