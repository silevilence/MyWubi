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
    private fun text(label:String, description:Boolean=false):UiObject2 {
        val selector=if(description) By.desc(label) else By.text(label)
        instrumentation.uiAutomation.clearCache()
        device.findObject(selector)?.let {return it}
        fun scroll(forward:Boolean):Boolean {
            instrumentation.uiAutomation.clearCache()
            val root=instrumentation.uiAutomation.rootInActiveWindow ?: return false
            val queue=java.util.ArrayDeque(listOf(root))
            while(queue.isNotEmpty()) {
                val n=queue.removeFirst()
                if(n.packageName==context.packageName && n.contentDescription=="设置内容" && n.isScrollable) {
                    val action=if(forward)android.view.accessibility.AccessibilityNodeInfo.ACTION_SCROLL_FORWARD
                        else android.view.accessibility.AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD
                    return n.performAction(action)
                }
                for(i in 0 until n.childCount)n.getChild(i)?.let(queue::add)
            }
            return false
        }
        for(forward in listOf(false,true)) {
            for(step in 0..45) {
                instrumentation.uiAutomation.clearCache()
                device.findObject(selector)?.let { return it }
                if(!scroll(forward))break
                android.os.SystemClock.sleep(180)
            }
        }
        error("Missing $label")
    }
    private fun clickReady(label:String, description:Boolean=false) {
        text(label,description)
        val target=if(description) By.desc(label) else By.text(label)
        val deadline=android.os.SystemClock.uptimeMillis()+10000
        while(android.os.SystemClock.uptimeMillis()<deadline) {
            device.waitForIdle()
            instrumentation.uiAutomation.clearCache()
            val button=device.findObject(target)?.actionNode()
            if(button!=null && button.isEnabled && button.visibleBounds.height() >= 40*context.resources.displayMetrics.density) {
                button.click();device.waitForIdle();instrumentation.uiAutomation.clearCache()
                return
            }
            if(button!=null && button.visibleBounds.height() < 40*context.resources.displayMetrics.density) {
                val bounds=device.findObject(By.desc("设置内容"))?.visibleBounds ?: error("Missing settings viewport")
                device.swipe(bounds.left+bounds.width()/8,bounds.bottom-20,bounds.left+bounds.width()/8,bounds.centerY(),20)
            } else android.os.SystemClock.sleep(100)
        }
        error("Control stayed unavailable: $label")
    }
    @get:org.junit.Rule val evidence=object:org.junit.rules.TestWatcher() {
        override fun failed(error:Throwable,description:org.junit.runner.Description) {
            device.takeScreenshot(File(context.filesDir,"skin-settings-failure.png"))
            device.dumpWindowHierarchy(File(context.filesDir,"skin-settings-failure.xml"))
        }
    }

    @Test fun safImportNativePreviewDraftSaveAndSystemShare() {
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
            // different preference so selection must create a real unsaved draft.
            val baseline=org.json.JSONObject(original.getJSONObject("config").toString())
            baseline.getJSONObject("skins").getJSONObject("android").put("preset","legacy").remove("selection")
            val current=store.readConfig()
            NativeHandle(current.getLong("version")).use {NativeHandle(store.saveConfig(baseline,it.value).getLong("version")).close()}
            ActivityScenario.launch(MainActivity::class.java).use { scenario ->
                text("皮肤").click()
                val importButton=text("从文件导入皮肤包")
                device.waitForIdle()
                device.takeScreenshot(File(context.filesDir,"skin-before-picker.png"))
                importButton.click()
                val opened=device.wait(Until.hasObject(By.pkg(java.util.regex.Pattern.compile("com(?:[.]google)?[.]android[.]documentsui"))),10000)
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
                clickReady("选用皮肤 jade 自定义副本，组件 jade",true)
                device.takeScreenshot(File(context.filesDir,"skin-selected-component.png"))
                device.dumpWindowHierarchy(File(context.filesDir,"skin-selected-component.xml"))
                scenario.onActivity {
                    val model=androidx.lifecycle.ViewModelProvider(it)[SettingsModel::class.java]
                    File(context.filesDir,"skin-selected-state.json").writeText(org.json.JSONObject()
                        .put("dirty",model.dirty).put("skin",model.config?.getJSONObject("skins")?.getJSONObject("android")).toString())
                    assertTrue("Selecting the imported component must create a draft",model.dirty)
                }
                text("有未保存更改 · 草稿会保留")
                clickReady("预览当前草稿");text("Android 原生键盘预览")
                text("q",true);text("空格",true)
                scenario.recreate();text("皮肤").click();text("预览当前草稿")
                assertTrue(text("草稿选择：mywubi.jade.copy @ 1 / jade").isEnabled)
                // Save validates the selected package before any persistent activation.
                text("保存设置").click()
                val end=android.os.SystemClock.uptimeMillis()+10000
                var selected=false
                do {
                    val read=store.readConfig();NativeHandle(read.getLong("version")).close()
                    selected=read.getJSONObject("config").getJSONObject("skins").getJSONObject("android")
                        .optJSONObject("selection")?.optString("id")=="mywubi.jade.copy"
                    if(!selected) android.os.SystemClock.sleep(100)
                } while(!selected && android.os.SystemClock.uptimeMillis()<end)
                assertTrue("Selection was not saved",selected)
                val exports=File(context.cacheDir,"skin-exports")
                val before=exports.listFiles().orEmpty().toSet()
                clickReady("分享皮肤 jade 自定义副本 完整包",true)
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
