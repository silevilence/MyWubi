package org.mywubi

import android.content.Context
import android.net.Uri
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/** SAF grants are only needed while copying; the validated import lives with config. */
class ExpressionCatalog(private val context: Context) {
    fun list(config: JSONObject): Map<String, List<JSONObject>> {
        val path = Resources.prepare(context)
        return listOf("emoji", "kaomoji", "kana", "math", "punctuation", "symbols").associateWith { kind ->
            val selected = config.optJSONObject("expressions")?.optJSONObject(kind) ?: JSONObject()
            val rows = NativeBridge.call("ListExpressions", "path" to path.path, "kind" to kind,
                "selected" to selected) as JSONArray
            (0 until rows.length()).map(rows::getJSONObject)
        }
    }

    fun import(kind: String, uri: Uri): JSONObject {
        require(kind in listOf("emoji", "kaomoji", "kana", "math", "punctuation", "symbols"))
        val staged = File.createTempFile("expression-", ".mwe", context.cacheDir)
        try {
            context.contentResolver.openInputStream(uri)?.use { input ->
                staged.outputStream().use { output ->
                    val bytes = ByteArray(8192)
                    var total = 0L
                    while (true) {
                        val count = input.read(bytes)
                        if (count < 0) break
                        total += count
                        require(total <= 64L * 1024 * 1024) { "表情词库超过 64 MiB" }
                        output.write(bytes, 0, count)
                    }
                    output.fd.sync()
                }
            } ?: error("无法读取表情词库")
            return NativeBridge.objectCall("ImportExpression", "path" to Resources.prepare(context).path,
                "kind" to kind, "source" to staged.path)
        } finally { staged.delete() }
    }
}
