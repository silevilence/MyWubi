package org.mywubi

import android.os.SystemClock
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.util.regex.Pattern

class SpecialPositionHostTest {
    // Scrolling can recycle a Compose node between two UI Automator calls.
    // Copy labels and touch bounds together before using a viewport.
    private fun viewport(dev: UiDevice, pkg: String): List<Pair<String, android.graphics.Rect>> {
        val deadline = SystemClock.uptimeMillis() + 3000
        while (true) {
            try {
                InstrumentationRegistry.getInstrumentation().uiAutomation.clearCache()
                return dev.findObjects(By.pkg(pkg).descStartsWith("候选 ")).map {
                    it.contentDescription to it.visibleBounds
                }.filter { !it.second.isEmpty }.sortedWith(compareBy({ it.second.top }, { it.second.left }))
            } catch (stale: StaleObjectException) {
                if (SystemClock.uptimeMillis() >= deadline) throw stale
                SystemClock.sleep(50)
            }
        }
    }

    @Test fun sparseDuplicateAndConflictingRulesCommitThroughTheKeyboard() {
        val casesFile = InstrumentationRegistry.getArguments().getString("si_cases")
        org.junit.Assume.assumeNotNull(casesFile)
        val ins = InstrumentationRegistry.getInstrumentation()
        val app = ins.targetContext
        val dev = UiDevice.getInstance(ins)
        val store = EngineStore(app)
        val cases = JSONObject(File(requireNotNull(casesFile)).readText()).getJSONArray("cases")
        fun fixture(id: String) = (0 until cases.length()).map(cases::getJSONObject).first { it.getString("id") == id }
        fun strings(values: JSONArray) = (0 until values.length()).map(values::getString)
        val previous = dev.executeShellCommand("settings get secure default_input_method").trim()
        val original = store.readConfig()
        NativeHandle(original.getLong("version")).close()
        val config = JSONObject(original.getJSONObject("config").toString())
        val root = File(app.cacheDir, "special-host-cases").apply { mkdirs() }
        val host = "org.mywubi.acceptancehost"
        val field = By.pkg(host).clazz("android.widget.EditText")
        val pkg = app.packageName
        val ime = "$pkg/org.mywubi.MyWubiService"
        fun save(value: JSONObject) {
            NativeHandle(store.readConfig().getLong("version")).use { old ->
                NativeHandle(store.saveConfig(value, old.value).getLong("version")).close()
            }
        }
        fun key(label: String) = requireNotNull(findFresh(dev, By.pkg(pkg).desc(label).enabled(true), 5000)) { label }.click()
        fun openEditor() {
            dev.executeShellCommand("am start -W -f 0x10008000 -n $host/.EditorActivity")
            focusExternalEditor(dev)
            if (dev.hasObject(By.pkg(pkg).desc("英 → 中"))) key("英 → 中")
        }
        fun rule(position: Int, text: String) = JSONObject().put("code","jjad").put("position",position)
            .put("kind","date_time").put("format","'$text'").put("enabled",true)
        fun activate(name: String, words: List<String>, rules: JSONArray, wildcard: Boolean = false) {
            val table = File(root, "$name.dict")
            val header = if (wildcard) "---\nwildcard_key: j\n---\n" else ""
            table.writeText(header + "z\t尾\t1\n" + words.mapIndexed { i, word -> "${if (wildcard) "aaad" else "jjad"}\t$word\t${100-i}\n" }.joinToString(""))
            config.getJSONObject("dictionary").put("system_table",table.path).put("enable_user_dict",false)
            config.getJSONObject("basic").put("auto_commit_unique",false)
            config.put("shortcuts",JSONObject().put("rules",rules))
            save(config)
            val prefs = EngineUpdates.preferences(app)
            val until = SystemClock.elapsedRealtime() + 20000
            while (prefs.getString("requested",null) != prefs.getString("applied",null) && SystemClock.elapsedRealtime() < until) SystemClock.sleep(50)
            assertEquals(prefs.getString("error",null),prefs.getString("requested",null),prefs.getString("applied",null))
        }
        fun checkOrder(expected: List<String>, dynamic: List<Boolean>, selected: Int) {
            openEditor()
            for ((index, ch) in "jjad".withIndex()) {
                key(ch.toString())
                assertTrue(awaitHostText(dev, field, "jjad".take(index + 1), 3000))
            }
            key("展开")
            val grid = requireNotNull(findFresh(dev, By.desc("全部候选"), 5000))
            val sequence = expected.zip(dynamic)
            fun rows(items: List<Pair<String, android.graphics.Rect>>) = items.map { (name, _) ->
                name.removePrefix("候选 ").substringBefore("，") to name.contains("，日期时间")
            }
            val first = viewport(dev, pkg)
            assertEquals(sequence.take(first.size), rows(first))
            val bounds = if (first.size == expected.size) first[selected].second else {
                grid.scroll(Direction.DOWN, 1f)
                val last = viewport(dev, pkg)
                assertTrue("Unobserved middle candidates", first.size + last.size >= expected.size)
                assertEquals(sequence.takeLast(last.size), rows(last))
                if (selected >= expected.size - last.size) last[selected - expected.size + last.size].second
                else {
                    requireNotNull(findFresh(dev, By.desc("全部候选"), 5000)).scroll(Direction.UP, 1f)
                    val top = viewport(dev, pkg)
                    assertEquals(rows(first), rows(top))
                    top[selected].second
                }
            }
            assertTrue(dev.click(bounds.centerX(), bounds.centerY()))
            assertTrue(awaitHostText(dev, field, expected[selected], 3000))
        }
        try {
            dev.executeShellCommand("ime enable $ime")
            dev.executeShellCommand("ime set $ime")
            openEditor()
            config.put("prediction",JSONObject()).put("expressions",JSONObject())
            val variants = fixture("SI-C03").getJSONArray("variants")
            for (i in 0 until variants.length()) {
                val v = variants.getJSONObject(i)
                val positions = v.getJSONArray("positions")
                activate("sparse-$i",strings(v.getJSONArray("base")),JSONArray()
                    .put(rule(positions.getInt(0),"日期甲")).put(rule(positions.getInt(1),"日期乙")))
                val expected = strings(v.getJSONArray("expected"))
                checkOrder(expected,expected.map { it.startsWith("日期") },expected.lastIndex)
            }
            for (id in listOf("SI-C04","SI-C05")) {
                val case = fixture(id)
                val base = case.getJSONArray("base")
                val ordinary = (0 until base.length()).map(base::getJSONObject)
                    .filter { it.getString("source") == "ordinary" }.map { it.getString("text") }
                if (id == "SI-C05") {
                    val library = File(root,"symbols.mwe")
                    library.writeText("# MyWubi Expressions 1\n" +
                        """{"kind":"symbols","name":"同文","version":"1","sources":[{"id":"t","url":"local:test","revision":"1","license":"MIT","notice":"test"}],"entries":[{"text":"X","name":"符号","order":0,"triggers":["甲"],"sources":["t"]}]}""")
                    config.put("expressions",JSONObject().put("symbols",JSONObject().put("append",true).put("resource",library.path)))
                }
                val rows = case.getJSONArray("rules")
                val rules = JSONArray()
                for (i in 0 until rows.length()) {
                    val row = rows.getJSONObject(i)
                    rules.put(rule(row.getInt("position"),row.getString("text")))
                }
                activate(id,ordinary,rules)
                val expected = strings(case.getJSONArray("expected_texts"))
                val sources = strings(case.getJSONArray("expected_sources")).map { it.startsWith("dynamic:") }
                val repeated = expected.groupingBy { it }.eachCount().filterValues { it > 1 }.keys
                for (index in expected.indices) {
                    if (expected[index] in repeated) checkOrder(expected,sources,index)
                }
            }
            config.put("expressions",JSONObject())
            val rows = JSONArray().put(rule(1,"恢复"))
            activate("conflict",listOf("甲"),rows,wildcard=true)
            checkOrder(listOf("甲"),listOf(false),0)
            activate("restored",listOf("甲"),rows)
            checkOrder(listOf("恢复","甲"),listOf(true,false),0)
        } finally {
            save(original.getJSONObject("config"))
            dev.executeShellCommand("am force-stop $host")
            dev.executeShellCommand("ime set $previous")
        }
    }

