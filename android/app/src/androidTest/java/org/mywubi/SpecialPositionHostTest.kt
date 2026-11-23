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
                    val nodes=dev.findObjects(By.pkg("org.mywubi").descStartsWith("候选 "))
                        .filter { it.visibleBounds.height()>0 }.sortedWith(compareBy({it.visibleBounds.top},{it.visibleBounds.left}))
                    for(n in nodes) {
                        val desc=n.contentDescription
                        observed.add(desc.removePrefix("候选 ").substringBefore("，"))
                        if(desc.contains("日期时间")) {
                            assertEquals("Visible candidate order $observed",position,observed.size)
                            val expected=observed.last()
                            n.click()
                            assertTrue("Host must commit $expected",dev.wait(Until.hasObject(editor().text(expected)),5000))
                            selected=true
                            break
                        }
                    }
                    if(selected)break
                    val bounds=nodes.map { it.visibleBounds }
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
