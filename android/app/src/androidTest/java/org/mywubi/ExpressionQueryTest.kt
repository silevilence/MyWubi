package org.mywubi

import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.util.UUID

/** JNI contract test; deliberately does not claim keyboard layout or host rendering coverage. */
class ExpressionQueryTest {
    @Test fun multiwordQueryStaysLocalAndOnlyTheCompleteExpressionCrossesJni() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val root = File(context.cacheDir, "query-${UUID.randomUUID()}").apply { mkdirs() }
        File(root, "main.dict").writeText("a\t禁止\nb\t自行车\nc\t开心\nd\t车\n")
        context.assets.open("expressions/emoji.mwe").use { input -> File(root, "emoji.mwe").outputStream().use(input::copyTo) }
        val config = File(root, "config.toml").apply {
            writeText("""[basic]
auto_commit_unique = false
[dictionary]
system_table = "main.dict"
user_table = "user.dict"
[expressions.emoji]
append = true
search = true
resource = "emoji.mwe"
""")
        }
        NativeHandle(NativeBridge.objectCall("BuildRuntime", "path" to config.path).getLong("handle")).use { runtime ->
            val host = JSONObject("""{"editor":9,"epoch":1,"supports_preedit":true,"supports_insert":true,"owns_preedit":false,"purpose":"Text","selection_available":true,"surrounding_available":true,"enter_action":"Send"}""")
            NativeHandle(NativeBridge.objectCall("CreateSession", "runtime" to runtime.value, "context" to host).getLong("handle")).use { session ->
                fun prepare(action: Any) = NativeBridge.objectCall("Prepare", "session" to session.value, "action" to action)
                fun ack(plan: JSONObject, outcome: String = "Applied") = NativeBridge.objectCall("Ack", "session" to session.value, "token" to plan.getJSONObject("token"), "outcome" to outcome, "owns_preedit" to false)
                fun local(action: Any) { val plan = prepare(action); assertEquals("NoOp", plan.getString("op")); ack(plan) }
                fun state() = NativeBridge.objectCall("State", "session" to session.value)
                assertTrue(state().getJSONObject("emoji").getBoolean("available"))
                local(JSONObject().put("BeginTemporary", "Emoji"))
                local(JSONObject().put("InputCode", "a")); local("Space")
                local(JSONObject().put("InputCode", "b")); local("FindExpressions")
                val query = state().getJSONObject("view").getJSONObject("expression")
                assertEquals("禁止自行车", query.getString("text"))
                assertEquals("Results", query.getString("phase"))
                assertEquals(2, query.getJSONArray("words").length())
                val results = NativeBridge.objectCall("Candidates", "session" to session.value, "offset" to 0, "count" to 256)
                val candidates = results.getJSONArray("candidates")
                val bicycle = (0 until candidates.length()).map(candidates::getJSONObject).first { it.getString("text") == "🚳" }
                val action = JSONObject().put("SelectCandidate", JSONArray().put(results.getJSONObject("query_revision")).put(bicycle.getInt("id")))
                val first = prepare(action)
                assertEquals("🚳", first.getJSONObject("op").getJSONObject("CommitAndReplace").getString("text"))
                assertEquals("Retained", ack(first, "RejectedBeforeWrite").getString("status"))
                val retry = prepare(action)
                assertEquals(first.getJSONObject("op").toString(), retry.getJSONObject("op").toString())
                ack(retry)
                assertTrue(state().getJSONObject("view").isNull("expression"))
                try { prepare(action); fail("stale result accepted") } catch (e: ProtocolException) { assertEquals(7, e.code) }
                local(JSONObject().put("BeginTemporary", "Emoji"))
                local(JSONObject().put("InputCode", "a")); local("Space")
                local(JSONObject().put("InputCode", "d")); local("FindExpressions")
                assertEquals("禁止车", state().getJSONObject("view").getJSONObject("expression").getString("text"))
                val fuzzy = NativeBridge.objectCall("Candidates", "session" to session.value, "offset" to 0, "count" to 256)
                val found = fuzzy.getJSONArray("candidates")
                val match = (0 until found.length()).map(found::getJSONObject).first { it.getString("text") == "🚳" }
                assertTrue(match.getJSONArray("hints").toString().contains("ExpressionMatch"))
                val chosen = prepare(JSONObject().put("SelectCandidate", JSONArray().put(fuzzy.getJSONObject("query_revision")).put(match.getInt("id"))))
                assertEquals("🚳", chosen.getJSONObject("op").getJSONObject("CommitAndReplace").getString("text"))
                ack(chosen)
            }
        }
    }
}
