package org.mywubi

import android.content.Context
import android.content.ContextWrapper
import android.net.Uri
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.util.UUID

class ExpressionResourceTest {
    private fun isolated(): Context {
        val target = InstrumentationRegistry.getInstrumentation().targetContext
        val root = File(target.cacheDir, "expressions-${UUID.randomUUID()}").apply { mkdirs() }
        return object : ContextWrapper(target) {
            override fun getFilesDir() = root
            override fun getSharedPreferences(name: String, mode: Int) = target.getSharedPreferences("${root.name}-$name", mode)
        }
    }

    private fun source(context: Context): File = File(context.filesDir, "source.mwe").apply {
        writeText("# MyWubi Expressions 1\n" + """{
          "kind":"emoji","name":"设备测试","version":"1",
          "sources":[{"id":"test","url":"local:test","revision":"1","license":"MIT","notice":"Complete fixture notice"}],
          "entries":[{"text":"👩🏽‍💻","name":"开心","triggers":["开心"],"order":0,"sources":["test"]}]
        }""")
    }

    @Test fun bundledCompleteLibrariesAndNoticesAreAvailableOffline() {
        val context = isolated()
        val store = EngineStore(context)
        val read = store.readConfig()
        NativeHandle(read.getLong("version")).close()
        val entries = ExpressionCatalog(context).list(read.getJSONObject("config"))
        assertEquals(3972, entries.getValue("emoji").single().getInt("entries"))
        assertEquals(34354, entries.getValue("kaomoji").single().getInt("entries"))
        for (notice in listOf("Unicode.txt", "Kaomojiya.txt", "Kaomojikan.txt", "MyWubi-MIT.txt")) {
            assertTrue(context.assets.open("expressions/licenses/$notice").bufferedReader().use { it.readText() }.isNotBlank())
        }
    }

    @Test fun independentSwitchesActivateWithBuiltinPathsFromTemporaryValidationDirectory() {
        val store = EngineStore(isolated())
        for ((emoji, kaomoji) in listOf(true to false, false to true, true to true, false to false)) {
            val read = store.readConfig()
            NativeHandle(read.getLong("version")).use { expected ->
                val config = read.getJSONObject("config")
                config.getJSONObject("expressions").getJSONObject("emoji").put("append", emoji)
                config.getJSONObject("expressions").getJSONObject("kaomoji").put("search", kaomoji)
                NativeHandle(store.activateConfig(config, expected.value).getLong("version")).close()
            }
            val reopened = store.readConfig()
            NativeHandle(reopened.getLong("version")).close()
            val expressions = reopened.getJSONObject("config").getJSONObject("expressions")
            assertEquals(emoji, expressions.getJSONObject("emoji").getBoolean("append"))
            assertEquals(kaomoji, expressions.getJSONObject("kaomoji").getBoolean("search"))
            assertTrue(expressions.getJSONObject("emoji").isNull("resource"))
        }
    }

    @Test fun importSurvivesSourceRemovalAndSelectionSurvivesRestartAndMainTableChange() {
        val context = isolated()
        val store = EngineStore(context)
        val original = store.configFile.readText()
        val source = source(context)
        val imported = ExpressionCatalog(context).import("emoji", Uri.fromFile(source))
        assertEquals(original, store.configFile.readText())
        assertTrue(source.delete())
        val path = File(imported.getString("path")).relativeTo(store.configFile.parentFile!!).path
        for (main in listOf("tables/wubi86.dict", "tables/english.dict")) {
            val read = store.readConfig()
            NativeHandle(read.getLong("version")).use { expected ->
                val config = read.getJSONObject("config")
                config.getJSONObject("dictionary").put("system_table", main)
                config.getJSONObject("expressions").getJSONObject("emoji").put("append", true).put("resource", path)
                NativeHandle(store.activateConfig(config, expected.value).getLong("version")).close()
            }
            val readAgain = EngineStore(context).readConfig()
            NativeHandle(readAgain.getLong("version")).close()
            assertEquals(path, readAgain.getJSONObject("config").getJSONObject("expressions").getJSONObject("emoji").getString("resource"))
        }
        val read = store.readConfig()
        NativeHandle(read.getLong("version")).use { expected ->
            val config = read.getJSONObject("config")
            config.getJSONObject("expressions").getJSONObject("emoji").put("resource", JSONObject.NULL)
            NativeHandle(store.activateConfig(config, expected.value).getLong("version")).close()
        }
        assertTrue(File(imported.getString("path")).isFile)
    }

    @Test fun wrongTypeAndCorruptResourceNeverOverwriteActiveConfigAndPreparationPreservesFiles() {
        val context = isolated()
        val store = EngineStore(context)
        val original = store.configFile.readText()
        val source = source(context)
        try { ExpressionCatalog(context).import("kaomoji", Uri.fromFile(source)); fail("wrong type accepted") }
        catch (_: ProtocolException) { }
        val imported = ExpressionCatalog(context).import("emoji", Uri.fromFile(source))
        File(imported.getString("path")).writeText("broken")
        val read = store.readConfig()
        NativeHandle(read.getLong("version")).use { expected ->
            val config = read.getJSONObject("config")
            config.getJSONObject("expressions").getJSONObject("emoji").put("append", true).put("resource", imported.getString("path"))
            try { store.activateConfig(config, expected.value); fail("broken resource accepted") }
            catch (_: ProtocolException) { }
        }
        assertEquals(original, store.configFile.readText())
        val builtin = File(store.configFile.parentFile, "expressions/emoji.mwe")
        source.copyTo(builtin, overwrite = true)
        Resources.prepare(context)
        assertEquals(source.readText(), builtin.readText())
        assertEquals(original, store.configFile.readText())
    }
}
