package org.mywubi

import android.content.Context
import android.content.ContextWrapper
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.util.UUID

class ResourceContractTest {
    private fun isolated(): Context {
        val target = InstrumentationRegistry.getInstrumentation().targetContext
        val root = File(target.cacheDir, "resources-${UUID.randomUUID()}").apply { mkdirs() }
        return object : ContextWrapper(target) {
            override fun getFilesDir() = root
            override fun getSharedPreferences(name: String, mode: Int) = target.getSharedPreferences("${root.name}-$name", mode)
        }
    }

    private fun pack(store: EngineStore, broken: Boolean = false): EngineStore.ImportSet {
        val root = File(store.configFile.parentFile, "packages/${UUID.randomUUID()}").apply { mkdirs() }
        val main = File(root, "0.dict").apply { writeText("---\ntemporary:\n  pinyin_table: 'C:\\old\\pinyin.dict'\n---\na\t甲\t10\n") }
        val auxiliary = File(root, "1.dict").apply { writeText(if (broken) "---\ntype: pinyin\n---\nINVALID\n" else "---\ntype: pinyin\nmax_code_len: 64\n---\nni\t你\t10\n") }
        return EngineStore.ImportSet(root, listOf(EngineStore.ImportFile("same.dict", main), EngineStore.ImportFile("same.dict", auxiliary)))
    }

    @Test fun activationInterruptionKeepsOldOrCompleteNewGeneration() {
        val store = EngineStore(isolated())
        val original = store.configFile.readText()
        val imported = pack(store)
        for (point in listOf("before_activation", "after_activation")) {
            val read = store.readConfig()
            NativeHandle(read.getLong("version")).use { version ->
                try {
                    store.activate(imported, 0, 1, null, version.value) { if (it == point) error("simulated termination") }
                    fail("checkpoint not reached")
                } catch (e: IllegalStateException) { assertEquals("simulated termination", e.message) }
            }
            if (point == "before_activation") assertEquals(original, store.configFile.readText())
            else assertTrue(store.configFile.readText().contains(imported.files[0].file.path))
            NativeHandle(NativeBridge.objectCall("BuildRuntime", "path" to store.configFile.path).getLong("handle")).close()
            assertTrue(File(store.configFile.parentFile, "tables/wubi86.dict").exists())
        }
    }

    @Test fun badReferencesAndConflictsPreserveActiveConfig() {
        val store = EngineStore(isolated())
        val original = store.configFile.readText()
        val read = store.readConfig()
        NativeHandle(read.getLong("version")).use { version ->
            try { store.activate(pack(store), 0, null, null, version.value); fail("unbound reference accepted") }
            catch (_: IllegalArgumentException) { }
            assertEquals(original, store.configFile.readText())
            try { store.activate(pack(store, true), 0, 1, null, version.value); fail("bad auxiliary accepted") }
            catch (_: ProtocolException) { }
            assertEquals(original, store.configFile.readText())
            store.configFile.appendText("\n# another writer\n")
            try { store.activate(pack(store), 0, 1, null, version.value); fail("stale version accepted") }
            catch (e: ProtocolException) { assertEquals(6, e.code) }
            assertTrue(store.configFile.readText().endsWith("# another writer\n"))
        }
    }

    @Test fun defaultsNeverOverwriteUserFilesAndTouchSaveIgnoresDesktopKeys() {
        val context = isolated()
        val store = EngineStore(context)
        store.configFile.appendText("\n# preserve configuration\n")
        val main = File(store.configFile.parentFile, "tables/wubi86.dict")
        main.appendText("\n# preserve table\n")
        Resources.prepare(context)
        assertTrue(main.readText().endsWith("# preserve table\n"))
        assertTrue(store.configFile.readText().endsWith("# preserve configuration\n"))
        val read = store.readConfig()
        NativeHandle(read.getLong("version")).use { version ->
            val config = read.getJSONObject("config")
            config.getJSONObject("hotkey").put("page_next", "=").put("page_prev", "=")
            val saved = store.saveConfig(config, version.value)
            NativeHandle(saved.getLong("version")).close()
        }
        assertFalse(store.configFile.readText().contains("vibration"))
    }
}
