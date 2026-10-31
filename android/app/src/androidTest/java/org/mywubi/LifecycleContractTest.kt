package org.mywubi

import android.view.inputmethod.InputConnection
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class LifecycleContractTest {
    @Test fun localPreeditBusyAndUnknownCommitRecoverWithoutReplay() {
        val context = androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().targetContext
        val config = Resources.prepare(context)
        NativeHandle(NativeBridge.objectCall("BuildRuntime", "path" to config.path).getLong("handle")).use { runtime ->
            val host = JSONObject("""{"editor":1,"epoch":1,"supports_preedit":false,"supports_insert":true,"owns_preedit":false,"purpose":"Text","selection_available":true,"surrounding_available":true,"enter_action":"Newline"}""")
            NativeHandle(NativeBridge.objectCall("CreateSession", "runtime" to runtime.value, "context" to host).getLong("handle")).use { session ->
                fun prepare(action: Any) = NativeBridge.objectCall("Prepare", "session" to session.value, "action" to action)
                fun ack(plan: JSONObject, outcome: String) = NativeBridge.objectCall("Ack", "session" to session.value, "token" to plan.getJSONObject("token"), "outcome" to outcome, "owns_preedit" to false)
                val first = prepare(JSONObject().put("InputCode", "g"))
                assertEquals("NoOp", first.get("op"))
                try { prepare("Space"); fail("Busy must be explicit") } catch (e: ProtocolException) { assertEquals(4, e.code) }
                ack(first, "Applied")
                val broken = java.io.File(context.cacheDir, "broken-runtime.toml").apply { writeText("[dictionary]\nsystem_table=\"missing.dict\"\n") }
                try { NativeBridge.call("BuildRuntime", "path" to broken.path); fail("bad reload accepted") }
                catch(e: ProtocolException) { assertEquals(5, e.code) }
                assertTrue(NativeBridge.objectCall("Candidates", "session" to session.value, "offset" to 0, "count" to 8).getInt("total") > 0)
                val commit = prepare("Space")
                assertTrue(commit.getJSONObject("op").has("CommitAndReplace"))
                ack(commit, "OutcomeUnknown")
                assertNull(NativeBridge.call("Recover", "session" to session.value))
                assertEquals("Ready", NativeBridge.objectCall("State", "session" to session.value).getString("status"))
                val pending = prepare(JSONObject().put("InputCode", "g"))
                NativeBridge.call("Context", "session" to session.value, "context" to host.put("epoch", 2), "preserve_base" to true)
                assertEquals("Stale", ack(pending, "Applied").getString("status"))
            }
        }
    }
    @Test fun lateBuildAndDestroyedOwnerCannotPublish() {
        val loads = LoadGeneration()
        val old = loads.next()
        val latest = loads.next()
        assertFalse(loads.accepts(old))
        assertTrue(loads.accepts(latest))
        loads.next()
        assertFalse(loads.accepts(latest))
    }
    private fun host(call: (String) -> Boolean) = java.lang.reflect.Proxy.newProxyInstance(
        InputConnection::class.java.classLoader, arrayOf(InputConnection::class.java)
    ) { _, method, _ -> call(method.name) } as InputConnection

    @Test fun contextSwitchDuringCommitCancelsRemainingSteps() {
        var current = true
        val writes = mutableListOf<String>()
        val host = host { name ->
            if (name == "commitText") { writes.add(name); current = false }
            if (name == "setComposingText") writes.add(name)
            true
        }
        val adapter = EditorAdapter()
        val plan = JSONObject("""{"target":"CurrentSelection","op":{"CommitThenUpdate":{"text":"王","spelling":"g"}}}""")
        assertEquals("StaleContext", adapter.execute(host, plan) { current })
        assertEquals(listOf("commitText"), writes)
        assertFalse(adapter.ownsPreedit)
    }

    @Test fun unknownWriteCannotRetainAnOldComposition() {
        var fail = false
        val host = host { name -> !(fail && name == "commitText") }
        val adapter = EditorAdapter()
        adapter.execute(host, JSONObject("""{"target":"CurrentSelection","op":{"CompositionUpdate":{"spelling":"g"}}}"""))
        assertTrue(adapter.ownsPreedit)
        fail = true
        assertEquals("OutcomeUnknown", adapter.execute(host, JSONObject("""{"target":"OwnedComposition","op":{"CommitAndReplace":{"text":"王"}}}""")))
        assertFalse(adapter.ownsPreedit)
    }

    @Test fun reentrantKeysRemainOrderedAndControlEventsInterrupt() {
        val queue = InputQueue()
        val keys = mutableListOf<Int>()
        queue.submit {
            keys.add(1)
            queue.submit { keys.add(3) }
            keys.add(2)
        }
        assertEquals(listOf(1, 2, 3), keys)
        queue.submit {
            queue.submit { keys.add(4) }
            queue.invalidate()
            queue.submit { keys.add(5) }
        }
        assertEquals(listOf(1, 2, 3, 5), keys)
    }

    @Test fun overflowIsExplicitAndDoesNotReplaceQueuedKeys() {
        val queue = InputQueue(1)
        var count = 0
        queue.submit {
            assertTrue(queue.submit { count++ })
            assertFalse(queue.submit { count += 100 })
        }
        assertEquals(1, count)
    }
}
