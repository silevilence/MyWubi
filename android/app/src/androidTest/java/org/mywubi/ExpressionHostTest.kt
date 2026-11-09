package org.mywubi

import android.os.SystemClock
import android.view.KeyEvent
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.util.regex.Pattern

/** Real external EditText + real IME. No mock InputConnection or injected core state. */
class ExpressionHostTest {
    private val instrumentation=InstrumentationRegistry.getInstrumentation()
    private val context=instrumentation.targetContext
    private val device=UiDevice.getInstance(instrumentation)
    private val evidence=File(context.getExternalFilesDir(null),"expression-host").apply { mkdirs() }
    private fun fresh(selector:BySelector):UiObject2 {
        val end=SystemClock.uptimeMillis()+10000
        do {
            if(android.os.Build.VERSION.SDK_INT >= 34) instrumentation.uiAutomation.clearCache()
            else {
                // Before API 34, reapplying the same service info clears the
                // framework cache without changing accessibility capabilities.
                val automation=instrumentation.uiAutomation
                automation.serviceInfo=automation.serviceInfo
            }
            device.findObject(selector)?.let {return it}
            SystemClock.sleep(80)
        } while(SystemClock.uptimeMillis()<end)
        device.takeScreenshot(File(evidence,"failure.png"));device.dumpWindowHierarchy(File(evidence,"failure.xml"))
        error("Missing $selector")
    }
    private fun key(label:String) { fresh(By.desc(label).pkg("org.mywubi").enabled(true)).click(); device.waitForIdle() }
    private fun button(label:String) { fresh(By.text(label).pkg("org.mywubi").enabled(true)).click(); device.waitForIdle() }
    private fun candidate(text:String) { fresh(By.desc(Pattern.compile("候选 ${Pattern.quote(text)}(?:，.*)?")).pkg("org.mywubi")).click();device.waitForIdle() }
    private fun host()=fresh(By.clazz("android.widget.EditText").pkg("org.mywubi.acceptancehost"))
    private fun journal()=device.executeShellCommand("cat /sdcard/Android/data/org.mywubi.acceptancehost/files/edits.jsonl")
    private fun shot(name:String) { device.takeScreenshot(File(evidence,"$name.png"));device.dumpWindowHierarchy(File(evidence,"$name.xml")) }
    private fun assertHost(expected:String) {
        val deadline=SystemClock.uptimeMillis()+5000
        while(host().text.orEmpty()!=expected && SystemClock.uptimeMillis()<deadline) SystemClock.sleep(50)
        assertEquals(expected,host().text.orEmpty())
    }

