package org.mywubi

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import org.json.JSONObject
import java.io.File
import java.util.UUID

/** Private immutable import generations, activated by one atomic config replacement. */
class EngineStore(private val context: Context) {
    val configFile get() = Resources.prepare(context)

    fun readConfig() = NativeBridge.objectCall("ReadConfig", "path" to configFile.path)

    fun saveConfig(config: JSONObject, expected: Long): JSONObject {
        val result = NativeBridge.objectCall("SaveConfig", "path" to configFile.path,
            "config" to config, "expected" to expected)
        EngineUpdates.request(context)
        return result
    }

    data class ImportFile(val label: String, val file: File)
    data class ImportSet(val root: File, val files: List<ImportFile>)

    fun stage(uris: List<Uri>): ImportSet {
        require(uris.isNotEmpty() && uris.size <= 32) { "请选择 1～32 个文件" }
        val root = File(configFile.parentFile, "packages/${UUID.randomUUID()}")
        check(root.mkdirs()) { "无法创建导入目录" }
        val files = uris.mapIndexed { index, uri ->
            val label = context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
                if (it.moveToFirst()) it.getString(0) else null
            } ?: "文件 ${index + 1}"
            // Provider filenames are display text only, never paths. Duplicate names
            // remain separate and require explicit reference selection in the UI.
            val target = File(root, "$index.dict")
            context.contentResolver.openInputStream(uri)?.use { input ->
                target.outputStream().use { out ->
                    val buffer = ByteArray(8192)
                    var total = 0L
                    while (true) {
                        val size = input.read(buffer)
                        if (size < 0) break
                        total += size
                        require(total <= 16L * 1024 * 1024) { "$label 超出 16 MiB 导入限制" }
                        out.write(buffer, 0, size)
                    }
                    out.fd.sync()
                }
            } ?: error("无法读取 $label")
            ImportFile(label, target)
        }
        return ImportSet(root, files)
    }

    /** Fault checkpoints are used by the device contract test to simulate process death. */
    fun activate(pack: ImportSet, main: Int, pinyin: Int?, english: Int?, expected: Long, draft: JSONObject? = null,
                 checkpoint: (String) -> Unit = {}): JSONObject {
        require(main in pack.files.indices) { "请显式选择主码表" }
        val mainFile = pack.files[main].file
        val table = NativeBridge.objectCall("ReadTable", "path" to mainFile.path)
        NativeHandle(table.getLong("version")).use { version ->
            val header = table.getJSONObject("config")
            val temporary = header.getJSONObject("temporary")
            fun bind(key: String, index: Int?) {
                require(temporary.optString(key).isEmpty() || index != null) { "$key 的引用尚未重绑定" }
                if (index != null) require(index in pack.files.indices && index != main) { "辅助表必须选择另一个文件" }
                temporary.put(key, index?.let { pack.files[it].file.name } ?: "")
            }
            bind("pinyin_table", pinyin); bind("english_table", english)
            val saved = NativeBridge.objectCall("SaveTable", "original" to mainFile.path, "target" to mainFile.path,
                "config" to header, "entries" to table.getJSONArray("entries"), "expected" to version.value)
            NativeHandle(saved.getLong("version")).close()
        }
        NativeBridge.call("ValidatePackage", "root" to pack.root.path, "main" to mainFile.path)
        val current = readConfig()
        NativeHandle(current.getLong("version")).use {
            val config = JSONObject((draft ?: current.getJSONObject("config")).toString())
            config.getJSONObject("dictionary").put("system_table", mainFile.path)
            // Validate the full prospective runtime before touching the active config.
            val trial = File(pack.root, "activation.toml")
            val trialConfig = JSONObject(config.toString())
            val user = trialConfig.getJSONObject("dictionary").getString("user_table")
            if (!File(user).isAbsolute) trialConfig.getJSONObject("dictionary").put("user_table", File(configFile.parentFile, user).path)
            NativeHandle(NativeBridge.objectCall("FileVersion", "path" to trial.path).getLong("version")).use { trialVersion ->
                val saved = NativeBridge.objectCall("SaveConfig", "path" to trial.path, "config" to trialConfig, "expected" to trialVersion.value)
                NativeHandle(saved.getLong("version")).close()
            }
            NativeHandle(NativeBridge.objectCall("BuildRuntime", "path" to trial.path).getLong("handle")).close()
            checkpoint("before_activation")
            val result = saveConfig(config, expected)
            try { checkpoint("after_activation") } catch (failure: Exception) {
                NativeHandle(result.getLong("version")).close()
                throw failure
            }
            // Old generations are intentionally retained: live snapshots and user
            // drafts may still reference them. No eager resource garbage collection.
            return result
        }
    }
}
