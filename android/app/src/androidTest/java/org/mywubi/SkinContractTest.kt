package org.mywubi

import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Density
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File

class SkinContractTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    @get:org.junit.Rule val evidence = object : org.junit.rules.TestWatcher() {
        override fun failed(error:Throwable,description:org.junit.runner.Description) {
            val device=UiDevice.getInstance(instrumentation)
            device.takeScreenshot(File(instrumentation.targetContext.filesDir,"skin-${description.methodName}-failure.png"))
            device.dumpWindowHierarchy(File(instrumentation.targetContext.filesDir,"skin-${description.methodName}-failure.xml"))
        }
    }
    @Test fun actualWindowsEditorExportResolvesAndRoundtripsWithoutChangingWindows() {
        val (path,config)=fixture()
        val source=File(path.parentFile,"editor-source.mywubi-skin")
        instrumentation.context.assets.open("skins/editor-jade.mywubi-skin").use { input -> source.outputStream().use {input.copyTo(it)} }
        val request=JSONObject().put("action","Import").put("path",path.path).put("source",source.path).put("platform","android")
        val item=NativeBridge.objectCall("ManageSkin","request" to request).getJSONObject("item")
        val selection=JSONObject().put("id",item.getString("id")).put("version",item.getString("version")).put("component","jade")
        config.getJSONObject("skins").getJSONObject("android").put("selection",selection)
        val appearance=loadSkinAppearance(path,config)
        assertFalse(appearance.getJSONObject("light").getBoolean("legacy"))
        assertNotEquals(appearance.getJSONObject("light").getJSONObject("parameters").getString("background"),
            appearance.getJSONObject("dark").getJSONObject("parameters").getString("background"))
        val target=File(path.parentFile,"roundtrip-${java.util.UUID.randomUUID()}.mywubi-skin")
        NativeBridge.objectCall("ManageSkin","request" to JSONObject().put("action","Export").put("path",path.path).put("selection",selection).put("target",target.path))
        java.util.zip.ZipFile(source).use { before -> java.util.zip.ZipFile(target).use { after ->
            val entries=before.entries().toList()
            assertEquals(entries.map {it.name}.toSet(),after.entries().toList().map {it.name}.toSet())
            for(entry in entries) assertArrayEquals(entry.name,before.getInputStream(entry).readBytes(),after.getInputStream(after.getEntry(entry.name)).readBytes())
        } }
    }

    private fun selectImageFixture(path:File, config:JSONObject) {
        val source=File(path.parentFile,"image-fixture.mywubi-skin")
        instrumentation.context.assets.open("skins/image-fixture.mywubi-skin").use {input->source.outputStream().use {input.copyTo(it)}}
        NativeBridge.objectCall("ManageSkin","request" to JSONObject().put("action","Import").put("path",path.path).put("source",source.path).put("platform","android"))
        config.getJSONObject("skins").getJSONObject("android").put("selection",JSONObject().put("id","mywubi.image.fixture").put("version","1").put("component","jade"))
    }
    @Test fun imageVariantsShareDecodedResourcesAndRejectCorruption() {
        val (path,config)=fixture();selectImageFixture(path,config)
        val skin=loadSkinAppearance(path,config)
        val light=skin.getJSONObject("light").get("decodedImages") as SkinImages
        val dark=skin.getJSONObject("dark").get("decodedImages") as SkinImages
        assertEquals(8,light.roles.size)
        assertSame(light.roles["key"]!!.bitmap,dark.roles["bubble"]!!.bitmap)
        assertSame(light.roles["background"]!!.bitmap,light.roles["pressed"]!!.bitmap)
        assertEquals(16,light.roles["background"]!!.bitmap.width)
        val variant=NativeBridge.objectCall("ResolveSkin","path" to path.path,"config" to config,"platform" to "android","dark" to false,"high_contrast" to false)
        for(role in variant.getJSONObject("images").keys()) variant.getJSONObject("images").getJSONObject(role).put("png","AA==")
        assertTrue(runCatching {decodeSkinImages(variant,mutableMapOf())}.isFailure)
    }

    @Test fun realImeKeepsPendingCandidateAcrossAppearanceSave() {
        val device=UiDevice.getInstance(instrumentation)
        val context=instrumentation.targetContext
        val store=EngineStore(context)
        val loaded=store.readConfig()
        val original=JSONObject(loaded.getJSONObject("config").toString())
        val config=JSONObject(original.toString())
        val table=File(context.filesDir,"skin-host.dict").apply {writeText("a\t工\t100\na\t式\t80\n")}
        config.getJSONObject("dictionary").put("system_table",table.path).put("enable_user_dict",false)
        config.getJSONObject("basic").put("auto_commit_unique",false)
        config.put("prediction",JSONObject().put("enabled",false))
        config.put("expressions",JSONObject())
        config.getJSONObject("skins").getJSONObject("android").put("preset","jade").remove("selection")
        fun save(value:JSONObject,version:Long) {NativeHandle(version).use { NativeHandle(store.saveConfig(value,it.value).getLong("version")).close() }}
        fun key(label:String):androidx.test.uiautomator.UiObject2 {
            instrumentation.uiAutomation.clearCache()
            return device.wait(Until.findObject(By.desc(label).pkg("org.mywubi").enabled(true)),10000) ?: error("Missing $label")
        }
        fun awaitApplied() {
            val preferences=EngineUpdates.preferences(context)
            val requested=preferences.getString("requested",null)
            val end=android.os.SystemClock.uptimeMillis()+15000
            while(preferences.getString("applied",null)!=requested && android.os.SystemClock.uptimeMillis()<end) android.os.SystemClock.sleep(50)
            assertEquals("Reload failed: ${preferences.getString("error",null)}",requested,preferences.getString("applied",null))
        }
        try {
            save(config,loaded.getLong("version"))
            device.executeShellCommand("ime enable org.mywubi/.MyWubiService")
            device.executeShellCommand("ime set org.mywubi/.MyWubiService")
            device.executeShellCommand("am force-stop org.mywubi.acceptancehost")
            device.executeShellCommand("am start -W -n org.mywubi.acceptancehost/.EditorActivity")
            val host=device.wait(Until.findObject(By.clazz("android.widget.EditText").pkg("org.mywubi.acceptancehost")),10000) ?: error("Host not installed")
            host.click(); awaitApplied(); key("a")
            if(device.hasObject(By.desc("英 → 中"))) {key("英 → 中").click();device.waitForIdle()}
            key("a").click();device.waitForIdle();instrumentation.uiAutomation.clearCache()
            assertNotNull(device.wait(Until.findObject(By.descStartsWith("候选 工").pkg("org.mywubi")),10000))
            config.getJSONObject("skins").getJSONObject("android").put("preset","plain")
            val current=store.readConfig();save(config,current.getLong("version"))
            awaitApplied()
            key("空格").click();device.waitForIdle();instrumentation.uiAutomation.clearCache()
            assertEquals("工",device.findObject(By.clazz("android.widget.EditText").pkg("org.mywubi.acceptancehost")).text)
            device.takeScreenshot(File(context.filesDir,"skin-real-host.png"))
        } finally {val current=store.readConfig();save(original,current.getLong("version"));EngineUpdates.request(context)}
    }
    private fun fixture(): Pair<File, JSONObject> {
        val root = File(instrumentation.targetContext.cacheDir,"skin-contract").apply { mkdirs() }
        File(root,"main.dict").writeText("a\t工\t100\na\t式\t80\n")
        val path = File(root,"config.toml").apply { writeText("[basic]\nauto_commit_unique=false\n[dictionary]\nsystem_table=\"main.dict\"\nuser_table=\"user.dict\"\n") }
        val read = NativeBridge.objectCall("ReadConfig","path" to path.path)
        NativeHandle(read.getLong("version")).close()
        return path to read.getJSONObject("config")
    }

    @Test fun resolvingBothVariantsPreservesCandidateIdentityAndPendingCommit() {
        val (path,config) = fixture()
        NativeHandle(NativeBridge.objectCall("BuildRuntime","path" to path.path).getLong("handle")).use { runtime ->
            val context = JSONObject("""{"editor":1,"epoch":1,"supports_preedit":true,"supports_insert":true,"owns_preedit":false,"purpose":"Text","selection_available":true,"surrounding_available":true,"enter_action":"Newline"}""")
            NativeHandle(NativeBridge.objectCall("CreateSession","runtime" to runtime.value,"context" to context).getLong("handle")).use { session ->
                val prepared = NativeBridge.objectCall("Prepare","session" to session.value,"action" to JSONObject().put("InputText",JSONArray().put("a").put("Normal")))
                NativeBridge.call("Ack","session" to session.value,"token" to prepared.getJSONObject("token"),"outcome" to "Applied","owns_preedit" to true)
                val before = NativeBridge.objectCall("Candidates","session" to session.value,"offset" to 0,"count" to 9).toString()
                for (preset in listOf("jade","plain","contrast","legacy")) {
                    config.getJSONObject("skins").getJSONObject("android").remove("selection")
                if(preset=="image") selectImageFixture(path,config) else config.getJSONObject("skins").getJSONObject("android").put("preset",preset)
                    loadSkinAppearance(path,config)
                    assertEquals(before,NativeBridge.objectCall("Candidates","session" to session.value,"offset" to 0,"count" to 9).toString())
                }
                val pending = NativeBridge.objectCall("Prepare","session" to session.value,"action" to "Space")
                loadSkinAppearance(path,config)
                assertEquals("工",pending.getJSONObject("op").getJSONObject("CommitAndReplace").getString("text"))
                assertEquals("Published",NativeBridge.objectCall("Ack","session" to session.value,"token" to pending.getJSONObject("token"),"outcome" to "Applied","owns_preedit" to false).getString("status"))
            }
        }
    }

    @Test fun nativeKeyboardKeepsCandidatesAndTouchTargetsAcrossSkinAndLargeFontChanges() {
        val (path, config) = fixture()
        val device = UiDevice.getInstance(instrumentation)
        device.executeShellCommand("am force-stop org.mywubi.acceptancehost")
        device.pressHome();device.waitForIdle()
        var skin by mutableStateOf<JSONObject?>(null)
        var scale by mutableFloatStateOf(1f)
        val events = java.util.concurrent.CopyOnWriteArrayList<Any>()
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { activity -> activity.setContent {
                val density=LocalDensity.current
                CompositionLocalProvider(LocalDensity provides Density(density.density,scale)) {
                    Box(Modifier.fillMaxSize(),contentAlignment=Alignment.BottomCenter) {
                        Keyboard(KeyboardModel(skin=skin,enabled=true,modesAvailable=true,spelling="a",
                            candidates=listOf(CandidateItem(1,"工","aaaa"),CandidateItem(2,"👩🏽‍💻","emoji"))),
                            {events.add(it)}, {}, switchIme={})
                    }
                }
            } }
            for (preset in listOf("jade","plain","contrast","image")) for (fontScale in listOf(1f,2f)) {
                config.getJSONObject("skins").getJSONObject("android").remove("selection")
                if(preset=="image") selectImageFixture(path,config) else config.getJSONObject("skins").getJSONObject("android").put("preset",preset)
                val resolved=loadSkinAppearance(path,config)
                scenario.onActivity { skin=resolved; scale=fontScale }
                device.waitForIdle(); instrumentation.waitForIdleSync(); instrumentation.uiAutomation.clearCache()
                val key=device.wait(Until.findObject(By.desc("q").pkg("org.mywubi")),5000) ?: error("q missing")
                val candidate=device.wait(Until.findObject(By.desc("候选 工，编码提示 aaaa").pkg("org.mywubi")),5000) ?: error("candidate missing")
                assertTrue(candidate.visibleBounds.bottom <= key.visibleBounds.top)
                assertTrue(key.visibleBounds.height() >= (48 * instrumentation.targetContext.resources.displayMetrics.density).toInt())
                val count=events.size
                key.click(); device.waitForIdle()
                val deadline=android.os.SystemClock.uptimeMillis()+5000
                while(events.size==count && android.os.SystemClock.uptimeMillis()<deadline) android.os.SystemClock.sleep(50)
                assertEquals("One release should dispatch one action",count+1,events.size)
                assertTrue(events.last().toString().contains("q"))
                device.takeScreenshot(File(instrumentation.targetContext.filesDir,"skin-$preset-$fontScale.png"))
            }
        }
    }
}