    @Test fun realQueryEditsNeverTouchHostAndFinalEmojiAndLongKaomojiStayComplete() {
        val store=EngineStore(context)
        val loaded=store.readConfig()
        val original=JSONObject(loaded.getJSONObject("config").toString())
        val config=JSONObject(original.toString())
        val table=File(context.filesDir,"em-host-main.dict").apply {
            writeText("---\ndisplay_name: 表情验收\n---\na\t禁止\nb\t自行车\nc\t车\nd\t开心\nx\t龘靐齉\nf\t家庭\ng\t工\n")
        }
        val long="(๑•̀ㅂ•́)و✧　".repeat(14)+"(╯°□°）╯︵ ┻━┻"
        val kaomoji=File(context.filesDir,"em-host-kaomoji.mwe").apply {
            writeText("# MyWubi Expressions 1\n"+JSONObject().put("kind","kaomoji").put("name","验收样本").put("version","1")
                .put("sources",JSONArray().put(JSONObject().put("id","fixture").put("url","local:test").put("revision","1").put("license","MIT").put("notice","MyWubi test fixture")))
                .put("entries",JSONArray().put(JSONObject().put("text",long).put("name","开心长颜文字").put("triggers",JSONArray().put("开心"))
                    .put("order",0).put("sources",JSONArray().put("fixture")))).toString())
        }
        config.getJSONObject("dictionary").put("system_table",table.path).put("enable_user_dict",false)
        config.getJSONObject("basic").put("auto_commit_unique",false)
        config.put("prediction",JSONObject().put("enabled",false))
        config.put("expressions",JSONObject().put("emoji",JSONObject().put("append",false).put("search",true))
            .put("kaomoji",JSONObject().put("append",false).put("search",true).put("resource",kaomoji.path)))
        fun save(value:JSONObject,version:Long) { NativeHandle(version).use { NativeHandle(store.saveConfig(value,it.value).getLong("version")).close() } }
        fun enter(mode:String) { key("键盘工具");button(if(mode=="Emoji") "Emoji 输入" else "颜文字输入") }
        try {
            save(config,loaded.getLong("version"))
            device.executeShellCommand("ime enable org.mywubi/.MyWubiService")
            device.executeShellCommand("ime set org.mywubi/.MyWubiService")
            device.executeShellCommand("am force-stop org.mywubi.acceptancehost")
            device.executeShellCommand("am start -n org.mywubi.acceptancehost/.EditorActivity")
            host().click();fresh(By.desc("a").pkg("org.mywubi").enabled(true))
            if(device.hasObject(By.desc("英 → 中"))) key("英 → 中")
            for(second in listOf("b","c")) {
                host().text="";host().click()
                val before=journal()
                enter("Emoji");key("a");key("空格");key(second)
                assertEquals("",host().text.orEmpty());assertEquals(before,journal())
                button("查找表情")
                fresh(By.descStartsWith("候选 🚳").pkg("org.mywubi"))
                assertEquals(before,journal());shot("emoji-$second-results")
                button("修改关键词");assertEquals(before,journal())
                button("查找表情");candidate("🚳")
                assertEquals("🚳",host().text.orEmpty())
                File(evidence,"emoji-$second-journal.jsonl").writeText(journal())
                shot("emoji-$second-committed")
                device.pressKeyCode(KeyEvent.KEYCODE_Z,KeyEvent.META_CTRL_ON)
                assertEquals("",host().text.orEmpty())
            }
            val before=journal()
            enter("Emoji");key("x");button("查找表情")
            fresh(By.text("没有匹配结果，可返回修改关键词。").pkg("org.mywubi"))
            assertFalse(device.hasObject(By.descStartsWith("候选 ").pkg("org.mywubi")))
            shot("no-result")
            assertEquals(before,journal());button("修改关键词");key("退格");button("取消")
            assertEquals(before,journal())
            // Losing focus must discard the private query, never flush it to the editor.
            enter("Emoji");key("a");key("空格")
            device.pressHome()
            assertTrue(device.wait(Until.gone(By.desc("q").pkg("org.mywubi")),5000))
            device.executeShellCommand("am start --activity-single-top -n org.mywubi.acceptancehost/.EditorActivity")
            host().click();fresh(By.desc("a").pkg("org.mywubi").enabled(true))
            assertTrue(device.wait(Until.gone(By.text("Emoji · 组合关键词").pkg("org.mywubi")),5000))
            assertEquals(before,journal());assertHost("")
            // A real config update cancels a displayed result; no old output may be committed.
            enter("Emoji");key("a");key("空格");key("c");button("查找表情")
            fresh(By.descStartsWith("候选 🚳").pkg("org.mywubi"))
            config.getJSONObject("appearance").put("font_size",15)
            save(config,store.readConfig().getLong("version"))
            assertTrue(device.wait(Until.gone(By.descStartsWith("候选 🚳").pkg("org.mywubi")),10000))
            assertEquals(before,journal());assertHost("")
            // Max-length automatic selection accumulates local words just like Space.
            val automaticTable=File(context.filesDir,"em-host-automatic.dict").apply {
                writeText(table.readText().replace("display_name: 表情验收", "display_name: 表情验收\nmax_code_len: 1"))
            }
            config.getJSONObject("dictionary").put("system_table",automaticTable.path)
            config.getJSONObject("basic").put("auto_commit_unique",true)
            save(config,store.readConfig().getLong("version"))
            device.waitForIdle();SystemClock.sleep(500)
            enter("Emoji");key("a");key("c");button("查找表情")
            fresh(By.descStartsWith("候选 🚳").pkg("org.mywubi"))
            assertEquals(before,journal());shot("automatic-local-results");button("取消")
            assertEquals(before,journal())
            enter("颜文字");key("d");button("查找表情");key("展开")
            val longCandidate=fresh(By.descStartsWith("候选 $long").pkg("org.mywubi"))
            val bounds=longCandidate.visibleBounds
            device.swipe(bounds.right-12,bounds.centerY(),bounds.left+12,bounds.centerY(),100)
            device.waitForIdle();SystemClock.sleep(500) // let scroll/fling finish before a separate tap
            assertEquals(before,journal());shot("long-kaomoji-scrolled")
            candidate(long);shot("long-kaomoji-after-tap");assertHost(long)
            File(evidence,"kaomoji-journal.jsonl").writeText(journal())
            shot("kaomoji-committed")
        } finally {
            val current=store.readConfig();save(original,current.getLong("version"))
            device.pressBack();device.pressBack()
        }
    }
}
