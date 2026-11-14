package org.mywubi

import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File

class GroupProtocolTest {
    @Test fun groupsMatchSharedRustFixturesAndNeverBecomeHostText() {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val fixtures=JSONArray(context.assets.open("protocol/groups.json").bufferedReader().use {it.readText()})
        val root=File(context.cacheDir,"group-protocol").apply {mkdirs()}
        File(root,"main.dict").writeText("j\t甲\nkj\t乙\n")
        val config=File(root,"config.toml").apply {writeText("[basic]\nauto_commit_unique=false\n[dictionary]\nsystem_table=\"main.dict\"\nuser_table=\"user.dict\"\n")}
        assertEquals(32,NativeBridge.objectCall("KeyboardCapabilities").getInt("max_positions"))
        NativeHandle(NativeBridge.objectCall("BuildRuntime","path" to config.path).getLong("handle")).use { runtime ->
            val host=JSONObject("""{"editor":3,"epoch":1,"supports_preedit":true,"supports_insert":true,"owns_preedit":false,"purpose":"Text","selection_available":true,"surrounding_available":true,"enter_action":"Newline"}""")
            NativeHandle(NativeBridge.objectCall("CreateSession","runtime" to runtime.value,"context" to host).getLong("handle")).use { session ->
                for(index in 0 until fixtures.length()) {
                    val fixture=fixtures.getJSONObject(index)
                    val plan=NativeBridge.objectCall("Prepare","session" to session.value,"action" to fixture.get("action"))
                    assertEquals("NoOp",plan.getString("op"))
                    NativeBridge.call("Ack","session" to session.value,"token" to plan.getJSONObject("token"),"outcome" to "Applied","owns_preedit" to false)
                    val state=NativeBridge.objectCall("State","session" to session.value)
                    assertEquals(fixture.getString("hint"),state.getJSONObject("view").getString("spelling"))
                    assertEquals(fixture.getInt("positions"),state.optJSONObject("groups")?.getJSONArray("positions")?.length() ?: 0)
                }
            }
        }
    }
}
