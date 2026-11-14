package org.mywubi

import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File

class KeyboardPackageTest {
    /** Optional artifact from an actual Windows editor session, supplied without downloading. */
    @Test fun windowsEditorArtifactPreservesEveryEntryAcrossAndroidExport() {
        val source=InstrumentationRegistry.getArguments().getString("keyboard_package")
        org.junit.Assume.assumeTrue("Supply keyboard_package from Windows editor",source!=null)
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val root=File(context.cacheDir,"windows-keyboard-${System.nanoTime()}").apply {mkdirs()}
        val config=File(root,"config.toml").apply {writeText("")}
        val input=File(source!!)
        NativeBridge.objectCall("ManageSkin","request" to JSONObject().put("action","Import").put("path",config.path).put("source",input.path).put("platform","android"))
        val selection=NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(config,"List"))
            .getJSONArray("items").getJSONObject(0).getJSONObject("selection")
        val read=NativeBridge.objectCall("ReadConfig","path" to config.path)
        NativeHandle(read.getLong("version")).close()
        val choice=JSONObject().put("layout",selection).put("mapping",JSONObject.NULL)
            .put("appearance",read.getJSONObject("config").getJSONObject("skins").getJSONObject("android"))
        val prepared=KeyboardCombination.decode(NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(config,"Prepare")
            .put("choice",choice).put("mode","Main").put("charset","abcdefghijklmnopqrstuvwxyz")))
        assertEquals(15,prepared.portrait.keys.size)
        NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(config,"Save").put("choice",choice)
            .put("mode","Main").put("charset","abcdefghijklmnopqrstuvwxyz").put("digest",prepared.digest))
        val output=File(root,"android-export.mywubi-skin")
        NativeBridge.objectCall("ManageSkin","request" to JSONObject().put("action","Export").put("path",config.path).put("selection",selection).put("target",output.path))
        fun entries(file:File)=java.util.zip.ZipFile(file).use { zip -> zip.entries().asSequence().filterNot {it.isDirectory}
            .associate {it.name to zip.getInputStream(it).use {stream -> java.security.MessageDigest.getInstance("SHA-256").digest(stream.readBytes()).joinToString("") {b -> "%02x".format(b)} }} }
        assertEquals(entries(input),entries(output))
        assertTrue(NativeBridge.objectCall("ManageSkin","request" to JSONObject().put("action","Import").put("path",config.path)
            .put("source",output.path).put("platform","android")).getBoolean("already_present"))
        File(context.filesDir,"ak-windows-roundtrip.json").writeText(JSONObject(entries(output)).toString(2))
    }
    @Test fun templateImportDecodeSaveExportAndReimportPreserveCombination() {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val root=File(context.cacheDir,"keyboard-roundtrip-${System.nanoTime()}").apply {mkdirs()}
        val config=File(root,"config.toml").apply {writeText("")}
        val read=NativeBridge.objectCall("ReadConfig","path" to config.path)
        NativeHandle(read.getLong("version")).close()
        val appearance=read.getJSONObject("config").getJSONObject("skins").getJSONObject("android")
        NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(config,"InstallTemplates"))
        assertTrue(NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(config,"InstallTemplates")).getBoolean("already_present"))
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
