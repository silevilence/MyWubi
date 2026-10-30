package org.mywubi

import android.view.inputmethod.InputConnection
import org.json.JSONObject

/** Applies only an already validated plan to its captured connection. */
class EditorAdapter {
    var ownsPreedit = false
        private set
    fun abandon() { ownsPreedit = false }

    fun execute(connection: InputConnection, plan: JSONObject): Any {
        val op = plan.get("op")
        if (op == "NoOp") return "Applied"
        if (plan.getString("target") == "LegacyRangeFallback") return "RejectedBeforeWrite"
        if (plan.getString("target") == "OwnedComposition" && !ownsPreedit) return "StaleContext"
        val operation = op as JSONObject
        var completed = 0
        connection.beginBatchEdit()
        return try {
            fun step(result: Boolean): Boolean { if (result) completed++; return result }
            val success = when {
                operation.has("CompositionUpdate") -> {
                    val p = operation.getJSONObject("CompositionUpdate")
                    step(connection.setComposingText(p.getString("spelling"), 1)).also { if (it) ownsPreedit = true }
                }
                operation.has("CommitAndReplace") -> {
                    val p = operation.getJSONObject("CommitAndReplace")
                    step(connection.commitText(p.getString("text"), 1)).also { if (it) ownsPreedit = false }
                }
                operation.has("CommitThenUpdate") -> {
                    val p = operation.getJSONObject("CommitThenUpdate")
                    if (!step(connection.commitText(p.getString("text"), 1))) false
                    else {
                        ownsPreedit = false
                        step(connection.setComposingText(p.getString("spelling"), 1)).also { if (it) ownsPreedit = true }
                    }
                }
                operation.has("EndComposition") -> {
                    val delete = operation.getJSONObject("EndComposition").getBoolean("delete_text")
                    if (!ownsPreedit) true
                    else if (delete && !step(connection.commitText("", 1))) false
                    else step(connection.finishComposingText()).also { if (it) ownsPreedit = false }
                }
                else -> false
            }
            if (success) "Applied" else if (completed > 0)
                JSONObject().put("PartiallyApplied", JSONObject().put("completed_steps", completed))
            else "OutcomeUnknown"
        } catch (_: RuntimeException) {
            if (completed > 0) JSONObject().put("PartiallyApplied", JSONObject().put("completed_steps", completed)) else "OutcomeUnknown"
        } finally { connection.endBatchEdit() }
    }
}
