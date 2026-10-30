package org.mywubi

import org.json.JSONObject

/** One transport for session and resource operations, matching the public C facade. */
object NativeBridge {
    init { System.loadLibrary("mywubi_android"); check(version() == 1) }
    @JvmStatic external fun version(): Int
    @JvmStatic external fun exchange(request: ByteArray): ByteArray

    fun call(op: String, vararg fields: Pair<String, Any?>): Any? {
        val request = JSONObject().put("op", op)
        fields.forEach { (key, value) -> request.put(key, value ?: JSONObject.NULL) }
        val response = JSONObject(exchange(JSONObject().put("version", 1).put("request", request)
            .toString().toByteArray(Charsets.UTF_8)).toString(Charsets.UTF_8))
        if (!response.getBoolean("ok")) {
            val error = response.getJSONObject("error")
            throw ProtocolException(error.getInt("code"), error.getString("message"))
        }
        return response.opt("value").takeUnless { it == JSONObject.NULL }
    }

    fun objectCall(op: String, vararg fields: Pair<String, Any?>): JSONObject =
        call(op, *fields) as JSONObject
}

class ProtocolException(val code: Int, message: String) : RuntimeException(message)

/** An explicit owner; repeated close is safe without attempting a second native destroy. */
class NativeHandle(val value: Long) : AutoCloseable {
    private var closed = false
    override fun close() {
        if (!closed) { NativeBridge.call("Destroy", "handle" to value); closed = true }
    }
}
