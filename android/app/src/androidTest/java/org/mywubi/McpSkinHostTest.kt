package org.mywubi

import android.view.accessibility.AccessibilityNodeInfo
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeNotNull
import org.junit.Test
import java.io.File
import java.security.MessageDigest

/** The desktop runner stages the exact exported package on an explicitly selected test device. */
class McpSkinHostTest {
    @Test fun exportedSkinInExternalEditor() {
        val args = InstrumentationRegistry.getArguments()
        val job = args.getString("skinJob")
        assumeNotNull(job)
        require(job!!.matches(Regex("[a-f0-9-]{36}")))
        val ins = InstrumentationRegistry.getInstrumentation()
        val context = ins.targetContext
        val device = UiDevice.getInstance(ins)
        val root = File(context.getExternalFilesDir(null), "mcp-$job")
        val source = File(root, "skin.mywubi-skin")
        val hash = MessageDigest.getInstance("SHA-256").digest(source.readBytes()).joinToString("") { "%02x".format(it) }
        assertEquals(args.getString("sha256"), hash)
        val path = Resources.prepare(context)
        val store = EngineStore(context)
        val loaded = store.readConfig()
        val original = loaded.getJSONObject("config")
        val config = JSONObject(original.toString())
        fun save(value: JSONObject) {
            NativeHandle(store.readConfig().getLong("version")).use { old ->
                NativeHandle(store.saveConfig(value, old.value).getLong("version")).close()
            }
        }
        val previous = device.executeShellCommand("settings get secure default_input_method").trim()
        val ime = android.content.ComponentName(context, MyWubiService::class.java).flattenToShortString()
        val enabled = device.executeShellCommand("ime list -s").lines().any { it.trim() == ime }
        val targetHost = "org.mywubi.acceptancehost"
        val field = By.pkg(targetHost).clazz("android.widget.EditText")
        var installed: File? = null
        try {
            val imported = NativeBridge.objectCall("ManageSkin","request" to JSONObject().put("action","Import")
                .put("path",path.path).put("source",source.path).put("platform","android"))
            val item = imported.getJSONObject("item")
            val selection = JSONObject().put("id",item.getString("id")).put("version",item.getString("version"))
                .put("component",args.getString("component"))
            if (imported.has("already_present") && !imported.getBoolean("already_present")) {
                installed = File(File(path.parentFile,"skins"), "${item.getString("id")}@${item.getString("version")}.mywubi-skin")
            }
            config.getJSONObject("skins").getJSONObject("android").put("selection",selection).put("mode","dark")
            val table = File(root,"main.dict").apply { writeText("ab\t以及\t10\nab\t已经\t9\n") }
            config.getJSONObject("dictionary").put("system_table",table.path).put("enable_user_dict",false)
            config.getJSONObject("basic").put("auto_commit_unique",false)
            config.put("prediction",JSONObject().put("enabled",false))
            config.put("frequency",JSONObject().put("record",false).put("rank",false))
            assertFalse(loadSkinAppearance(path, config).getJSONObject("dark").getBoolean("legacy"))
            save(config)
            device.executeShellCommand("ime enable $ime")
            device.executeShellCommand("ime set $ime")
            device.executeShellCommand("am start -W -S -n $targetHost/.EditorActivity")
            focusExternalEditor(device)
            if (device.hasObject(By.desc("英 → 中"))) findFresh(device,By.desc("英 → 中"),3000)!!.click()
            for (key in listOf("a","b")) requireNotNull(findFresh(device,By.pkg(context.packageName).desc(key).enabled(true),8000)) { "Missing key $key" }.click()
            val candidate = By.pkg(context.packageName).descStartsWith("候选 以及")
            requireNotNull(findFresh(device,candidate,10000)) { "Missing candidate 以及" }
            assertTrue(device.takeScreenshot(File(root,"android.png")))
            requireNotNull(findFresh(device,candidate,3000)).click()
            assertTrue("external editor did not receive 以及",awaitHostText(device,field,"以及",8000))
            ins.uiAutomation.clearCache()
            val focused = ins.uiAutomation.windows.mapNotNull { it.root?.findFocus(AccessibilityNodeInfo.FOCUS_INPUT) }
                .first { it.packageName == targetHost }
            assertEquals("以及",focused.text.toString())
            assertEquals(2,focused.textSelectionStart)
            assertEquals(2,focused.textSelectionEnd)
            File(root,"assertions.json").writeText(JSONObject().put("text","以及").put("focus",true)
                .put("selectionStart",2).put("selectionEnd",2).put("sha256",hash).toString())
        } finally {
            try { save(original) } finally {
                device.executeShellCommand("am force-stop $targetHost")
                if (previous.contains('/')) device.executeShellCommand("ime set $previous")
                if (!enabled) device.executeShellCommand("ime disable $ime")
                installed?.delete()
            }
        }
    }
}
