package org.mywubi

import android.view.inputmethod.InputConnection
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/** Captures bounded observations only after explicit consent and editor gating. */
class PersonalCapture {
    private var settings = JSONObject()
    private var config: File? = null
    private var allowed = false
    private var session = ""
    private var revision = 0L
    private var lastDisplay = ""
    private var context = JSONObject()
    private var modelId: String? = null
    var method = "touch_or_key"
    val enabled get() = allowed && settings.optBoolean("collect")
    fun configure(file: File, personal: JSONObject, prediction:JSONObject = JSONObject()) { config = file; settings = personal; modelId=prediction.optJSONObject("model")?.let{it.optString("id")+":"+it.optString("version")}; gate(allowed, session) }
    fun gate(value: Boolean, editor: String) {
        if (editor != session || !value || !settings.optBoolean("collect")) { lastDisplay = ""; context = JSONObject() }
        allowed = value; session = editor
        val file = config ?: return
        send(JSONObject().put("op", "Configure").put("config", file.absolutePath)
            .put("settings", settings).put("allowed", allowed).put("session", editor.ifEmpty { "inactive" }))
    }
    private fun send(request: JSONObject) {
        try { NativeBridge.personalCapture(request.toString().toByteArray(Charsets.UTF_8)) } catch (_: RuntimeException) { }
    }
    fun observe(connection: InputConnection?, start: Int, end: Int, external: Boolean = false): JSONObject? {
        if (!enabled) return null
        val next = readContext(connection, start, end)
        context = next
        send(JSONObject().put("op","Context").put("context",next).put("external",external))
        return next
    }
    fun readContext(connection: InputConnection?, start: Int, end: Int): JSONObject {
        fun read(tail: Boolean = false, block: () -> CharSequence?): Any {
            val raw = try { block() } catch (_: RuntimeException) { null } ?: return JSONObject.NULL
            val bounded = if (tail) raw.takeLast(512) else raw.take(512)
            var first = 0
            var last = bounded.length
            if (first < last && bounded[first].isLowSurrogate()) first++
            if (first < last && bounded[last - 1].isHighSurrogate()) last--
            return bounded.subSequence(first, last).toString()
        }
        return JSONObject().put("revision", ++revision)
            .put("left", read(true) { connection?.getTextBeforeCursor(512, 0) })
            .put("right", read { connection?.getTextAfterCursor(512, 0) })
            .put("selected", if (start >= 0 && start == end) "" else read { connection?.getSelectedText(0) })
            .put("selection", if (start >= 0 && end >= 0) JSONObject().put("start", start)
                .put("end", end).put("unit", "Utf16").put("absolute", true) else JSONObject.NULL)
            .put("truncated", true).put("discontinuity", false)
    }
    fun operation(connection: InputConnection?, operation: JSONObject?, outcome: Any, start: Int, end: Int) {
        if (!enabled || operation == null) return
        val commit = operation.optJSONObject("CommitAndReplace") ?: operation.optJSONObject("CommitThenUpdate")
        var confirmed = "Unknown"
        var after: JSONObject? = null
        if (outcome == "Applied") {
            confirmed = "CommandAccepted"
            // Boolean acceptance alone does not prove the remote text. Compare
            // bounded readback only for completed commits without a new preedit.
            if (commit != null && operation.has("CommitAndReplace")) {
                val text = commit.optString("text")
                val observed = readContext(connection, start, end)
                val left = context.opt("left") as? String
                val right = context.opt("right") as? String
                val expected = left?.let { (it + text).takeLast(512).dropWhile(Char::isLowSurrogate) }
                if (text.isNotEmpty() && expected != null && right != null &&
                    observed.opt("left") == expected && observed.opt("right") == right) {
                    confirmed = "Confirmed"
                    after = observed
                }
            }
        } else if (outcome == "RejectedBeforeWrite") confirmed = "Rejected"
        send(JSONObject().put("op","Operation").put("operation",operation).put("outcome",confirmed)
            .put("after",after ?: JSONObject.NULL).put("method",method))
        if (after != null) context = after
    }
    fun nonLearningCommit(outcome: Any) {
        if(!enabled || outcome=="RejectedBeforeWrite")return
        lastDisplay="";context=JSONObject()
        send(JSONObject().put("op","NonLearningCommit").put("outcome",if(outcome=="Applied")"CommandAccepted" else "Unknown"))
    }
    fun display(state: JSONObject, window: JSONObject, personalVersion: String? = null) {
        if (!enabled) return
        val list = window.optJSONArray("candidates") ?: return
        val source = if(state.optJSONObject("view")?.optBoolean("prediction") == true) {
            if(settings.optBoolean("predict")) "PersonalPrediction" else if(modelId!=null) "ModelPrediction" else "DictionaryPrediction"
        } else "Code"
        val candidates = JSONArray()
        for(i in 0 until list.length()) {
            val item = list.getJSONObject(i)
            val hints=item.optJSONArray("hints") ?: JSONArray()
            if((0 until hints.length()).any {hints.optJSONObject(it)?.has("DateTime")==true})continue
            candidates.put(JSONObject().put("id",item.get("id").toString()).put("text",item.getString("text")).put("source",source))
        }
        val display = JSONObject().put("request",window.optJSONObject("query_revision")?.toString() ?: "unknown")
            .put("model",(if(source=="PersonalPrediction")personalVersion else modelId) ?: JSONObject.NULL).put("policy","v1").put("candidates",candidates)
        if(display.toString() == lastDisplay) return
        lastDisplay = display.toString()
        send(JSONObject().put("op","Display").put("mode",state.optJSONObject("view")?.optString("mode") ?: "android").put("display",display))
    }
    fun close() { allowed=false; context=JSONObject(); lastDisplay=""; send(JSONObject().put("op","Close")) }
}
