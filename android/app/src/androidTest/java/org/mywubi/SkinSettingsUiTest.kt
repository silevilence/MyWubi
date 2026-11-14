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
    private fun text(label:String):UiObject2 {
        instrumentation.uiAutomation.clearCache()
        device.findObject(By.text(label))?.let {return it}
        UiScrollable(UiSelector().description("设置内容").scrollable(true)).setAsVerticalList().scrollToBeginning(30)
        repeat(30) {
            instrumentation.uiAutomation.clearCache()
            device.findObject(By.text(label))?.let {return it}
            val bounds=device.findObject(By.desc("设置内容").scrollable(true))?.visibleBounds
            if(bounds!=null) device.swipe(bounds.centerX(),bounds.bottom-20,bounds.centerX(),bounds.top+bounds.height()/3,20)
            device.waitForIdle()
        }
        error("Missing $label")
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
        instrumentation.context.assets.open("skins/editor-jade.mywubi-skin").use {input -> context.contentResolver.openOutputStream(uri)!!.use {input.copyTo(it)} }
        try {
            ActivityScenario.launch(MainActivity::class.java).use { scenario ->
                text("键盘").click();text("从文件导入皮肤包").click()
                device.wait(Until.findObject(By.desc("Show roots")),5000)?.click();device.waitForIdle()
                val rootDeadline=android.os.SystemClock.uptimeMillis()+10000
                while(true) {
                    instrumentation.uiAutomation.clearCache()
                    try {
                        val root=device.findObject(By.res("android","title").text("Downloads"))
                        if(root!=null) {root.click();break}
                    } catch (_:StaleObjectException) { /* DocumentsUI replaces its root nodes while loading. */ }
                    check(android.os.SystemClock.uptimeMillis()<rootDeadline) {"Downloads root unavailable"}
                    android.os.SystemClock.sleep(100)
                }
                device.waitForIdle();instrumentation.uiAutomation.clearCache()
                // MediaStore may append the MIME extension to the supplied display name.
                device.wait(Until.findObject(By.textStartsWith(name)),10000)?.click() ?: error("SAF fixture not listed")
                device.waitForIdle()
                text("选用 jade").click()
                assertTrue(text("有未保存更改 · 草稿会保留").isEnabled)
                text("预览当前草稿").click();text("Android 原生键盘预览")
                scenario.recreate();text("键盘").click();text("预览当前草稿")
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
                text("导出并分享完整包").click()
                assertTrue(device.wait(Until.hasObject(By.text("分享完整皮肤包")),10000) ||
                    device.hasObject(By.res("android","chooser_header")))
                device.takeScreenshot(File(context.filesDir,"skin-system-share.png"))
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
