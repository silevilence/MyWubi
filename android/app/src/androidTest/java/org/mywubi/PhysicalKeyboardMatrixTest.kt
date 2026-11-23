package org.mywubi

import android.content.res.Configuration
import android.os.SystemClock
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeNotNull
import org.junit.Test
import java.io.File
import kotlin.math.cos
import kotlin.math.sin

/** Opt-in joint SK-20 matrix. Each result requires committed text in the independent host. */
class PhysicalKeyboardMatrixTest {
    @Test fun gesturesCommitWithEachSkinLayoutThemeAndOrientation() {
        val fixtureDir = InstrumentationRegistry.getArguments().getString("matrix_dir")
        assumeNotNull(fixtureDir)
        val probe=InstrumentationRegistry.getArguments().getString("matrix_probe")=="true"
        fun <T> cases(items:List<T>)=if(probe)items.take(1) else items
        val ins = InstrumentationRegistry.getInstrumentation()
        val app = ins.targetContext
        val device = UiDevice.getInstance(ins)
        val store = EngineStore(app)
        val original = store.readConfig()
        NativeHandle(original.getLong("version")).close()
        val config = JSONObject(original.getJSONObject("config").toString())
        val receipt = File(store.configFile.parentFile, "keyboard-selection.json")
        val previousReceipt = receipt.takeIf { it.isFile }?.readBytes()
        val previousIme = device.executeShellCommand("settings get secure default_input_method").trim()
        val previousNight = device.executeShellCommand("cmd uimode night").substringAfter("Night mode: ").trim()
        require(previousNight in listOf("yes", "no", "auto"))
        val report = File(app.filesDir, "physical-matrix.jsonl")
        val table = File(app.cacheDir, "physical-matrix.dict")
        val glyphs = "甲乙丙丁戊己庚辛"
        table.writeText("---\ncharset: abcdefghijklmnopqrstuvwxyz\nmax_code_len: 4\n---\n" + ('a'..'h').zip(glyphs.asIterable())
            .joinToString("\n") { (key, word) -> "$key\t$word\t100" } + "\n")
        val host = "org.mywubi.acceptancehost"
        fun editor() = By.pkg(host).clazz("android.widget.EditText")
        fun key(label: String) = requireNotNull(device.wait(Until.findObject(
            By.pkg("org.mywubi").desc(label).enabled(true)), 5000)) { "Missing key $label" }
        fun save(value: JSONObject) {
            NativeHandle(store.readConfig().getLong("version")).use {
                NativeHandle(store.saveConfig(value, it.value).getLong("version")).close()
            }
        }
        fun select(id: String, layout: String) {
            val appearance = config.getJSONObject("skins").getJSONObject("android")
            appearance.put("selection", JSONObject().put("id", id).put("version", "1").put("component", "jade"))
            val selection = JSONObject().put("id", id).put("version", "1").put("component", layout)
            val choice = JSONObject().put("layout", selection).put("mapping", JSONObject.NULL).put("appearance", appearance)
            fun request(op: String) = keyboardRequest(store.configFile, op).put("choice", choice)
                .put("mode", "Main").put("charset", keyboardCharset(store.configFile, config, "Main"))
            val prepared = KeyboardCombination.decode(NativeBridge.objectCall("ManageKeyboard", "request" to request("Prepare")))
            NativeBridge.objectCall("ManageKeyboard", "request" to request("Save").put("digest", prepared.digest))
            save(config)
            EngineUpdates.request(app)
            val prefs = EngineUpdates.preferences(app)
            val end = SystemClock.elapsedRealtime() + 10000
            while (prefs.getString("applied", null) != prefs.getString("requested", null) && SystemClock.elapsedRealtime() < end)
                SystemClock.sleep(60)
            assertEquals(prefs.getString("error", null), prefs.getString("requested", null), prefs.getString("applied", null))
        }
        fun fresh(selector: androidx.test.uiautomator.BySelector, ms: Long = 5000): androidx.test.uiautomator.UiObject2? {
            val until=SystemClock.uptimeMillis()+ms
            do {
                ins.uiAutomation.clearCache()
                try { device.findObject(selector)?.let { return it } } catch (_: androidx.test.uiautomator.StaleObjectException) { }
                SystemClock.sleep(60)
            } while(SystemClock.uptimeMillis()<until)
            return null
        }
        val automation=ins.uiAutomation
        val originalFlags=automation.serviceInfo.flags
        val serviceInfo=automation.serviceInfo
        serviceInfo.flags=originalFlags or android.accessibilityservice.AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS
        automation.serviceInfo=serviceInfo
        fun directionalActions(label:String):List<String> {
            val nodes=java.util.ArrayDeque(automation.windows.mapNotNull { it.root }.filter { it.packageName=="org.mywubi" })
            while(nodes.isNotEmpty()) {
                val n=nodes.removeFirst()
                if(n.contentDescription?.toString()==label && n.actionList.any { it.label?.contains("滑") == true })
                    return n.actionList.mapNotNull { it.label?.toString() }.filter { it.contains("滑") }
                for(i in 0 until n.childCount)n.getChild(i)?.let(nodes::add)
            }
            return emptyList()
        }
        var failures = 0
        var completed = 0
        try {
            report.writeText("")
            config.getJSONObject("dictionary").put("system_table", table.absolutePath).put("enable_user_dict", false)
            config.getJSONObject("basic").put("auto_commit_unique", false)
            config.put("personal", JSONObject()).put("prediction", JSONObject()).put("expressions", JSONObject())
            device.executeShellCommand("ime enable org.mywubi/.MyWubiService")
            device.executeShellCommand("ime set org.mywubi/.MyWubiService")
            device.executeShellCommand("am force-stop $host")
            device.executeShellCommand("am start -W -n $host/.EditorActivity")
            requireNotNull(device.wait(Until.findObject(editor()), 10000)).click()
            for (skin in cases(listOf("parameters", "image"))) for (profile in cases(listOf("eight", "merged"))) {
                val id = "pgz.matrix.$skin.$profile"
                val input = File(fixtureDir, "$id.mywubi-skin")
                val result = NativeBridge.objectCall("ManageSkin", "request" to JSONObject().put("action", "Import")
                    .put("path", store.configFile.path).put("source", input.path).put("platform", "android"))
                assertEquals(id, result.getJSONObject("item").getString("id"))
                for ((layout, label) in cases(listOf("full26" to "s", "shape14" to "DF", "shape9" to "MNO"))) {
                    select(id, layout)
                    for (night in cases(listOf("no", "yes"))) {
                        device.executeShellCommand("cmd uimode night $night")
                        for (orientation in cases(listOf("portrait", "landscape"))) {
                            val row = JSONObject().put("skin", skin).put("profile", profile).put("layout", layout)
                                .put("night", night).put("orientation", orientation)
                            try {
                                if (orientation == "portrait") device.setOrientationNatural() else device.setOrientationLeft()
                                device.waitForIdle()
                                val wide = device.displayWidth > device.displayHeight
                                assertEquals(orientation == "landscape", wide)
                                val actualNight = app.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK
                                assertEquals(night == "yes", actualNight == Configuration.UI_MODE_NIGHT_YES)
                                if(completed>0) {
                                    device.executeShellCommand("am force-stop $host")
                                    device.executeShellCommand("am start -W -n $host/.EditorActivity")
                                }
                                val field = requireNotNull(fresh(editor(),6000))
                                field.text = ""
                                field.click()
                                if (device.hasObject(By.pkg("org.mywubi").desc("英 → 中"))) key("英 → 中").click()
                                val appearance = loadSkinAppearance(store.configFile, config)
                                val variant = appearance.getJSONObject(if (night == "yes") "dark" else "light")
                                val images = variant.get("decodedImages") as SkinImages
                                assertEquals(skin == "image", images.roles.isNotEmpty())
                                requireNotNull(fresh(By.pkg("org.mywubi").desc(label).enabled(true)))
                                val actions=directionalActions(label)
                                row.put("named_actions",org.json.JSONArray(actions))
                                row.put("observations",org.json.JSONArray())
                                var expected = ""
                                for (direction in 0 until (if (probe) 2 else 8)) {
                                    val target = requireNotNull(fresh(By.pkg("org.mywubi").desc(label).enabled(true)))
                                    val center = target.visibleCenter
                                    val radius = app.resources.displayMetrics.density * 36
                                    val angle = direction * Math.PI / 4
                                    val toX = center.x + (sin(angle) * radius).toInt()
                                    val toY = center.y - (cos(angle) * radius).toInt()
                                    assertTrue(device.swipe(center.x, center.y, toX, toY, 6))
                                    val directions = if (profile == "eight") glyphs else "甲甲丙戊戊戊庚甲"
                                    val word = directions[direction].toString()
                                    val candidate = requireNotNull(fresh(By.pkg("org.mywubi").descStartsWith("候选 $word"))) {
                                        val present=device.findObjects(By.pkg("org.mywubi").descStartsWith("候选 ")).map { it.contentDescription }
                                        "Missing candidate $word after direction=$direction at $center; editor=${device.findObject(editor())?.text}; candidates=$present"
                                    }
                                    row.getJSONArray("observations").put(JSONObject().put("direction",direction).put("name",candidate.contentDescription))
                                    val point = candidate.visibleCenter
                                    device.click(point.x, point.y)
                                    expected += word
                                    assertTrue("Host expected $expected", device.wait(Until.hasObject(editor().text(expected).focused(true)), 4000))
                                }
                                assertEquals("Named direction actions",8,actions.distinct().size)
                                row.put("host", expected).put("result", "pass")
                            } catch (e: Exception) {
                                row.put("result", "fail").put("error", e.toString())
                                failures++
                            } catch (e: AssertionError) {
                                row.put("result", "fail").put("error", e.toString())
                                failures++
                            }
                            report.appendText(row.toString() + "\n")
                            completed++
                        }
                    }
                }
            }
            assertEquals(if(probe)1 else 48, completed)
            assertEquals("See physical-matrix.jsonl", 0, failures)
        } finally {
            val restored=automation.serviceInfo
            restored.flags=originalFlags
            automation.serviceInfo=restored
            device.executeShellCommand("cmd uimode night $previousNight")
            device.unfreezeRotation()
            if (previousReceipt == null) receipt.delete() else receipt.writeBytes(previousReceipt)
            save(original.getJSONObject("config"))
            device.executeShellCommand("am force-stop $host")
            if (previousIme.isNotEmpty() && previousIme != "null") device.executeShellCommand("ime set $previousIme")
            table.delete()
        }
    }
}