    @Test fun globalPositionsAndReachablePrefixCommitToTheExternalEditor() {
        org.junit.Assume.assumeTrue(InstrumentationRegistry.getArguments().getString("physical_acceptance")=="true")
        val ins=InstrumentationRegistry.getInstrumentation()
        val app=ins.targetContext
        val dev=UiDevice.getInstance(ins)
        val store=EngineStore(app)
        val original=store.readConfig()
        NativeHandle(original.getLong("version")).close()
        val cfg=JSONObject(original.getJSONObject("config").toString())
        val previous=dev.executeShellCommand("settings get secure default_input_method").trim()
        val host="org.mywubi.acceptancehost"
        val report=File(app.filesDir,"special-positions.jsonl")
        val table=File.createTempFile("positions-",".dict",app.cacheDir)
        table.writeText("---\nmax_code_len: 4\n---\njjaa\t独\t100\n"+(0..11).joinToString("\n") { "jjad\t普通$it\t${100-it}" }+"\n")
        cfg.getJSONObject("dictionary").put("system_table",table.absolutePath).put("enable_user_dict",false)
        cfg.put("prediction",JSONObject()).put("expressions",JSONObject())
        fun save(value:JSONObject, waitForReload:Boolean = true) {
            NativeHandle(store.readConfig().getLong("version")).use { NativeHandle(store.saveConfig(value,it.value).getLong("version")).close() }
            if (!waitForReload) return
            val prefs=EngineUpdates.preferences(app)
            val deadline=SystemClock.elapsedRealtime()+25000
            while(prefs.getString("requested",null)!=prefs.getString("applied",null) && SystemClock.elapsedRealtime()<deadline) SystemClock.sleep(40)
            assertEquals(prefs.getString("error",null),prefs.getString("requested",null),prefs.getString("applied",null))
        }
        fun editor()=By.pkg(host).clazz("android.widget.EditText")
        fun start() {
            dev.executeShellCommand("am start -W -f 0x10008000 -n $host/.EditorActivity")
            focusExternalEditor(dev)
            requireNotNull(findFresh(dev,By.pkg("org.mywubi").desc("j").enabled(true),10000)) { "New host missing j" }
            if(dev.hasObject(By.pkg("org.mywubi").desc("英 → 中"))) dev.findObject(By.pkg("org.mywubi").desc("英 → 中")).click()
        }
        fun key(name:String) {
            requireNotNull(findFresh(dev,By.pkg("org.mywubi").desc(name).enabled(true),5000)) { "Missing $name" }.click()
        }
        fun type(code:String) {
            var typed=""
            for(c in code) {
                key(c.toString())
                typed+=c
                assertTrue("Missing composing prefix $typed, host="+dev.findObject(editor())?.text,
                    dev.wait(Until.hasObject(editor().text(typed)),3000))
            }
        }
        fun rule(code:String,position:Int)=JSONObject().put("enabled",true).put("code",code).put("kind","date_time")
            .put("position",position).put("format","yyyy-MM-dd")
        val date=By.pkg("org.mywubi").desc(Pattern.compile("候选 [0-9]{4}-[0-9]{2}-[0-9]{2}，日期时间.*"))
        var failures=0
        fun row(name:String, action:()->Unit) {
            val r=JSONObject().put("case",name)
            try { action();r.put("pass",true) }
            catch(e:Exception){failures++;r.put("error",e.toString())}
            catch(e:AssertionError){failures++;r.put("error",e.toString())}
            if (r.has("error")) {
                ins.uiAutomation.clearCache()
                r.put("host",dev.findObject(editor())?.text.orEmpty())
                r.put("ime",JSONArray(dev.findObjects(By.pkg("org.mywubi")).mapNotNull {
                    it.contentDescription?.takeIf(String::isNotBlank) ?: it.text?.takeIf(String::isNotBlank)
                }.take(100)))
            }
            report.appendText(r.toString()+"\n")
        }
        try {
            report.writeText("")
            dev.executeShellCommand("ime enable org.mywubi/.MyWubiService")
            dev.executeShellCommand("ime set org.mywubi/.MyWubiService")
            start() // Bind the service before waiting for the first config reload.
            for(capacity in listOf(1,9,10)) for(position in listOf(9,10,11)) row("C06-$capacity-$position") {
                cfg.getJSONObject("basic").put("candidate_count",capacity).put("auto_commit_unique",false)
                cfg.put("shortcuts",JSONObject().put("rules",JSONArray().put(rule("jjad",position))))
                save(cfg)
                start()
                type("jjad")
                key("展开")
                val observed=linkedSetOf<String>()
                var selected=false
                for(attempt in 0..8) {
                    dev.waitForIdle();ins.uiAutomation.clearCache()
                    val nodes=viewport(dev,app.packageName)
                    for((desc, rect) in nodes) {
                        observed.add(desc.removePrefix("候选 ").substringBefore("，"))
                        if(desc.contains("日期时间")) {
                            assertEquals("Visible candidate order $observed",position,observed.size)
                            val expected=observed.last()
                            dev.click(rect.centerX(),rect.centerY())
                            assertTrue("Host must commit $expected",dev.wait(Until.hasObject(editor().text(expected)),5000))
                            selected=true
                            break
                        }
                    }
                    if(selected)break
                    val bounds=nodes.map { it.second }
                    if(bounds.isEmpty())break
                    val left=bounds.minOf { it.left };val right=bounds.maxOf { it.right }
                    val top=bounds.minOf { it.top };val bottom=bounds.maxOf { it.bottom }
                    dev.swipe((left+right)/2,bottom-20,(left+right)/2,(top+bottom)/2,20)
                }
                assertTrue("Missing date in $observed",selected)
            }
            row("C11") {
                start()
                cfg.getJSONObject("basic").put("candidate_count",5).put("auto_commit_unique",true)
                cfg.put("shortcuts",JSONObject().put("rules",JSONArray().put(rule("jjaad",2)).put(rule("jjad",2)).put(rule("jjadx",2))))
                save(cfg)
                type("jjaa")
                assertTrue("Unique word prematurely committed",dev.wait(Until.hasObject(editor().text("jjaa")),3000))
                key("d")
                val dynamic=requireNotNull(findFresh(dev,date,4000))
                val expected=dynamic.contentDescription.removePrefix("候选 ").substringBefore("，")
                dynamic.click()
                assertTrue("Extended shortcut commit",dev.wait(Until.hasObject(editor().text(expected)),5000))
            }
            assertEquals("See special-positions.jsonl",0,failures)
        } finally {
            save(original.getJSONObject("config"), waitForReload = false)
            dev.executeShellCommand("am force-stop $host")
            if(previous.isNotEmpty())dev.executeShellCommand("ime set $previous")
            table.delete()
        }
    }
}
