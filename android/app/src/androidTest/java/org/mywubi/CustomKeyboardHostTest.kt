package org.mywubi

import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import android.os.SystemClock
import android.view.InputDevice
import android.view.MotionEvent
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File

/** Real independent EditText + active IME service; no replacement InputConnection. */
class CustomKeyboardHostTest {
    @get:org.junit.Rule val evidence=object:org.junit.rules.TestWatcher() {
        override fun failed(error:Throwable,description:org.junit.runner.Description) {
            val ins=InstrumentationRegistry.getInstrumentation()
            val device=UiDevice.getInstance(ins)
            device.takeScreenshot(File(ins.targetContext.filesDir,"keyboard-switch-host-failure.png"))
            device.dumpWindowHierarchy(File(ins.targetContext.filesDir,"keyboard-switch-host-failure.xml"))
        }
    }
    @Test fun realTemplatesKeepGroupsLocalAndReturnToRememberedMode() {
        val instrumentation=InstrumentationRegistry.getInstrumentation()
        val context=instrumentation.targetContext
        val device=UiDevice.getInstance(instrumentation)
        val store=EngineStore(context)
        val ui=context.getSharedPreferences("android-ui",0)
        val previousClipboard=if(ui.contains("clipboard_suggestion")) ui.getBoolean("clipboard_suggestion",true) else null
        val original=store.readConfig()
        val config=JSONObject(original.getJSONObject("config").toString())
        val receipts=listOf("keyboard-selection.json","keyboard-selection-Pinyin.json","keyboard-selection-English.json")
            .associate {name -> File(store.configFile.parentFile,name).let {it to if(it.exists()) it.readBytes() else null}}
        fun save(value: JSONObject,version:Long) {NativeHandle(version).use {NativeHandle(store.saveConfig(value,it.value).getLong("version")).close()}}
        fun settle(){device.waitForIdle();instrumentation.waitForIdleSync();instrumentation.uiAutomation.clearCache()}
        fun key(name:String){settle();device.wait(Until.findObject(By.desc(name).pkg("org.mywubi").enabled(true)),10000)?.click() ?: error("Missing $name");settle()}
        fun expect(text:String){assertTrue("Host expected $text",awaitHostText(device,By.pkg("org.mywubi.acceptancehost").clazz("android.widget.EditText"),text,10000))}
        fun hold(label: String, options: List<String>, chosen: String, cancel: Boolean = false) {
            val center = requireNotNull(findFresh(device, By.desc(label).pkg("org.mywubi"), 5000)).visibleCenter
            val start = SystemClock.uptimeMillis()
            fun touch(action: Int, x: Int) {
                val event = MotionEvent.obtain(start, SystemClock.uptimeMillis(), action, x.toFloat(), center.y.toFloat(), 0)
                event.source = InputDevice.SOURCE_TOUCHSCREEN
                try { assertTrue(instrumentation.uiAutomation.injectInputEvent(event, true)) } finally { event.recycle() }
            }
            touch(MotionEvent.ACTION_DOWN, center.x)
            var x = center.x
            var released = false
            try {
                val strip = requireNotNull(findFresh(device, By.desc("长按选择"), 5000)) { "No picker for $label" }
                for (option in options) assertNotNull("Missing option $option", strip.findObject(By.text(option)))
                assertEquals(options.size, strip.findObjects(By.text(java.util.regex.Pattern.compile(".+"))).size)
                fun highlighted(option: String): Boolean {
                    // Compose exposes selection as either checked or selected, depending on the node role.
                    val node = findFresh(device, By.desc("长按选择").hasDescendant(By.text(option)), 2000)
                        ?.findObject(By.text(option)) ?: return false
                    return node.isSelected || node.isChecked
                }
                assertTrue("Default highlight for $label", highlighted(options.first()))
                if (chosen != options.first()) {
                    x = strip.findObject(By.text(chosen)).visibleCenter.x
                    touch(MotionEvent.ACTION_MOVE, x)
                    instrumentation.waitForIdleSync()
                    assertTrue("Sliding highlight for $chosen", highlighted(chosen))
                }
                if (!cancel && chosen == options.last()) {
                    device.takeScreenshot(File(context.filesDir, "hold-picker-$label.png"))
                    device.dumpWindowHierarchy(File(context.filesDir, "hold-picker-$label.xml"))
                }
                touch(if (cancel) MotionEvent.ACTION_CANCEL else MotionEvent.ACTION_UP, x)
                released = true
            } finally { if (!released) touch(MotionEvent.ACTION_CANCEL, x) }
            val deadline = SystemClock.uptimeMillis() + 3000
            while (SystemClock.uptimeMillis() < deadline) {
                instrumentation.uiAutomation.clearCache()
                if (!device.hasObject(By.desc("长按选择"))) return
                SystemClock.sleep(40)
            }
            device.dumpWindowHierarchy(File(context.filesDir, "hold-picker-after-release.xml"))
            device.takeScreenshot(File(context.filesDir, "hold-picker-after-release.png"))
            fail("Picker still visible after releasing $label (cancel=$cancel)")
        }
        fun normalized(value:Any?):Any? = when(value) {
            is JSONObject -> value.keys().asSequence().associateWith {normalized(value.get(it))}
            is org.json.JSONArray -> (0 until value.length()).map {normalized(value.get(it))}
            else -> value
        }
        fun select(id:String,mode:String) {
            val items=NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(store.configFile,"List").put("mode",mode)).getJSONArray("items")
            val selection=(0 until items.length()).map {items.getJSONObject(it).getJSONObject("selection")}.single {
                it.getString("id") == "mywubi.keyboards" && it.getString("version") == "4" && it.getString("component") == id
            }
            val choice=JSONObject().put("layout",selection).put("mapping",JSONObject.NULL).put("appearance",config.getJSONObject("skins").getJSONObject("android"))
            val prepared=KeyboardCombination.decode(NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(store.configFile,"Prepare")
                .put("choice",choice).put("mode",mode).put("charset",keyboardCharset(store.configFile,config,mode))))
            NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(store.configFile,"Save")
                .put("choice",choice).put("mode",mode).put("charset",keyboardCharset(store.configFile,config,mode)).put("digest",prepared.digest))
            EngineUpdates.request(context)
        }
        fun candidate(word:String){settle();device.wait(Until.findObject(By.desc(java.util.regex.Pattern.compile("候选 $word(?:，.*)?")).pkg("org.mywubi")),10000)?.click() ?: error("Missing candidate $word");settle()}
        fun switchLayout(component:String,firstKey:String) {
            key("切换形码键盘")
            assertFalse("Pinyin layouts must not be listed as shape keyboards",device.hasObject(By.descStartsWith("形码布局 pinyin9")))
            key("形码布局 $component")
            assertTrue(device.wait(Until.hasObject(By.desc(firstKey).pkg("org.mywubi").enabled(true)),15000))
            val saved=NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(store.configFile,"List").put("mode","Main"))
            assertEquals(component,saved.getJSONObject("choice").getJSONObject("layout").getString("component"))
            assertTrue("Host must keep focus",device.findObject(By.clazz("android.widget.EditText").pkg("org.mywubi.acceptancehost")).isFocused)
        }
        try {
            ui.edit().putBoolean("clipboard_suggestion",false).commit()
            NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(store.configFile,"InstallTemplates"))
            config.getJSONObject("skins").getJSONObject("android").put("selection",
                JSONObject().put("id","mywubi.keyboards").put("version","4").put("component","jade"))
            config.getJSONObject("dictionary").put("system_table","tables/wubi86.dict").put("enable_user_dict",false)
            config.getJSONObject("basic").put("auto_commit_unique",false)
            config.put("prediction",JSONObject().put("enabled",false));config.put("expressions",JSONObject());config.put("shortcuts",JSONObject())
            save(config,original.getLong("version"))
            for(mode in listOf("Main","Pinyin","English")) NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(store.configFile,"Reset").put("mode",mode))
            NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(store.configFile,"InstallTemplates"))
            select("shape14","Main");select("pinyin9","Pinyin")
            device.executeShellCommand("ime enable org.mywubi/.MyWubiService")
            device.executeShellCommand("ime set org.mywubi/.MyWubiService")
            device.executeShellCommand("am force-stop org.mywubi.acceptancehost")
            device.executeShellCommand("am start -n org.mywubi.acceptancehost/.EditorActivity")
            val field=device.wait(Until.findObject(By.clazz("android.widget.EditText").pkg("org.mywubi.acceptancehost")),15000)
            focusExternalEditor(device)
            assertTrue(device.wait(Until.hasObject(By.desc("GH").pkg("org.mywubi").enabled(true)),15000))
            key("切换形码键盘")
            val option = device.wait(Until.findObject(By.desc("形码布局 shape14，当前布局")),5000)
            device.dumpWindowHierarchy(File(context.filesDir,"keyboard-switch-host-menu.xml"))
            val selected = requireNotNull(option) { "Current layout is missing from the opened keyboard menu" }.actionNode()
            assertTrue(selected.isChecked || selected.isSelected)
            device.pressBack();expect("")
            switchLayout("full26","q")
            key("g");key("g");key("l");key("l");candidate("一");expect("一")
            key("退格");expect("")
            switchLayout("shape14","GH")
            val paired = listOf("q", "w")
            hold("QW", paired, "w", cancel = true); expect("")
            hold("QW", paired, "w"); expect("w")
            hold("QW", paired, "q"); expect("wq")
            key("退格"); key("退格"); expect("")
            val idleBounds=device.findObject(By.desc("GH").pkg("org.mywubi")).visibleBounds
            key("GH");expect("")
            assertFalse("A composing group must not be reinterpreted by a layout switch",device.hasObject(By.desc("切换形码键盘")))
            assertEquals("Group controls must not move keys while typing",idleBounds,device.findObject(By.desc("GH").pkg("org.mywubi")).visibleBounds)
            key("GH");key("L");key("L");expect("");candidate("一");expect("一")
            device.takeScreenshot(File(context.filesDir,"ak14-real-host.png"))
            key("中 → 英");key("a");key("b");key("c");expect("一abc");key("英 → 中")
            switchLayout("shape9","GHI")
            assertTrue(device.wait(Until.hasObject(By.desc("GHI").pkg("org.mywubi").enabled(true)),10000))
            key("GHI");key("GHI");key("JKL");key("JKL");expect("一abc");candidate("一");expect("一abc一")
            key("123")
            assertFalse(device.hasObject(By.desc("切换形码键盘")))
            key("ABC")
            assertTrue(device.hasObject(By.desc("GHI").pkg("org.mywubi")))
            key("符号")
            assertFalse(device.hasObject(By.desc("切换形码键盘")))
            key("ABC")
            val savedConfig=store.readConfig();NativeHandle(savedConfig.getLong("version")).close()
            assertEquals(normalized(config.getJSONObject("skins")),normalized(savedConfig.getJSONObject("config").getJSONObject("skins")))
            key("拼音")
            key("MNO");key("GHI");expect("一abc一");candidate("你");expect("一abc一你")
            assertTrue(device.wait(Until.hasObject(By.desc("PQR").pkg("org.mywubi").enabled(true)),10000))
            device.takeScreenshot(File(context.filesDir,"ak9-pinyin-real-host.png"))
            val journal=device.executeShellCommand("cat /sdcard/Android/data/org.mywubi.acceptancehost/files/edits.jsonl")
            assertFalse("Internal group markers leaked into real host",journal.contains("[") || journal.contains("]"))
            File(context.filesDir,"ak-host-edits.jsonl").writeText(journal)
            // Saved group layouts must never enter password, number, URI or email fields.
            val directFailures = mutableListOf<String>()
            val editor = By.pkg("org.mywubi.acceptancehost").clazz("android.widget.EditText")
            for ((inputType,label) in listOf(2 to "1",17 to "q",33 to "q",129 to "q")) {
                device.executeShellCommand("am start -W -f 0x10008000 -n org.mywubi.acceptancehost/.EditorActivity --ei inputType $inputType")
                val fieldBounds = requireNotNull(findFresh(device,editor,5000)).visibleBounds
                device.click(fieldBounds.centerX(),fieldBounds.top + 24)
                val target = findFresh(device,By.pkg("org.mywubi").desc(label).enabled(true),5000)
                if (target == null) {
                    directFailures += "inputType=$inputType: MyWubi key $label unavailable"
                    continue
                }
                val box = target.visibleBounds
                // The lower-case q occupies the leftmost key beside Android's back gesture.
                device.click(if (label == "q") box.right-box.width()/8 else box.centerX(),box.centerY())
                if (!awaitHostText(device,editor,label,4000)) {
                    directFailures += "inputType=$inputType: host did not receive $label"
                    continue
                }
                assertFalse("Shape layout menu leaked into direct field",device.hasObject(By.desc("切换形码键盘")))
                assertFalse("Group keyboard leaked into inputType $inputType",device.hasObject(By.desc("GHI").pkg("org.mywubi")))
                val edits=device.executeShellCommand("cat /sdcard/Android/data/org.mywubi.acceptancehost/files/edits.jsonl")
                assertEquals(label,JSONObject(edits.lineSequence().last {it.isNotBlank()}).getString("text"))
            }
            device.executeShellCommand("am force-stop org.mywubi.acceptancehost")
            device.executeShellCommand("am start -n org.mywubi.acceptancehost/.EditorActivity")
            focusExternalEditor(device)
            assertTrue(device.wait(Until.hasObject(By.desc("GHI").pkg("org.mywubi")),10000))
            key("切换形码键盘")
            val restore=device.wait(Until.findObject(By.text("原始全键盘").pkg("org.mywubi")),5000)
            restore.click();assertTrue(device.wait(Until.hasObject(By.desc("q").pkg("org.mywubi").enabled(true)),10000))
            hold("q", listOf("1", "q"), "1"); expect("1")
            hold("q", listOf("1", "q"), "q"); expect("1q")
            assertTrue(device.findObject(By.copy(editor).focused(true)).isFocused)
            assertTrue(directFailures.joinToString("; "), directFailures.isEmpty())
        } finally {
            ui.edit().apply { if(previousClipboard==null) remove("clipboard_suggestion") else putBoolean("clipboard_suggestion",previousClipboard) }.commit()
            device.pressBack()
            val loaded=store.readConfig();save(original.getJSONObject("config"),loaded.getLong("version"))
            for((file,bytes) in receipts) {if(bytes==null) file.delete() else file.writeBytes(bytes)}
            EngineUpdates.request(context)
            device.executeShellCommand("am force-stop org.mywubi.acceptancehost")
        }
    }
}
