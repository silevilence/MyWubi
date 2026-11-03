package org.mywubi

import android.content.Context
import android.util.AtomicFile
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/** Persistent display names and selectable tables; importing never selects a table. */
class TableCatalog(private val context: Context) {
    data class Table(val label: String, val file: File, val main: Boolean)

    private val root get() = Resources.prepare(context).parentFile!!
    fun resolve(path: String) = File(path).let { if (it.isAbsolute) it else File(root, path) }.canonicalFile

    fun tables(activePath: String? = null): List<Table> {
        val result = mutableListOf(
            Table("内置五笔 86", File(root, "tables/wubi86.dict"), true),
            Table("内置拼音", File(root, "tables/pinyin.dict"), false),
            Table("内置英文", File(root, "tables/english.dict"), false)
        )
        val ui = context.getSharedPreferences("android-ui", 0)
        val legacyPath = ui.getString("active_table_path", null)?.let(::resolve)
        fun legacyLabel(file: File) = if (file.canonicalFile == legacyPath)
            ui.getString("active_table_name", null) ?: file.name else "${file.name}（${file.parentFile!!.name.take(8)}）"
        for (directory in File(root, "packages").listFiles().orEmpty().sortedBy { it.name }) {
            val manifest = AtomicFile(File(directory, "catalog.json"))
            if (manifest.baseFile.exists()) {
                val items = runCatching { JSONArray(manifest.openRead().bufferedReader().use { it.readText() }) }.getOrNull()
                if (items != null) for (i in 0 until items.length()) {
                    val item = items.getJSONObject(i)
                    val file = File(directory, item.getString("file")).canonicalFile
                    if (file.parentFile == directory.canonicalFile && file.isFile)
                        result.add(Table(item.getString("label"), file, item.getBoolean("main")))
                }
            } else if (File(directory, "activation.toml").isFile) {
                // Before the catalog existed, successful imports retained their trial config.
                val trial = runCatching { NativeBridge.objectCall("ReadConfig", "path" to File(directory, "activation.toml").path) }.getOrNull()
                if (trial != null) {
                    NativeHandle(trial.getLong("version")).close()
                    val main = resolve(trial.getJSONObject("config").getJSONObject("dictionary").getString("system_table"))
                    directory.listFiles().orEmpty().filter { it.extension == "dict" }.sortedBy { it.name }.forEach {
                        result.add(Table(legacyLabel(it), it, it.canonicalFile == main))
                    }
                }
            }
        }
        activePath?.let(::resolve)?.let { active ->
            if (active.isFile && result.none { it.file.canonicalFile == active })
                result.add(Table(legacyLabel(active), active, true))
        }
        return result.distinctBy { it.file.canonicalPath }
    }

    fun label(path: String, tables: List<Table> = tables(path)): String =
        tables.firstOrNull { it.file.canonicalFile == resolve(path) }?.label ?: File(path).name

    fun record(pack: EngineStore.ImportSet, main: Int) {
        val entries = JSONArray()
        pack.files.forEachIndexed { i, file ->
            entries.put(JSONObject().put("label", file.label).put("file", file.file.name).put("main", i == main))
        }
        val target = AtomicFile(File(pack.root, "catalog.json"))
        val stream = target.startWrite()
        try { stream.write(entries.toString().toByteArray()); target.finishWrite(stream) }
        catch (failure: Exception) { target.failWrite(stream); throw failure }
    }
}
