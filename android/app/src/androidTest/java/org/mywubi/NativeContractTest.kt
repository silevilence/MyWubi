package org.mywubi

import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File

class NativeContractTest {
    @Test fun protocolBoundariesAndLifecycle() {
        assertEquals(1, NativeBridge.version())
        val fixtureText = InstrumentationRegistry.getInstrumentation().targetContext.assets.open("protocol/errors.json").bufferedReader().use { it.readText() }
        val fixtures = JSONArray(fixtureText)
        for (i in 0 until fixtures.length()) {
            val f = fixtures.getJSONObject(i)
            val result = JSONObject(NativeBridge.exchange(f.getJSONObject("request").toString().toByteArray(Charsets.UTF_8)).toString(Charsets.UTF_8))
            assertEquals(f.getInt("code"), result.getJSONObject("error").getInt("code"))
        }
        for (bytes in listOf(byteArrayOf(0xff.toByte()), byteArrayOf(),
            "{\"version\":2,\"request\":{\"op\":\"Destroy\",\"handle\":0}}".toByteArray())) {
            assertFalse(JSONObject(NativeBridge.exchange(bytes).toString(Charsets.UTF_8)).getBoolean("ok"))
        }
        val root = File(InstrumentationRegistry.getInstrumentation().targetContext.cacheDir, "contract").apply { mkdirs() }
        File(root,"main.dict").writeText("a\t一\t100\nab\t丁\t80\n")
        val config = File(root,"config.toml").apply {
            writeText("[dictionary]\nsystem_table = \"main.dict\"\nuser_table = \"user.dict\"\n")
        }
        NativeHandle(NativeBridge.objectCall("BuildRuntime", "path" to config.path).getLong("handle")).use { runtime ->
            val context = JSONObject("""{"editor":1,"epoch":1,"supports_preedit":true,"supports_insert":true,"owns_preedit":false,"purpose":"Text","selection_available":true,"surrounding_available":true,"enter_action":"Newline"}""")
            val session = NativeHandle(NativeBridge.objectCall("CreateSession", "runtime" to runtime.value, "context" to context).getLong("handle"))
            session.use {
                val text = "😀\u0000e\u0301"
                val plan = NativeBridge.objectCall("Prepare", "session" to it.value, "action" to JSONObject().put("InsertLiteral",text))
                assertEquals(text, plan.getJSONObject("op").getJSONObject("CommitAndReplace").getString("text"))
                assertEquals("Published",NativeBridge.objectCall("Ack","session" to it.value,"token" to plan.getJSONObject("token"),"outcome" to "Applied","owns_preedit" to false).getString("status"))
                val p = NativeBridge.objectCall("Prepare","session" to it.value,"action" to JSONObject().put("InputText",JSONArray().put("a").put("Normal")))
                NativeBridge.call("Ack","session" to it.value,"token" to p.getJSONObject("token"),"outcome" to "Applied","owns_preedit" to true)
                assertEquals("一",NativeBridge.objectCall("Candidates","session" to it.value,"offset" to 0,"count" to 32).getJSONArray("candidates").getJSONObject(0).getString("text"))
                NativeBridge.call("Close","session" to it.value)
            }
            try { NativeBridge.call("Destroy","handle" to session.value); fail("stale handle accepted") }
            catch (e: ProtocolException) { assertEquals(3,e.code) }
        }
    }
}
