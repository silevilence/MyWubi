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
        if (editor != session || !value) { lastDisplay = ""; context = JSONObject() }
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
        fun read(block: () -> CharSequence?): Any = try {
            val text = block()?.toString()
            if (text == null) JSONObject.NULL else {
                var end = minOf(text.length,512)
                if(end>0 && text[end-1].isHighSurrogate())end--
                val start = if(end>0 && text[0].isLowSurrogate()) 1 else 0
                text.substring(start,end)
            }
        } catch (_: RuntimeException) { JSONObject.NULL }
        val next = JSONObject().put("revision", ++revision)
            .put("left", read { connection?.getTextBeforeCursor(512, 0) })
            .put("right", read { connection?.getTextAfterCursor(512, 0) })
            .put("selected", if(start >= 0 && start == end) "" else read { connection?.getSelectedText(0) })
            .put("selection", if(start >= 0 && end >= 0) JSONObject().put("start",start).put("end",end).put("unit","Utf16").put("absolute",true) else JSONObject.NULL)
            .put("truncated",true).put("discontinuity",false)
        context = next
        send(JSONObject().put("op","Context").put("context",next).put("external",external))
        return next
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
                val before = try { connection?.getTextBeforeCursor(512,0)?.toString() } catch (_:RuntimeException) { null }
                val left = context.opt("left") as? String
                if (left != null && before != null && text.isNotEmpty() && before.endsWith((left + text).takeLast(512))) confirmed = "Confirmed"
            }
        } else if (outcome == "RejectedBeforeWrite") confirmed = "Rejected"
        send(JSONObject().put("op","Operation").put("operation",operation).put("outcome",confirmed)
            .put("after",after ?: JSONObject.NULL).put("method",method))
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
    fun close() { allowed=false; send(JSONObject().put("op","Close")) }
}
