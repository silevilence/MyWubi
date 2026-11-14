package org.mywubi

import java.io.File
import org.json.JSONObject

/** Fully decoded before publication; the view never independently reloads its components. */
internal data class KeyboardCombination(val wire: JSONObject, val portrait: KeyboardLayout,
    val landscape: KeyboardLayout, val appearance: JSONObject) {
    val identity get() = wire.optJSONObject("mapping_identity")
    val definition get() = wire.optJSONObject("definition")
    val mode get() = wire.getJSONObject("layout").getJSONArray("modes").getString(0)
    val digest get() = wire.getString("digest")
    companion object {
        fun decode(wire: JSONObject): KeyboardCombination {
            val id=wire.getJSONObject("choice").getJSONObject("layout").toString()
            val layout=wire.getJSONObject("layout")
            val mapping=wire.optJSONObject("mapping")
            val fallback=wire.getJSONObject("role_fallbacks")
            fun geometry(wide: Boolean)=KeyboardLayout.validated(id,layout,mapping,wide).let { value ->
                value.copy(keys=value.keys.map { it.copy(role=fallback.optString(it.role,it.role)) })
            }
            val portrait=geometry(false); val landscape=geometry(true)
            val appearance=wire.getJSONObject("skin")
            val bitmaps=mutableMapOf<String,android.graphics.Bitmap>()
            for(name in listOf("light","dark")) appearance.getJSONObject(name).let {
                it.put("decodedImages",decodeSkinImages(it,bitmaps))
            }
            return KeyboardCombination(wire,portrait,landscape,appearance)
        }
    }
}

internal fun keyboardRequest(path: File, action: String)=JSONObject().put("action",action).put("path",path.path)
internal fun keyboardCharset(path: File, config: JSONObject): String {
    val name=File(config.getJSONObject("dictionary").getString("system_table"))
    val table=if(name.isAbsolute) name else File(path.parentFile,name.path)
    return NativeBridge.objectCall("ReadTableConfig","path" to table.path).getString("charset")
}
internal fun keyboardCharset(path: File,config: JSONObject,mode: String): String =
    if(mode=="Pinyin") "abcdefghijklmnopqrstuvwxyz'" else keyboardCharset(path,config)
internal fun restoreKeyboard(path: File, config: JSONObject): KeyboardCombination? {
    val catalog=NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(path,"List"))
    val choice=catalog.optJSONObject("choice") ?: return null
    val selection=choice.getJSONObject("layout")
    val items=catalog.getJSONArray("items")
    val item=(0 until items.length()).map {items.getJSONObject(it)}.firstOrNull {
        val other=it.getJSONObject("selection")
        listOf("id","version","component").all {key->other.getString(key)==selection.getString(key)}
    }
    val mode=item?.optJSONArray("modes")?.optString(0) ?: "Main"
    val result=NativeBridge.call("ManageKeyboard","request" to keyboardRequest(path,"Restore")
        .put("mode",mode).put("charset",keyboardCharset(path,config,mode))) as? JSONObject
    return result?.let(KeyboardCombination::decode)
}

/** Input geometry cannot change under a finger or reinterpret a nonempty query. */
internal fun canPublishKeyboard(touches: Int, spelling: String, sameInput: Boolean): Boolean =
    touches == 0 && (spelling.isEmpty() || sameInput)
internal fun customKeyboardAllowed(purpose: String, layout: String, base: String, temporary: String?, mode: String = "Main"): Boolean =
    purpose == "Text" && layout == "letters" &&
        (if(mode=="Pinyin") temporary=="Pinyin" else base == "Main" && temporary == null)
