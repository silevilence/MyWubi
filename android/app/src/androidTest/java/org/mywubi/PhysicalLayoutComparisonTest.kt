package org.mywubi

import android.os.SystemClock
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.util.regex.Pattern

/** AK-09 fixed centre/offset traces on real Wubi86 and a separate disposable user dictionary. */
class PhysicalLayoutComparisonTest {
    @Test fun identicalWordsOnThreeLayouts() {
        org.junit.Assume.assumeTrue(InstrumentationRegistry.getArguments().getString("physical_acceptance")=="true")
        val ins=InstrumentationRegistry.getInstrumentation()
        val app=ins.targetContext
        val dev=UiDevice.getInstance(ins)
        val store=EngineStore(app)
        val configPath=store.configFile
        val initial=store.readConfig()
        NativeHandle(initial.getLong("version")).close()
        val config=JSONObject(initial.getJSONObject("config").toString())
        val receipt=File(configPath.parentFile,"keyboard-selection.json")
        val prior=receipt.takeIf {it.isFile}?.readBytes()
        val user=File.createTempFile("compare-user-",".dict",app.cacheDir).apply {writeText("mmaa\t联调用词\t999999999\n")}
        val host="org.mywubi.acceptancehost"
        val previous=dev.executeShellCommand("settings get secure default_input_method").trim()
        val report=File(app.filesDir,"layout-comparison.jsonl")
        fun save(c:JSONObject) {NativeHandle(store.readConfig().getLong("version")).use {NativeHandle(store.saveConfig(c,it.value).getLong("version")).close()}}
        fun field()=By.pkg(host).clazz("android.widget.EditText")
        fun key(label:String)=requireNotNull(findFresh(dev,By.pkg("org.mywubi").desc(label).enabled(true),5000)) { "Missing key $label" }
        val guides=mapOf("shape14" to "QW ER TY UI OP AS DF GH JK L ZX CV BN M","shape9" to "ABC DEF GHI JKL MNO PQR STU VWX YZ")
        var failed=0
        try {
            report.writeText("")
            config.getJSONObject("dictionary").put("system_table","tables/wubi86.dict").put("enable_user_dict",true).put("user_table",user.path)
            config.getJSONObject("basic").put("auto_commit_unique",false)
            config.put("prediction",JSONObject()).put("expressions",JSONObject()).put("shortcuts",JSONObject())
            NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(configPath,"InstallTemplates"))
            val appear=config.getJSONObject("skins").getJSONObject("android")
            appear.put("selection",JSONObject().put("id","mywubi.keyboards").put("version","4").put("component","jade"))
            save(config)
            dev.executeShellCommand("ime enable org.mywubi/.MyWubiService")
            dev.executeShellCommand("ime set org.mywubi/.MyWubiService")
            for(layout in listOf("full26","shape14","shape9")) {
                val select=JSONObject().put("id","mywubi.keyboards").put("version","4").put("component",layout)
                val choice=JSONObject().put("layout",select).put("mapping",JSONObject.NULL).put("appearance",appear)
                fun req(action:String)=keyboardRequest(configPath,action).put("choice",choice).put("mode","Main").put("charset",keyboardCharset(configPath,config,"Main"))
                val prep=NativeBridge.objectCall("ManageKeyboard","request" to req("Prepare"))
                NativeBridge.objectCall("ManageKeyboard","request" to req("Save").put("digest",prep.getString("digest")))
                EngineUpdates.request(app)
                for((code,word) in listOf("wqvb" to "你好","uegd" to "龘","aaaa" to "恭恭敬敬","kwwl" to "中华人民共和国","mmaa" to "联调用词")) {
                    val row=JSONObject().put("layout",layout).put("code",code).put("expected",word)
                    try {
                        dev.executeShellCommand("am force-stop $host")
                        dev.executeShellCommand("am start -W -n $host/.EditorActivity")
                        focusExternalEditor(dev)
                        if(dev.hasObject(By.pkg("org.mywubi").desc("英 → 中")))key("英 → 中").click()
                        val updates = EngineUpdates.preferences(app)
                        val readyBy = SystemClock.elapsedRealtime() + 15000
                        while (updates.getString("requested",null) != updates.getString("applied",null) &&
                            SystemClock.elapsedRealtime() < readyBy) SystemClock.sleep(80)
                        assertEquals("Layout reload",updates.getString("requested",null),updates.getString("applied",null))
                        dev.waitForIdle()
                        ins.uiAutomation.clearCache()
                        val start=SystemClock.uptimeMillis()
                        for((index,c) in code.withIndex()) {
                            val k=if(layout=="full26")c.toString() else guides.getValue(layout).split(" ").single { c.uppercaseChar() in it }
                            val bounds=key(k).visibleBounds
                            val x=bounds.centerX() + when(index%3) {1->bounds.width()/4;2->-bounds.width()/4;else->0}
                            assertTrue(dev.click(x,bounds.centerY()))
                            val acknowledged = if (layout == "full26") field().text(code.take(index+1))
                                else By.textStartsWith("${index+1}: ")
                            assertTrue("No composing acknowledgement for $layout / $code after key ${index+1}",
                                findFresh(dev,acknowledged,3000) != null)
                            if(index==0)row.put("first_key_width_px",bounds.width())
                        }
                        val wanted=By.pkg("org.mywubi").desc(Pattern.compile("候选 "+Pattern.quote(word)+"(?:，.*)?"))
                        var item=findFresh(dev,wanted,2000)
                        if(item==null) {
                            key("展开").click()
                            repeat(12) {
                                if(item==null) {
                                    ins.uiAutomation.clearCache()
                                    item=dev.findObject(wanted)
                                    if(item==null)dev.findObject(By.pkg("org.mywubi").scrollable(true))?.scroll(Direction.DOWN,0.7f)
                                }
                            }
                        }
                        requireNotNull(item){"Candidate missing from compact strip and expanded search"}.click()
                        assertTrue("Committed host text",dev.wait(Until.hasObject(field().text(word).focused(true)),5000))
                        row.put("through_commit_ms",SystemClock.uptimeMillis()-start).put("result","pass")
                    } catch(e:Exception){failed++;row.put("error",e.toString())}
                    catch(e:AssertionError){failed++;row.put("error",e.toString())}
                    if (row.has("error")) {
                        ins.uiAutomation.clearCache()
                        row.put("host",dev.findObject(field())?.text.orEmpty())
                        row.put("ime",org.json.JSONArray(dev.findObjects(By.pkg("org.mywubi")).mapNotNull {
                            it.contentDescription?.takeIf(String::isNotBlank) ?: it.text?.takeIf(String::isNotBlank)
                        }.take(100)))
                    }
                    report.appendText(row.toString()+"\n")
                }
            }
            assertEquals("See layout-comparison.jsonl",0,failed)
        } finally {
            if(prior==null)receipt.delete() else receipt.writeBytes(prior)
            save(initial.getJSONObject("config"))
            dev.executeShellCommand("am force-stop $host")
            if(previous.isNotEmpty())dev.executeShellCommand("ime set $previous")
            user.delete()
        }
    }
}
