package org.mywubi

import java.io.File
import org.json.JSONObject

data class KeyboardSelection(val id: String, val version: String, val component: String) {
    fun json(): JSONObject = JSONObject().put("id", id).put("version", version).put("component", component)
    fun samePackage(other: KeyboardSelection): Boolean = id == other.id && version == other.version

    companion object {
        fun decode(value: JSONObject) = KeyboardSelection(value.getString("id"), value.getString("version"), value.getString("component"))
    }
}

data class MainKeyboardOption(val selection: KeyboardSelection, val name: String, val reason: String? = null)
internal data class SwitchMainKeyboard(val selection: KeyboardSelection)

/** An appearance-only skin keeps the separately configured keyboard package. */
internal fun mainKeyboardFamily(options: List<MainKeyboardOption>, skin: KeyboardSelection?,
                                current: KeyboardSelection?): List<MainKeyboardOption> {
    val family = skin?.takeIf { candidate -> options.any { it.selection.samePackage(candidate) } } ?: current
    return if (family == null) emptyList() else options.filter { it.selection.samePackage(family) }
}

internal fun mainKeyboardChoices(path: File, config: JSONObject): List<MainKeyboardOption> {
    val result = NativeBridge.objectCall("ManageKeyboard", "request" to keyboardRequest(path, "List").put("mode", "Main"))
    val items = result.getJSONArray("items")
    val options = (0 until items.length()).mapNotNull { index ->
        val item = items.getJSONObject(index)
        val modes = item.optJSONArray("modes") ?: return@mapNotNull null
        if ((0 until modes.length()).none { modes.getString(it) == "Main" }) return@mapNotNull null
        MainKeyboardOption(KeyboardSelection.decode(item.getJSONObject("selection")), item.getString("name"),
            if (item.getBoolean("enabled")) null else item.optString("reason", "布局不可用"))
    }
    val skin = config.getJSONObject("skins").getJSONObject("android").optJSONObject("selection")?.let(KeyboardSelection::decode)
    val current = result.optJSONObject("choice")?.getJSONObject("layout")?.let(KeyboardSelection::decode)
    return mainKeyboardFamily(options, skin, current)
}

internal fun KeyboardCombination.selection(): KeyboardSelection =
    KeyboardSelection.decode(wire.getJSONObject("choice").getJSONObject("layout"))
