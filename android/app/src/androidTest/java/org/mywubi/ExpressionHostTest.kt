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

    @Test fun numericChoicesStayLocalAndResourceReloadRejectsAnOldAccessibilityClick() {
        val store = EngineStore(context)
        val loaded = store.readConfig()
        val original = JSONObject(loaded.getJSONObject("config").toString())
        val config = JSONObject(original.toString())
        val table = File(context.filesDir, "em-host-numeric.dict").apply {
            writeText("---\nmax_code_len: 4\n---\na\t禁止\nc\t车\n")
        }
        val alternate = File(context.filesDir, "em-host-alternate.mwe").apply {
            writeText("# MyWubi Expressions 1\n" + JSONObject()
                .put("kind", "emoji").put("name", "重载验收样本").put("version", "1")
                .put("sources", JSONArray().put(JSONObject().put("id", "fixture").put("url", "local:test")
                    .put("revision", "1").put("license", "MIT").put("notice", "MyWubi test fixture")))
                .put("entries", JSONArray().put(JSONObject().put("text", "🙂").put("name", "重载后的表情")
                    .put("triggers", JSONArray().put("禁止车")).put("order", 0)
                    .put("sources", JSONArray().put("fixture")))).toString())
        }
        config.getJSONObject("dictionary").put("system_table", table.path).put("enable_user_dict", false)
        config.getJSONObject("basic").put("auto_commit_unique", false)
        config.put("prediction", JSONObject().put("enabled", false))
        config.put("expressions", JSONObject().put("emoji", JSONObject().put("append", false).put("search", true)))
        fun save(value: JSONObject, version: Long) {
            NativeHandle(version).use { NativeHandle(store.saveConfig(value, it.value).getLong("version")).close() }
        }
        fun enter() { key("键盘工具"); button("Emoji 输入") }
        val automation = instrumentation.uiAutomation
        val originalFlags = automation.serviceInfo.flags
        val retainedNodes = mutableListOf<android.view.accessibility.AccessibilityNodeInfo>()
        fun findOldCandidate(node: android.view.accessibility.AccessibilityNodeInfo): android.view.accessibility.AccessibilityNodeInfo? {
            retainedNodes.add(node)
            if (node.contentDescription?.toString()?.startsWith("候选 🚳") == true) return node
            for (index in 0 until node.childCount) {
                node.getChild(index)?.let { child -> findOldCandidate(child)?.let { return it } }
            }
            return null
        }
        try {
            automation.serviceInfo = automation.serviceInfo.apply {
                flags = flags or android.accessibilityservice.AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS
            }
            save(config, loaded.getLong("version"))
            device.executeShellCommand("ime enable org.mywubi/.MyWubiService")
            device.executeShellCommand("ime set org.mywubi/.MyWubiService")
            device.executeShellCommand("am force-stop org.mywubi.acceptancehost")
            // Wait for launch and UI idle before tapping; merely finding an editor
            // node did not reliably open the keyboard on the first API 26 run.
            device.executeShellCommand("am start -W -n org.mywubi.acceptancehost/.EditorActivity")
            device.waitForIdle()
            host().click(); fresh(By.desc("a").pkg("org.mywubi").enabled(true))
            if (device.hasObject(By.desc("英 → 中"))) key("英 → 中")
            val before = journal()

            enter(); key("a"); key("123"); key("1")
            fresh(By.text("关键词：禁止").pkg("org.mywubi"))
            assertHost(""); assertEquals(before, journal()); shot("numeric-word-local")
            key("ABC"); key("c"); key("空格"); button("查找表情")
            fresh(By.descStartsWith("候选 🚳").pkg("org.mywubi"))
            assertEquals(before, journal())
            key("123"); key("1"); assertHost("🚳")
            File(evidence, "numeric-final-journal.jsonl").writeText(journal())
            shot("numeric-final-committed")
            device.pressKeyCode(KeyEvent.KEYCODE_Z, KeyEvent.META_CTRL_ON); assertHost("")
            key("ABC")

            val beforeReload = journal()
            enter(); key("a"); key("空格"); key("c"); button("查找表情")
            fresh(By.descStartsWith("候选 🚳").pkg("org.mywubi"))
            val oldCandidate = automation.windows.asSequence().mapNotNull { it.root }
                .mapNotNull(::findOldCandidate).firstOrNull() ?: error("No actual accessibility candidate node")
            config.getJSONObject("expressions").getJSONObject("emoji").put("resource", alternate.path)
            save(config, store.readConfig().getLong("version"))
            // Wait for the real service to publish idle controls after cancelling the old query.
            fresh(By.desc("中 → 英").pkg("org.mywubi").enabled(true))
            assertFalse(device.hasObject(By.descStartsWith("候选 🚳").pkg("org.mywubi")))
            // Dispatch the retained virtual node's action, not a coordinate tap on a new key.
            val oldActionAccepted = oldCandidate.performAction(android.view.accessibility.AccessibilityNodeInfo.ACTION_CLICK)
            device.waitForIdle(); assertHost(""); assertEquals(beforeReload, journal())
            shot("resource-reload-old-action")

            enter(); key("a"); key("空格"); key("c"); button("查找表情")
            fresh(By.descStartsWith("候选 🙂").pkg("org.mywubi"))
            assertFalse(device.hasObject(By.descStartsWith("候选 🚳").pkg("org.mywubi")))
            assertEquals(beforeReload, journal()); shot("resource-reload-new-result")
            candidate("🙂"); assertHost("🙂")
            File(evidence, "resource-reload-journal.jsonl").writeText(journal())
            File(evidence, "resource-reload-check.json").writeText(JSONObject()
                .put("old_accessibility_action_returned", oldActionAccepted)
                .put("no_write_after_old_action", true).put("new_output", "🙂")
                .put("scope", "Real InputMethodService, stale accessibility node action, external EditText; no JNI action injection")
                .toString(2))
            shot("resource-reload-committed")
        } finally {
            save(original, store.readConfig().getLong("version"))
            automation.serviceInfo = automation.serviceInfo.apply { flags = originalFlags }
            @Suppress("DEPRECATION")
            retainedNodes.forEach { it.recycle() }
            device.pressBack(); device.pressBack()
        }
    }

    @Test fun fullLibraryTailSelectionAndModeReturnDoNotLeakPartialQueries() {
        val store = EngineStore(context)
        val loaded = store.readConfig()
        val original = JSONObject(loaded.getJSONObject("config").toString())
        val config = JSONObject(original.toString())
        val table = File(context.filesDir, "em-host-tail.dict").apply {
            writeText("---\nmax_code_len: 4\n---\na\t禁止\nc\t车\nd\t开心\nx\t龘靐齉\n")
        }
        config.getJSONObject("dictionary").put("system_table", table.path).put("enable_user_dict", false)
        config.getJSONObject("basic").put("auto_commit_unique", false)
        config.put("prediction", JSONObject().put("enabled", false))
        config.put("expressions", JSONObject()
            .put("emoji", JSONObject().put("append", false).put("search", true))
            .put("kaomoji", JSONObject().put("append", false).put("search", true)))
        fun save(value: JSONObject, version: Long) {
            NativeHandle(version).use { NativeHandle(store.saveConfig(value, it.value).getLong("version")).close() }
        }
        fun enter(label: String) { key("键盘工具"); button(label) }
        try {
            save(config, loaded.getLong("version"))
            // Obtain the full-library ordering from an independent JNI session.
            // The actual selection below still travels through the visible IME and OS editor.
            val expected = NativeHandle(NativeBridge.objectCall("BuildRuntime", "path" to store.configFile.path).getLong("handle")).use { runtime ->
                val editor = JSONObject("""{"editor":99,"epoch":1,"supports_preedit":true,"supports_insert":true,"owns_preedit":false,"purpose":"Text","selection_available":true,"surrounding_available":true,"enter_action":"Send"}""")
                NativeHandle(NativeBridge.objectCall("CreateSession", "runtime" to runtime.value, "context" to editor).getLong("handle")).use { session ->
                    for (action in listOf(JSONObject().put("BeginTemporary", "Kaomoji"), JSONObject().put("InputCode", "d"), "FindExpressions")) {
                        val plan = NativeBridge.objectCall("Prepare", "session" to session.value, "action" to action)
                        assertEquals("NoOp", plan.getString("op"))
                        NativeBridge.objectCall("Ack", "session" to session.value, "token" to plan.getJSONObject("token"),
                            "outcome" to "Applied", "owns_preedit" to false)
                    }
                    val first = NativeBridge.objectCall("Candidates", "session" to session.value, "offset" to 0, "count" to 256)
                    val total = first.getInt("total")
                    assertTrue("Use the full library, beyond the first candidate window", total > 512)
                    val offset = ((total - 1) / 256) * 256
                    val tail = NativeBridge.objectCall("Candidates", "session" to session.value, "offset" to offset, "count" to 256)
                    JSONObject().put("total", total).put("offset", offset)
                        .put("text", tail.getJSONArray("candidates").getJSONObject(0).getString("text"))
                }
            }
            device.executeShellCommand("ime enable org.mywubi/.MyWubiService")
            device.executeShellCommand("ime set org.mywubi/.MyWubiService")
            device.executeShellCommand("am force-stop org.mywubi.acceptancehost")
            device.executeShellCommand("am start -n org.mywubi.acceptancehost/.EditorActivity")
            host().click(); fresh(By.desc("a").pkg("org.mywubi").enabled(true))
            if (device.hasObject(By.desc("中 → 英"))) key("中 → 英")
            val before = journal()

            enter("Emoji 输入"); key("a"); key("空格"); key("x"); button("查找表情")
            fresh(By.text("没有满足全部关键词的结果；以下仅为部分匹配。").pkg("org.mywubi"))
            fresh(By.desc(Pattern.compile("候选 .*部分匹配.*")).pkg("org.mywubi"))
            assertHost(""); assertEquals(before, journal()); shot("partial-query-local")
            button("取消")
            fresh(By.desc("英 → 中").pkg("org.mywubi").enabled(true))
            assertEquals(before, journal())

            // Cancel returns to English; entering the other mode still uses the main table.
            enter("颜文字输入"); key("d"); button("查找表情"); key("展开")
            val total = expected.getInt("total")
            val lastOffset = expected.getInt("offset")
            for (offset in 256..lastOffset step 256) {
                key("下一组")
                fresh(By.text("${offset + 1}–${minOf(offset + 256, total)} / $total").pkg("org.mywubi"))
                assertEquals("Paging must never write the host", before, journal())
            }
            fresh(By.desc("下一组").pkg("org.mywubi").enabled(false))
            val tailText = expected.getString("text")
            fresh(By.desc(Pattern.compile("候选 ${Pattern.quote(tailText)}(?:，.*)?")).pkg("org.mywubi"))
            shot("full-library-final-window")
            candidate(tailText); assertHost(tailText)
            fresh(By.desc("英 → 中").pkg("org.mywubi").enabled(true))
            File(evidence, "full-library-tail.json").writeText(expected.toString(2))
            File(evidence, "full-library-tail-journal.jsonl").writeText(journal())
            shot("full-library-tail-committed")
            device.pressKeyCode(KeyEvent.KEYCODE_Z, KeyEvent.META_CTRL_ON)
            assertHost("")
            // Re-entering Emoji after a tail commit must not retain the previous query/window.
            enter("Emoji 输入"); key("a"); key("空格"); key("c"); button("查找表情")
            candidate("🚳"); assertHost("🚳")
            fresh(By.desc("英 → 中").pkg("org.mywubi").enabled(true))
            shot("mode-return-committed")
        } finally {
            save(original, store.readConfig().getLong("version"))
            device.pressBack(); device.pressBack()
        }
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
