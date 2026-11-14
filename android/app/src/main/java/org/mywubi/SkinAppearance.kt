package org.mywubi

import java.io.File
import org.json.JSONObject

/** Both variants are resolved on the loader, never by a key event or recomposition. */
internal fun loadSkinAppearance(path: File, config: JSONObject): JSONObject {
    val result = JSONObject()
    for ((name, dark) in listOf("light" to false, "dark" to true)) {
        result.put(name, NativeBridge.objectCall("ResolveSkin", "path" to path.path,
            "config" to config, "platform" to "android", "dark" to dark, "high_contrast" to false))
    }
    return result
}
