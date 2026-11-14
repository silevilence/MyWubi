package org.mywubi

import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File

class KeyboardPackageTest {
    @Test fun templateImportDecodeSaveExportAndReimportPreserveCombination() {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val root=File(context.cacheDir,"keyboard-roundtrip-${System.nanoTime()}").apply {mkdirs()}
        val config=File(root,"config.toml").apply {writeText("")}
        val read=NativeBridge.objectCall("ReadConfig","path" to config.path)
        NativeHandle(read.getLong("version")).close()
        val appearance=read.getJSONObject("config").getJSONObject("skins").getJSONObject("android")
        NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(config,"InstallTemplates"))
        val items=NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(config,"List")).getJSONArray("items")
        for(i in 0 until items.length()) {
            val item=items.getJSONObject(i);val selection=item.getJSONObject("selection")
            val mode=if(selection.getString("component")=="pinyin9") "Pinyin" else "Main"
            val charset=if(mode=="Pinyin") "abcdefghijklmnopqrstuvwxyz'" else "abcdefghijklmnopqrstuvwxyz"
            val choice=JSONObject().put("layout",selection).put("mapping",JSONObject.NULL).put("appearance",appearance)
            val prepared=NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(config,"Prepare").put("choice",choice).put("mode",mode).put("charset",charset))
            val decoded=KeyboardCombination.decode(prepared)
            assertTrue(decoded.portrait.keys.isNotEmpty())
            NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(config,"Save").put("choice",choice).put("mode",mode).put("charset",charset).put("digest",decoded.digest))
            val restored=NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(config,"Restore").put("mode",mode).put("charset",charset))
            assertEquals(decoded.digest,restored.getString("digest"))
            val output=File(root,"export-$i.mywubi-skin")
            NativeBridge.objectCall("ManageSkin","request" to JSONObject().put("action","Export").put("path",config.path).put("selection",selection).put("target",output.path))
            val imported=NativeBridge.objectCall("ManageSkin","request" to JSONObject().put("action","Import").put("path",config.path).put("source",output.path).put("platform","android"))
            assertTrue(imported.getBoolean("already_present"))
            val again=NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(config,"Restore").put("mode",mode).put("charset",charset))
            assertEquals(prepared.getJSONObject("resource_digests").toString(),again.getJSONObject("resource_digests").toString())
        }
    }
}
