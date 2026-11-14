package org.mywubi

import java.io.File
import org.json.JSONObject

/** Fully decoded before publication; the view never independently reloads its components. */
internal data class KeyboardCombination(val wire: JSONObject, val portrait: KeyboardLayout,
    val landscape: KeyboardLayout, val appearance: JSONObject) {
    val identity get() = wire.optJSONObject("mapping_identity")
    val definition get() = wire.optJSONObject("definition")
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
internal fun restoreKeyboard(path: File, config: JSONObject): KeyboardCombination? {
    val result=NativeBridge.call("ManageKeyboard","request" to keyboardRequest(path,"Restore")
        .put("mode","Main").put("charset",keyboardCharset(path,config))) as? JSONObject
    return result?.let(KeyboardCombination::decode)
}

/** Input geometry cannot change under a finger or reinterpret a nonempty query. */
internal fun canPublishKeyboard(touches: Int, spelling: String, sameInput: Boolean): Boolean =
    touches == 0 && (spelling.isEmpty() || sameInput)
internal fun customKeyboardAllowed(purpose: String, layout: String, base: String, temporary: String?): Boolean =
    purpose == "Text" && layout == "letters" && base == "Main" && temporary == null
