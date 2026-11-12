package org.mywubi

import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.util.regex.Pattern

/** Synthetic model + actual external Android editor; preserves pre-existing data. */
class PersonalHostTest {
    @Test fun personalCandidateCommitsAndCollectionCanBeStoppedIndependently() {
        val ins=InstrumentationRegistry.getInstrumentation();val app=ins.targetContext;val device=UiDevice.getInstance(ins)
        val store=EngineStore(app);val original=store.readConfig();val initial=JSONObject(original.getJSONObject("config").toString())
        val data=File(store.configFile.parentFile,"personal");val backup=File(store.configFile.parentFile,"personal-backup-${System.nanoTime()}")
        NativeBridge.personalCapture("{\"op\":\"Close\"}".toByteArray())
        if(data.exists())check(data.renameTo(backup))
        fun save(config:JSONObject,version:Long){NativeHandle(version).use{NativeHandle(store.saveConfig(config,it.value).getLong("version")).close()}}
        fun command(capture:Boolean,op:String,vararg fields:Pair<String,Any?>):JSONObject{
            val request=JSONObject().put("op",op).put("config",store.configFile.absolutePath);fields.forEach{(k,v)->request.put(k,v ?: JSONObject.NULL)}
            val bytes=if(capture)NativeBridge.personalCapture(request.toString().toByteArray())else NativeBridge.personalManage(request.toString().toByteArray())
            val reply=JSONObject(bytes.toString(Charsets.UTF_8));assertTrue(reply.toString(),reply.getBoolean("ok"));return reply.optJSONObject("value") ?: JSONObject()
        }
        fun fresh(selector:BySelector):UiObject2{
            val deadline=android.os.SystemClock.uptimeMillis()+10000
            do{ins.uiAutomation.clearCache();device.findObject(selector)?.let{return it};android.os.SystemClock.sleep(80)}while(android.os.SystemClock.uptimeMillis()<deadline)
            device.takeScreenshot(File(app.filesDir,"personal-host-failure.png"));device.dumpWindowHierarchy(File(app.filesDir,"personal-host-failure.xml"));error("missing $selector")
        }
        fun key(label:String){fresh(By.desc(label).pkg("org.mywubi").clickable(true)).click()}
        fun expectText(text:String){fresh(By.clazz("android.widget.EditText").pkg("org.mywubi.acceptancehost").text(text))}
        fun waitApplied(){
            val prefs=EngineUpdates.preferences(app)
            val deadline=android.os.SystemClock.uptimeMillis()+10000
            while(prefs.getString("requested",null)!=prefs.getString("applied",null) && android.os.SystemClock.uptimeMillis()<deadline)android.os.SystemClock.sleep(50)
            assertEquals(prefs.getString("error",null),prefs.getString("requested",null),prefs.getString("applied",null))
        }
        val config=JSONObject(initial.toString())
        val table=File(app.cacheDir,"personal-host.dict").apply{writeText("---\nmax_code_len: 4\n---\nw\t明天去\ng\t图书馆\n")}
        config.getJSONObject("dictionary").put("system_table",table.absolutePath).put("enable_user_dict",false)
        config.getJSONObject("basic").put("auto_commit_unique",false)
        config.put("personal",JSONObject().put("collect",true).put("learn",true).put("predict",false))
        config.put("prediction",JSONObject().put("enabled",false).put("wait_ms",1000))
        config.put("expressions",JSONObject())
        try {
            save(config,original.getLong("version"))
            command(true,"Configure","settings" to config.getJSONObject("personal"),"allowed" to true,"session" to "synthetic-seed")
            command(true,"Context","context" to JSONObject("""{"revision":1,"left":"明天去","right":"","selected":"","selection":null,"truncated":false,"discontinuity":false}"""),"external" to false)
            repeat(40){command(true,"Operation","operation" to JSONObject("""{"CommitAndReplace":{"text":"图书馆","replace_len":0}}"""),"outcome" to "Confirmed","after" to null,"method" to "fixture")}
            val end=android.os.SystemClock.uptimeMillis()+10000
            while(command(false,"Status").getJSONObject("data").getInt("records")<42 && android.os.SystemClock.uptimeMillis()<end)android.os.SystemClock.sleep(50)
            command(false,"Train");command(true,"Close")
            config.getJSONObject("personal").put("predict",true).put("learn",false)
            save(config,store.readConfig().getLong("version"))
            device.executeShellCommand("ime enable org.mywubi/.MyWubiService");device.executeShellCommand("ime set org.mywubi/.MyWubiService")
            device.executeShellCommand("am force-stop org.mywubi.acceptancehost")
            device.executeShellCommand("am start -n org.mywubi.acceptancehost/.EditorActivity")
            fresh(By.clazz("android.widget.EditText").pkg("org.mywubi.acceptancehost")).click()
            waitApplied()
            if(device.hasObject(By.desc("英 → 中")))key("英 → 中")
            key("w");key("空格")
            fresh(By.desc(Pattern.compile("候选 图书馆(?:，.*)?")).pkg("org.mywubi")).click()
            expectText("明天去图书馆")
            device.takeScreenshot(File(app.filesDir,"personal-host-commit.png"))
            config.getJSONObject("personal").put("collect",false)
            save(config,store.readConfig().getLong("version"))
            waitApplied()
            val records=command(false,"Status").getJSONObject("data").getInt("records")
            fresh(By.clazz("android.widget.EditText").pkg("org.mywubi.acceptancehost")).text=""
            key("w");key("空格")
            fresh(By.desc(Pattern.compile("候选 图书馆(?:，.*)?")).pkg("org.mywubi")).click()
            expectText("明天去图书馆")
            assertEquals(records,command(false,"Status").getJSONObject("data").getInt("records"))
        } finally {
            command(true,"Close")
            save(initial,store.readConfig().getLong("version"))
            data.deleteRecursively();if(backup.exists())check(backup.renameTo(data));table.delete()
        }
    }
}
