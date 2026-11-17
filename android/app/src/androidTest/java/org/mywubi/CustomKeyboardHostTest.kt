package org.mywubi

import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
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
        fun expect(text:String){assertTrue("Host expected $text",device.wait(Until.hasObject(By.clazz("android.widget.EditText").text(text)),10000))}
        fun normalized(value:Any?):Any? = when(value) {
            is JSONObject -> value.keys().asSequence().associateWith {normalized(value.get(it))}
            is org.json.JSONArray -> (0 until value.length()).map {normalized(value.get(it))}
            else -> value
        }
        fun select(id:String,mode:String) {
            val items=NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(store.configFile,"List").put("mode",mode)).getJSONArray("items")
            val selection=(0 until items.length()).map {items.getJSONObject(it).getJSONObject("selection")}.first {it.getString("component")==id}
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
            field.click()
            assertTrue(device.wait(Until.hasObject(By.desc("GH").pkg("org.mywubi").enabled(true)),15000))
            key("切换形码键盘")
            val selected=device.wait(Until.findObject(By.desc("形码布局 shape14，当前布局")),5000).actionNode()
            device.dumpWindowHierarchy(File(context.filesDir,"keyboard-switch-host-menu.xml"))
            assertTrue(selected.isChecked || selected.isSelected)
            device.pressBack();expect("")
            switchLayout("full26","q")
            key("g");key("g");key("l");key("l");candidate("王");expect("王")
            key("退格");expect("")
            switchLayout("shape14","GH")
            val idleBounds=device.findObject(By.desc("GH").pkg("org.mywubi")).visibleBounds
            key("GH");expect("")
            assertFalse("A composing group must not be reinterpreted by a layout switch",device.hasObject(By.desc("切换形码键盘")))
            assertEquals("Group controls must not move keys while typing",idleBounds,device.findObject(By.desc("GH").pkg("org.mywubi")).visibleBounds)
            key("GH");key("L");key("L");expect("");candidate("王");expect("王")
            device.takeScreenshot(File(context.filesDir,"ak14-real-host.png"))
            key("中 → 英");key("a");key("b");key("c");expect("王abc");key("英 → 中")
            switchLayout("shape9","GHI")
            assertTrue(device.wait(Until.hasObject(By.desc("GHI").pkg("org.mywubi").enabled(true)),10000))
            key("GHI");key("GHI");key("JKL");key("JKL");expect("王abc");candidate("王");expect("王abc王")
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
            key("MNO");key("GHI");expect("王abc王");candidate("你");expect("王abc王你")
            assertTrue(device.wait(Until.hasObject(By.desc("PQR").pkg("org.mywubi").enabled(true)),10000))
            device.takeScreenshot(File(context.filesDir,"ak9-pinyin-real-host.png"))
            val journal=device.executeShellCommand("cat /sdcard/Android/data/org.mywubi.acceptancehost/files/edits.jsonl")
            assertFalse("Internal group markers leaked into real host",journal.contains("[") || journal.contains("]"))
            File(context.filesDir,"ak-host-edits.jsonl").writeText(journal)
            // Saved group layouts must never enter password, number, URI or email fields.
            for((inputType,label) in listOf(129 to "q",2 to "1",17 to "q",33 to "q")) {
                device.executeShellCommand("am force-stop org.mywubi.acceptancehost")
                device.executeShellCommand("am start -n org.mywubi.acceptancehost/.EditorActivity --ei inputType $inputType")
                device.wait(Until.findObject(By.clazz("android.widget.EditText").pkg("org.mywubi.acceptancehost")),10000).click()
                key(label)
                assertFalse("Shape layout menu leaked into direct field",device.hasObject(By.desc("切换形码键盘")))
                assertFalse("Group keyboard leaked into inputType $inputType",device.hasObject(By.desc("GHI").pkg("org.mywubi")))
                val edits=device.executeShellCommand("cat /sdcard/Android/data/org.mywubi.acceptancehost/files/edits.jsonl")
                assertEquals(label,JSONObject(edits.lineSequence().last {it.isNotBlank()}).getString("text"))
            }
            device.executeShellCommand("am force-stop org.mywubi.acceptancehost")
            device.executeShellCommand("am start -n org.mywubi.acceptancehost/.EditorActivity")
            device.wait(Until.findObject(By.clazz("android.widget.EditText").pkg("org.mywubi.acceptancehost")),10000).click()
            assertTrue(device.wait(Until.hasObject(By.desc("GHI").pkg("org.mywubi")),10000))
            val restore=device.wait(Until.findObject(By.text("取消输入并恢复全键布局").pkg("org.mywubi")),5000)
            restore.click();assertTrue(device.wait(Until.hasObject(By.desc("q").pkg("org.mywubi").enabled(true)),10000))
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
