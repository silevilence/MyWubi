package org.mywubi

import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.util.UUID

class LibraryContractTest {
    @Test fun userCrudConflictImportAndExportUseSharedRules() {
        val root = File(InstrumentationRegistry.getInstrumentation().targetContext.cacheDir, "user-${UUID.randomUUID()}").apply { mkdirs() }
        val file = File(root, "user.dict")
        var version = NativeHandle(NativeBridge.objectCall("ReadUser", "path" to file.path).getLong("version"))
        fun edit(action: JSONObject) {
            val result = NativeBridge.objectCall("EditUser", "path" to file.path, "expected" to version.value, "edit" to action)
            version.close(); version = NativeHandle(result.getLong("version"))
        }
        try {
            val first = JSONObject("""{"code":"ab","word":"甲😀","weight":100}""")
            edit(JSONObject().put("Add", first))
            val original = file.readText()
            try { edit(JSONObject().put("Add", JSONObject("""{"code":"!","word":"非法","weight":1}"""))); fail("invalid code accepted") }
            catch(e: ProtocolException) { assertEquals(5, e.code) }
            assertEquals(original, file.readText())
            val id = JSONObject().put("code", "ab").put("word", "甲😀")
            edit(JSONObject().put("Update", JSONArray().put(id).put(JSONObject("""{"code":"ab","word":"甲😀","weight":999}"""))))
            val imported = File(root, "import.dict").apply { writeText("ab\t甲😀\t1200\ncd\t乙\t50\n") }
            edit(JSONObject().put("Import", imported.path))
            val read = NativeBridge.objectCall("ReadUser", "path" to file.path)
            NativeHandle(read.getLong("version")).close()
            assertEquals(2, read.getJSONArray("entries").length())
            val exported = File(root, "export.dict")
            NativeHandle(NativeBridge.objectCall("FileVersion", "path" to exported.path).getLong("version")).use { target ->
                val result = NativeBridge.objectCall("ExportUser", "source" to file.path, "target" to exported.path, "expected" to target.value)
                NativeHandle(result.getLong("version")).close()
            }
            assertTrue(exported.readText().contains("1200"))
            edit(JSONObject().put("Remove", id))
            file.appendText("ef\t外部\t10\n")
            try { edit(JSONObject().put("Add", first)); fail("conflicting write accepted") }
            catch(e: ProtocolException) { assertEquals(6, e.code) }
            assertTrue(file.readText().contains("外部"))
        } finally { version.close() }
    }
}
