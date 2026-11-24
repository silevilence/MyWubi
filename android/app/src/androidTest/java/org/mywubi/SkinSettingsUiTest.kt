package org.mywubi

import android.content.ContentValues
import android.provider.MediaStore
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.junit.Assert.*
import org.junit.Test
import java.io.File

class SkinSettingsUiTest {
    private val instrumentation=InstrumentationRegistry.getInstrumentation()
    private val context=instrumentation.targetContext
    private val device=UiDevice.getInstance(instrumentation)
    private fun text(label: String, description: Boolean = false): UiObject2 {
        val selector = if (description) By.desc(label) else By.text(label)
        return try {
            findSettingsControl(device, selector)
        } catch (failure: IllegalStateException) {
            device.dumpWindowHierarchy(File(context.filesDir, "skin-control-failure.xml"))
            throw failure
        }
    }
    private fun clickReady(label: String, description: Boolean = false) = clickSetting(device, label, description)

    @get:org.junit.Rule val evidence=object:org.junit.rules.TestWatcher() {
        override fun failed(error:Throwable,description:org.junit.runner.Description) {
            device.takeScreenshot(File(context.filesDir,"skin-settings-failure.png"))
            device.dumpWindowHierarchy(File(context.filesDir,"skin-settings-failure.xml"))
        }
    }

