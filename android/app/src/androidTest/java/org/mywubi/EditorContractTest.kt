package org.mywubi

import android.text.InputType
import android.view.inputmethod.EditorInfo
import org.junit.Assert.*
import org.junit.Test

class EditorContractTest {
    @Test fun clearSelectsWholeEditorAndStopsWhenSelectionFailsOrContextChanges() {
        for (mode in listOf("ok", "reject", "stale")) {
            val calls = mutableListOf<String>()
            var current = true
            val connection = java.lang.reflect.Proxy.newProxyInstance(
                android.view.inputmethod.InputConnection::class.java.classLoader,
                arrayOf(android.view.inputmethod.InputConnection::class.java)
            ) { _, method, args ->
                calls.add(method.name)
                if (method.name == "performContextMenuAction") {
                    assertEquals(android.R.id.selectAll, args!![0])
                    if (mode == "stale") current = false
                    mode != "reject"
                } else {
                    if (method.name == "commitText") assertEquals("", args!![0])
                    true
                }
            } as android.view.inputmethod.InputConnection
            assertEquals(mode == "ok", EditorShortcuts.clear(connection) { current })
            assertEquals(mode == "ok", calls.contains("commitText"))
            assertEquals("endBatchEdit", calls.last())
        }
    }

    @Test fun pasteCommitsLiteralTextExactlyOnceWithoutClipboardSideEffects() {
        val values = mutableListOf<String>()
        val connection = java.lang.reflect.Proxy.newProxyInstance(
            android.view.inputmethod.InputConnection::class.java.classLoader,
            arrayOf(android.view.inputmethod.InputConnection::class.java)
        ) { _, method, args ->
            assertNotEquals("performContextMenuAction", method.name)
            if (method.name == "commitText") values.add(args!![0].toString())
            true
        } as android.view.inputmethod.InputConnection
        val text = "beancount 今日确认\n😀"
        assertTrue(EditorShortcuts.paste(connection, text) { true })
        assertEquals(listOf(text), values)
        assertFalse(EditorShortcuts.paste(connection, "stale") { false })
        assertEquals(1, values.size)
    }
    @Test fun clearingCompositionReleasesOwnershipEvenWhenFinishFails() {
        val connection = java.lang.reflect.Proxy.newProxyInstance(
            android.view.inputmethod.InputConnection::class.java.classLoader,
            arrayOf(android.view.inputmethod.InputConnection::class.java)
        ) { _, method, _ -> method.name != "finishComposingText" } as android.view.inputmethod.InputConnection
        val adapter=EditorAdapter()
        adapter.execute(connection,org.json.JSONObject("""{"target":"CurrentSelection","op":{"CompositionUpdate":{"spelling":"a"}}}"""))
        assertTrue(adapter.ownsPreedit)
        val result=adapter.execute(connection,org.json.JSONObject("""{"target":"OwnedComposition","op":{"EndComposition":{"delete_text":true}}}""")) as org.json.JSONObject
        assertEquals(1,result.getJSONObject("PartiallyApplied").getInt("completed_steps"))
        assertFalse(adapter.ownsPreedit)
    }

    @Test fun batchBoundaryFailuresRemainUnknownWithoutRetry() {
        for (boundary in listOf("beginBatchEdit", "endBatchEdit")) {
            var commits = 0
            val connection = java.lang.reflect.Proxy.newProxyInstance(
                android.view.inputmethod.InputConnection::class.java.classLoader,
                arrayOf(android.view.inputmethod.InputConnection::class.java)
            ) { _, method, _ ->
                if (method.name == boundary) throw IllegalStateException("disconnected")
                if (method.name == "commitText") commits++
                true
            } as android.view.inputmethod.InputConnection
            val plan = org.json.JSONObject("""{"target":"CurrentSelection","op":{"CommitAndReplace":{"text":"测试","replace_len":0}}}""")
            assertEquals("OutcomeUnknown", EditorAdapter().execute(connection, plan))
            assertEquals(if(boundary == "beginBatchEdit") 0 else 1, commits)
        }
    }

    @Test fun unicodeDeletionUsesWholeGraphemes() {
        for (text in listOf("汉", "😀", "e\u0301", "👩‍💻", "🇨🇳", "👍🏽"))
            assertEquals(text, text.length, UnicodeEditor.deletionLength("x$text"))
        assertEquals(0, UnicodeEditor.deletionLength(""))
        assertEquals(0, UnicodeEditor.deletionLength("\uD83D"))
    }
    @Test fun inputPurposeAndActionAreIndependentOfBaseMode() {
        assertEquals("Password",InputPolicy.from(InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD,EditorInfo.IME_ACTION_DONE).purpose)
        assertEquals("Password",InputPolicy.from(InputType.TYPE_CLASS_NUMBER or InputType.TYPE_NUMBER_VARIATION_PASSWORD,0).purpose)
        assertEquals("phone",InputPolicy.from(InputType.TYPE_CLASS_PHONE,0).layout)
        assertEquals("email",InputPolicy.from(InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS,0).layout)
        assertEquals("Send",InputPolicy.from(InputType.TYPE_CLASS_TEXT,EditorInfo.IME_ACTION_SEND).action)
        assertEquals("Newline",InputPolicy.from(InputType.TYPE_CLASS_TEXT,EditorInfo.IME_ACTION_SEND or EditorInfo.IME_FLAG_NO_ENTER_ACTION).action)
    }
}
