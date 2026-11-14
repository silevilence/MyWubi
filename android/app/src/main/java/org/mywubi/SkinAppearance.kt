package org.mywubi

import java.io.File
import org.json.JSONObject
import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color

internal val LocalSkinParameters = staticCompositionLocalOf<JSONObject?> { null }

@Composable
internal fun skinColor(key: String, fallback: Color): Color = skinColorValue(LocalSkinParameters.current, key, fallback)

internal fun skinColorValue(parameters: JSONObject?, key: String, fallback: Color): Color {
    val color = parameters?.optString(key)?.takeIf { it.startsWith("#") } ?: return fallback
    val raw = color.substring(1).toLongOrNull(16) ?: return fallback
    val argb = if (color.length == 9) ((raw and 255L) shl 24) or (raw ushr 8) else 0xFF000000L or raw
    return Color(argb)
}

@Composable
internal fun skinNumber(key: String, fallback: Float): Float = LocalSkinParameters.current?.optDouble(key, fallback.toDouble())?.toFloat() ?: fallback

/** Both variants are resolved on the loader, never by a key event or recomposition. */
internal fun loadSkinAppearance(path: File, config: JSONObject): JSONObject {
    val result = JSONObject()
    for ((name, dark) in listOf("light" to false, "dark" to true)) {
        result.put(name, NativeBridge.objectCall("ResolveSkin", "path" to path.path,
            "config" to config, "platform" to "android", "dark" to dark, "high_contrast" to false))
    }
    return result
}