    @Test fun importPreviewSwitchShareAndDeleteFromLibrary() {
        val store=EngineStore(context)
        val original=store.readConfig()
        NativeHandle(original.getLong("version")).close()
        val draft=File(context.filesDir,"settings-draft.json")
        val originalDraft=draft.takeIf {it.exists()}?.readBytes()
        if(draft.exists()) check(draft.delete())
        val name="SK-editor-${System.currentTimeMillis()}.mywubi-skin"
        val uri=context.contentResolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI,ContentValues().apply {
            put(MediaStore.MediaColumns.DISPLAY_NAME,name);put(MediaStore.MediaColumns.MIME_TYPE,"application/zip")
            put(MediaStore.MediaColumns.RELATIVE_PATH,"Download")
        }) ?: error("Cannot prepare SAF fixture")
        val packagePath = InstrumentationRegistry.getArguments().getString("keyboard_package")
        val packageInput = packagePath?.let { File(it).inputStream() }
            ?: instrumentation.context.assets.open("skins/editor-jade.mywubi-skin")
        packageInput.use {input -> context.contentResolver.openOutputStream(uri)!!.use {input.copyTo(it)} }
        try {
            // A repeated run may start with this package already selected. Establish a
            // different preference so enabling a skin must persist a real change.
            val baseline=org.json.JSONObject(original.getJSONObject("config").toString())
            baseline.getJSONObject("skins").getJSONObject("android").put("preset","legacy").remove("selection")
            val current=store.readConfig()
            NativeHandle(current.getLong("version")).use {NativeHandle(store.saveConfig(baseline,it.value).getLong("version")).close()}
            val otherVersion = System.nanoTime().toString()
            val skinBytes = java.io.ByteArrayOutputStream()
            instrumentation.context.assets.open("skins/editor-bamboo.mywubi-skin").use { source ->
                java.util.zip.ZipInputStream(source).use { input ->
                    java.util.zip.ZipOutputStream(skinBytes).use { output ->
                        while (true) {
                            val entry = input.nextEntry ?: break
                            output.putNextEntry(java.util.zip.ZipEntry(entry.name))
                            val bytes = input.readBytes()
                            if (entry.name == "metadata.toml") {
                                output.write(bytes.toString(Charsets.UTF_8).replace("version = \"1\"", "version = \"$otherVersion\"").toByteArray())
                            } else output.write(bytes)
                            output.closeEntry()
                        }
                    }
                }
            }
            val bambooItem = skinBytes.toByteArray().inputStream().use { importSkinDocument(store.configFile, context.cacheDir, it) }.getJSONObject("item")
            assertEquals(otherVersion, bambooItem.getString("version"))
            val bamboo = "竹纸 · $otherVersion"
            val args = InstrumentationRegistry.getArguments()
            val skinId = args.getString("skin_id") ?: "mywubi.jade.copy"
            val firstSkin = args.getString("skin_title") ?: "jade 自定义副本 · 1"
            fun savedSkin(): org.json.JSONObject {
                val result = store.readConfig()
                NativeHandle(result.getLong("version")).close()
                return result.getJSONObject("config").getJSONObject("skins").getJSONObject("android")
            }
            ActivityScenario.launch(MainActivity::class.java).use { scenario ->
                text("皮肤").click()
                text("导入皮肤")
                device.takeScreenshot(File(context.filesDir,"skin-before-picker.png"))
                clickReady("导入皮肤")
                val opened=device.wait(Until.hasObject(By.pkg(java.util.regex.Pattern.compile("com(?:[.]google)?[.]android[.]documentsui"))),30000)
                if(!opened) {
                    device.takeScreenshot(File(context.filesDir,"skin-picker-not-open.png"))
                    device.dumpWindowHierarchy(File(context.filesDir,"skin-picker-not-open.xml"))
                }
                assertTrue("System file picker did not open",opened)
                device.waitForIdle();instrumentation.uiAutomation.clearCache()
                device.findObject(By.desc(java.util.regex.Pattern.compile("Show roots|显示根目录|显示根路径|显示根文件夹|显示导航栏|显示位置")))?.click();device.waitForIdle()
                val rootDeadline=android.os.SystemClock.uptimeMillis()+10000
                while(true) {
                    instrumentation.uiAutomation.clearCache()
                    try {
                        val root=device.findObject(By.res("android","title").text(java.util.regex.Pattern.compile("Downloads|下载(?:内容|项)?")))
                        if(root!=null) {root.click();break}
                    } catch (_:StaleObjectException) { /* DocumentsUI replaces its root nodes while loading. */ }
                    if(android.os.SystemClock.uptimeMillis()>=rootDeadline) {
                        device.dumpWindowHierarchy(File(context.filesDir,"skin-picker-failure.xml"))
                        error("Downloads root unavailable")
                    }
                    android.os.SystemClock.sleep(100)
                }
                device.waitForIdle();instrumentation.uiAutomation.clearCache()
                // MediaStore may append the MIME extension to the supplied display name.
                device.wait(Until.findObject(By.textStartsWith(name)),10000)?.click() ?: error("SAF fixture not listed")
                device.waitForIdle()
                clickReady("预览「$firstSkin」", true)
                text("q", true); text("空格", true)
                device.takeScreenshot(File(context.filesDir,"skin-library-preview.png"))
                assertFalse(savedSkin().has("selection") && !savedSkin().isNull("selection"))
                clickReady("关闭预览")
                clickReady("启用「$firstSkin」", true)
                text("正在使用「$firstSkin」", true)
                device.takeScreenshot(File(context.filesDir,"skin-library.png"))
                assertEquals(skinId, savedSkin().getJSONObject("selection").getString("id"))
                // A display draft must survive switching without being saved implicitly.
                val diskMode = savedSkin().getString("mode")
                val draftMode = if (diskMode == "dark") "light" else "dark"
                scenario.onActivity {
                    val model = androidx.lifecycle.ViewModelProvider(it)[SettingsModel::class.java]
                    val skin = org.json.JSONObject(model.config!!.getJSONObject("skins").getJSONObject("android").toString())
                    model.change("skins", "android", skin.put("mode", draftMode))
                }
                clickReady("启用「$bamboo」", true); text("正在使用「$bamboo」", true)
                assertEquals(otherVersion, savedSkin().getJSONObject("selection").getString("version"))
                assertEquals(diskMode, savedSkin().getString("mode"))
                scenario.recreate()
                text("我的皮肤")
                text("正在使用「$bamboo」", true)
                scenario.onActivity {
                    val model = androidx.lifecycle.ViewModelProvider(it)[SettingsModel::class.java]
                    assertTrue(model.dirty)
                    assertEquals(draftMode, model.config!!.getJSONObject("skins").getJSONObject("android").getString("mode"))
                }
                clickReady("删除皮肤 $bamboo", true)
                clickReady("知道了")
                clickReady("启用「素简」", true); text("正在使用「素简」", true)
                assertEquals("plain", savedSkin().getString("preset"))
                assertTrue(savedSkin().isNull("selection"))
                clickReady("启用「$firstSkin」", true); text("正在使用「$firstSkin」", true)
                clickReady("删除皮肤 $bamboo", true); clickReady("取消")
                clickReady("删除皮肤 $bamboo", true); clickReady("确认删除")
                text("已删除「$bamboo」。")
                assertFalse(File(store.configFile.parentFile, "skins/mywubi.bamboo@$otherVersion.mywubi-skin").exists())
                assertEquals(skinId, savedSkin().getJSONObject("selection").getString("id"))
                val exports=File(context.cacheDir,"skin-exports")
                val before=exports.listFiles().orEmpty().toSet()
                clickReady("分享皮肤 $firstSkin",true)
                assertTrue(device.wait(Until.hasObject(By.text("分享完整皮肤包")),10000) ||
                    device.hasObject(By.res("android","chooser_header")) ||
                    device.executeShellCommand("dumpsys activity activities").lineSequence().filter { it.contains("mResumedActivity") || it.contains("topResumedActivity") }
                        .any { it.contains("chooser",true) || it.contains("resolver",true) })
                val exported=(exports.listFiles().orEmpty().toSet()-before).single()
                fun fingerprints(input:java.io.InputStream)=java.util.zip.ZipInputStream(input).use { stream ->
                    val contents=sortedMapOf<String,String>()
                    while(true) {
                        val entry=stream.nextEntry ?: break
                        if(!entry.isDirectory) contents[entry.name]=java.security.MessageDigest.getInstance("SHA-256")
                            .digest(stream.readBytes()).joinToString("") { "%02x".format(it) }
                    }
                    contents
                }
                val source=packagePath?.let { File(it).inputStream() } ?: instrumentation.context.assets.open("skins/editor-jade.mywubi-skin")
                val hashes=fingerprints(exported.inputStream())
                assertEquals(fingerprints(source),hashes)
                val imported=NativeBridge.objectCall("ManageSkin","request" to org.json.JSONObject()
                    .put("action","Import").put("path",store.configFile.path).put("source",exported.path).put("platform","android"))
                assertTrue(imported.toString(),imported.getBoolean("already_present"))
                File(context.filesDir,"skin-share-roundtrip.json").writeText(org.json.JSONObject(hashes).toString(2))
                device.pressBack()
            }
        } finally {
            val current=store.readConfig()
            NativeHandle(current.getLong("version")).use {NativeHandle(store.saveConfig(original.getJSONObject("config"),it.value).getLong("version")).close()}
            originalDraft?.let {draft.writeBytes(it)} ?: run {if(draft.exists()) check(draft.delete())}
            context.contentResolver.delete(uri,null,null)
        }
    }
}
