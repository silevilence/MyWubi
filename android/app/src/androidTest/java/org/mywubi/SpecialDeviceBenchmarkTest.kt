package org.mywubi

import android.os.Debug
import android.os.SystemClock
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeNotNull
import org.junit.Test
import java.io.File

/** SI-15, using the project's frozen workloads.json supplied as an instrumentation argument. */
class SpecialDeviceBenchmarkTest {
    @Test fun threeRunsOnArtWithTheFixedRuleAndCategoryCounts() {
        val workloadsPath = InstrumentationRegistry.getArguments().getString("si_workloads")
        assumeNotNull(workloadsPath)
        val ins=InstrumentationRegistry.getInstrumentation()
        val app = ins.targetContext
        val device = androidx.test.uiautomator.UiDevice.getInstance(ins)
        device.executeShellCommand("ime enable org.mywubi/.MyWubiService")
        device.executeShellCommand("ime set org.mywubi/.MyWubiService")
        device.executeShellCommand("am force-stop org.mywubi.acceptancehost")
        device.executeShellCommand("am start -W -n org.mywubi.acceptancehost/.EditorActivity")
        focusExternalEditor(device)
        val inputs = JSONObject(File(requireNotNull(workloadsPath)).readText())
        val engine = Resources.prepare(app).parentFile!!
        val root = File(app.cacheDir, "special-device-benchmark").apply { mkdirs() }
        val report = File(app.filesDir, "si-device-benchmark.jsonl")
        val multi = inputs.getJSONObject("multi_category")
        val kinds = multi.getJSONArray("kinds")
        val count = multi.getInt("entries_per_kind")
        val hits = inputs.getJSONObject("same_code").getJSONArray("rules")
        val misses = inputs.getJSONObject("misses").getJSONArray("rules")
        val host = JSONObject("""{"editor":1,"epoch":1,"supports_preedit":false,"supports_insert":true,"owns_preedit":false,"purpose":"Text","selection_available":true,"surrounding_available":false,"enter_action":"Newline"}""")
        fun sections(rules: JSONArray, limit: Int = rules.length()): String = buildString {
            repeat(limit) { i ->
                val rule = rules.getJSONObject(i)
                appendLine("[[shortcuts.rules]]")
                appendLine("code = " + JSONObject.quote(rule.getString("code")))
                appendLine("position = " + rule.getInt("position"))
                appendLine("kind = \"date_time\"")
                appendLine("format = " + JSONObject.quote(rule.getString("format")))
                appendLine("enabled = true")
            }
        }
        repeat(kinds.length()) { k ->
            val kind = kinds.getString(k)
            val entries = JSONArray()
            repeat(count) { i ->
                entries.put(JSONObject().put("text", "$kind$i").put("name", "样例$i")
                    .put("triggers", JSONArray().put(multi.getString("trigger"))).put("aliases", JSONArray())
                    .put("tags", JSONArray()).put("order", i).put("sources", JSONArray().put("synthetic")))
            }
            val source = JSONObject().put("id", "synthetic").put("url", "local:SI-01")
                .put("revision", "1").put("license", "MIT").put("notice", "Synthetic benchmark data")
            File(root, "$kind.mwe").writeText("# MyWubi Expressions 1\n" + JSONObject().put("kind", kind)
                .put("name", kind).put("version", "SI-01-fixed").put("entries", entries)
                .put("sources", JSONArray().put(source)).toString())
        }
        val budgetFailures = mutableListOf<String>()
        val store = EngineStore(app)
        fun activate(config: JSONObject, sample: () -> Unit) {
            NativeHandle(store.readConfig().getLong("version")).use { current ->
                NativeHandle(store.saveConfig(config,current.value).getLong("version")).close()
            }
            val prefs = EngineUpdates.preferences(app)
            val request = prefs.getString("requested",null)
            val deadline = SystemClock.elapsedRealtime() + 20000
            do {
                sample()
                if (prefs.getString("applied",null) == request) return
                SystemClock.sleep(25)
            } while (SystemClock.elapsedRealtime() < deadline)
            error("Production IME did not apply the workload: ${prefs.getString("error",null)}")
        }
        report.writeText("")
        for (run in 1..3) for (scenario in listOf("disabled", "misses", "multi", "same_code", "full")) {
            val full = scenario == "full"
            val table = if (full) File(engine, "tables/wubi86.dict") else File(root, "main.dict").apply {
                writeText((if (scenario == "multi") "jjad" else "jjaa") + "\t基线\t100\n")
            }
            val path = File(root, "config.toml")
            path.writeText(buildString {
                appendLine("[basic]\nauto_commit_unique=false\n[dictionary]")
                appendLine("system_table = \"${table.absolutePath}\"\nenable_user_dict=false")
                if (scenario == "multi" || full) {
                    val active = if (full) specialKinds else (0 until kinds.length()).map { kinds.getString(it) }
                    for (kind in active) {
                        val source = if (full) File(engine, "expressions/$kind.mwe") else File(root, "$kind.mwe")
                        require(source.isFile) { source.path }
                        appendLine("[expressions.$kind]\nappend=true\nsearch=true\nresource=\"${source.absolutePath}\"")
                    }
                }
                if (scenario == "misses" || full) append(sections(misses, if (full) 992 else 1024))
                if (scenario == "same_code" || full) append(sections(hits))
            })
            // The visible service already owns a full Wubi dictionary. Activate the
            // measured configuration through that service, including both generations
            // during the normal hot reload. A second independent benchmark runtime
            // would remain resident alongside it for the entire measurement.
            if (full) {
                val before = store.readConfig()
                NativeHandle(before.getLong("version")).close()
                val fixture = NativeBridge.objectCall("ReadConfig","path" to path.path)
                NativeHandle(fixture.getLong("version")).close()
                try {
                    var peak = Debug.getPss()
                    val start = SystemClock.elapsedRealtimeNanos()
                    activate(fixture.getJSONObject("config")) { peak = maxOf(peak,Debug.getPss()) }
                    val elapsed = (SystemClock.elapsedRealtimeNanos()-start)/1e6
                    val resident = Debug.getPss()
                    peak = maxOf(peak,resident)
                    val row = JSONObject().put("run",run).put("scenario",scenario)
                        .put("build_ms",elapsed).put("pss_kib",peak).put("resident_pss_kib",resident)
                        .put("measurement","visible IME, including reload peak")
                    report.appendText(row.toString()+"\n")
                    if (elapsed > 3000) budgetFailures.add("$run/full build")
                    if (peak > 256*1024) budgetFailures.add("$run/full PSS")
                } finally {
                    activate(before.getJSONObject("config")) { }
                }
                continue
            }
            val start = SystemClock.elapsedRealtimeNanos()
            val built = NativeBridge.objectCall("BuildRuntime", "path" to path.path)
            NativeHandle(built.getLong("handle")).use { runtime ->
                val buildMs = (SystemClock.elapsedRealtimeNanos() - start) / 1e6
                val row = JSONObject().put("run", run).put("scenario", scenario).put("build_ms", buildMs)
                if (!full) {
                    val samples = if (scenario in listOf("multi", "same_code")) 200 else 1000
                    val elapsed = ArrayList<Long>(samples)
                    val text = if (scenario == "disabled") "jjaad" else "jjad"
                    NativeHandle(NativeBridge.objectCall("CreateSession", "runtime" to runtime.value, "context" to host).getLong("handle")).use { session ->
                        fun action(value: Any) {
                            val plan = NativeBridge.objectCall("Prepare", "session" to session.value, "action" to value)
                            NativeBridge.call("Ack", "session" to session.value, "token" to plan.getJSONObject("token"), "outcome" to "Applied", "owns_preedit" to false)
                        }
                        var lastCount = 0
                        repeat(20 + samples) { i ->
                            action("Cancel")
                            val begin = SystemClock.elapsedRealtimeNanos()
                            for (c in text) action(JSONObject().put("InputCode", c.toString()))
                            val candidates = NativeBridge.objectCall("Candidates", "session" to session.value, "offset" to 0, "count" to 256)
                            val dt = SystemClock.elapsedRealtimeNanos() - begin
                            if (i >= 20) elapsed.add(dt)
                            lastCount = candidates.getInt("total")
                        }
                        if (scenario == "multi") assertTrue("Missing multi-category entries: $lastCount", lastCount >= count * kinds.length())
                        if (scenario == "same_code") assertTrue("Missing date rules: $lastCount", lastCount >= hits.length())
                        row.put("candidates", lastCount)
                    }
                    elapsed.sort()
                    fun percentile(p: Int) = elapsed[(samples * p + 99) / 100 - 1] / 1e6
                    val (p95, p99) = when (scenario) { "disabled" -> 25 to 50; "misses" -> 35 to 60; else -> 150 to 250 }
                    row.put("warmup", 20).put("samples", samples).put("p95_ms", percentile(95)).put("p99_ms", percentile(99))
                    if (percentile(95) > p95 || percentile(99) > p99) budgetFailures.add("$run/$scenario latency")
                } else if (buildMs > 3000) budgetFailures.add("$run/full build")
                val pss = Debug.getPss()
                row.put("pss_kib", pss)
                if (pss > 256 * 1024) budgetFailures.add("$run/$scenario PSS")
                report.appendText(row.toString() + "\n")
            }
        }
        assertTrue(budgetFailures.toString(), budgetFailures.isEmpty())
    }
}
