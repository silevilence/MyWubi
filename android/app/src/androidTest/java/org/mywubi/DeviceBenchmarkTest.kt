package org.mywubi

import android.os.Build
import android.os.Debug
import android.os.SystemClock
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File
import java.security.MessageDigest

/** Opt-in measurement. JNI timings exclude UI rendering and remote host completion. */
class DeviceBenchmarkTest {
    @Test fun recordRealDictionaryJniLatencyAndMemory() {
        val args = InstrumentationRegistry.getArguments()
        val source = args.getString("benchmark_main")
        assumeTrue("Supply benchmark_main with a real main table", source != null)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val defaults = Resources.prepare(context)
        val root = File(context.cacheDir, "device-benchmark").apply { mkdirs() }
        val main = File(root, "main.dict")
        File(source!!).inputStream().use { input -> main.outputStream().use { input.copyTo(it) } }
        val originalHash = MessageDigest.getInstance("SHA-256").digest(main.readBytes()).joinToString("") { "%02x".format(it) }
        val table = NativeBridge.objectCall("ReadTable", "path" to main.path)
        NativeHandle(table.getLong("version")).use { version ->
            val config = table.getJSONObject("config")
            config.getJSONObject("temporary").put("pinyin_table", File(defaults.parentFile, "tables/pinyin.dict").path)
                .put("english_table", File(defaults.parentFile, "tables/english.dict").path)
            val saved = NativeBridge.objectCall("SaveTable", "original" to main.path, "target" to main.path, "config" to config, "entries" to table.getJSONArray("entries"), "expected" to version.value)
            NativeHandle(saved.getLong("version")).close()
        }
        val config = File(root, "config.toml").apply { writeText("[basic]\nauto_commit_unique=false\n[dictionary]\nsystem_table=\"main.dict\"\nuser_table=\"user.dict\"\n") }
        val count = (args.getString("benchmark_samples")?.toIntOrNull() ?: 1000).coerceIn(100, 10000)
        fun quantiles(values: List<Long>): JSONObject {
            val sorted = values.sorted()
            return JSONObject().put("count", sorted.size).put("p50_us", sorted[(sorted.size * .50).toInt()] / 1000.0)
                .put("p95_us", sorted[(sorted.size * .95).toInt()] / 1000.0).put("p99_us", sorted[(sorted.size * .99).toInt()] / 1000.0)
        }
        val start = SystemClock.elapsedRealtimeNanos()
        val runtime = NativeHandle(NativeBridge.objectCall("BuildRuntime", "path" to config.path).getLong("handle"))
        val loadMs = (SystemClock.elapsedRealtimeNanos() - start) / 1_000_000.0
        val host = JSONObject("""{"editor":1,"epoch":1,"supports_preedit":false,"supports_insert":true,"owns_preedit":false,"purpose":"Text","selection_available":true,"surrounding_available":true,"enter_action":"Newline"}""")
        val results = JSONObject().put("model", Build.MODEL).put("api", Build.VERSION.SDK_INT)
            .put("abis", JSONArray(Build.SUPPORTED_ABIS.toList())).put("page_size", android.system.Os.sysconf(android.system.OsConstants._SC_PAGESIZE))
            .put("main_sha256", originalHash).put("main_entries", table.getJSONArray("entries").length())
            .put("initial_runtime_build_ms", loadMs).put("pss_before_kib", Debug.getPss())
        runtime.use {
            for (mode in listOf("Main", "Pinyin", "English")) {
                val actionTimes = mutableListOf<Long>()
                val windowTimes = mutableListOf<Long>()
                NativeHandle(NativeBridge.objectCall("CreateSession", "runtime" to runtime.value, "context" to host).getLong("handle")).use { session ->
                    fun action(value: Any) {
                        val plan = NativeBridge.objectCall("Prepare", "session" to session.value, "action" to value)
                        NativeBridge.call("Ack", "session" to session.value, "token" to plan.getJSONObject("token"), "outcome" to "Applied", "owns_preedit" to false)
                    }
                    if (mode == "English") action(JSONObject().put("SetBaseMode", "English"))
                    repeat(count) {
                        if(mode == "Pinyin") action(JSONObject().put("BeginTemporary", "Pinyin"))
                        val begin = SystemClock.elapsedRealtimeNanos()
                        action(JSONObject().put("InputText", JSONArray().put(if(mode == "Pinyin") "n" else "a").put("Normal")))
                        actionTimes.add(SystemClock.elapsedRealtimeNanos() - begin)
                        val query = SystemClock.elapsedRealtimeNanos()
                        NativeBridge.call("Candidates", "session" to session.value, "offset" to 0, "count" to 256)
                        windowTimes.add(SystemClock.elapsedRealtimeNanos() - query)
                        action("Cancel")
                    }
                }
                results.put(mode, JSONObject().put("prepare_ack", quantiles(actionTimes)).put("expanded_candidates", quantiles(windowTimes)).put("pss_kib", Debug.getPss()))
            }
        }
        results.put("pss_after_release_kib", Debug.getPss())
        File(context.filesDir, "device-benchmark.json").writeText(results.toString(2))
    }
}
