package org.mywubi

import android.app.Application
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.util.AtomicFile
import androidx.lifecycle.AndroidViewModel
import androidx.compose.runtime.*
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.security.MessageDigest
import java.util.concurrent.Executors

/** Edits one draft at a time; shared native operations own validation and conflict checks. */
class LibraryModel(app: Application) : AndroidViewModel(app) {
    private val app = app
    private val worker = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())
    private var closed = false
    private var version: NativeHandle? = null
    private var baseline = ""
    var paths by mutableStateOf<List<Pair<String, File>>>(emptyList()); private set
    var selected by mutableIntStateOf(-1); private set
    var data by mutableStateOf(JSONObject()); private set
    var form by mutableStateOf(JSONObject()); private set
    var dirty by mutableStateOf(false); private set
    var busy by mutableStateOf(false); private set
    var message by mutableStateOf(""); private set
    val user get() = selected == paths.lastIndex
    private val path get() = paths[selected].second
    private fun digest(file: File) = MessageDigest.getInstance("SHA-256").digest(if(file.exists()) file.readBytes() else byteArrayOf()).joinToString("") { "%02x".format(it) }
    private fun draftFile(file: File = path) = AtomicFile(File(app.filesDir, "library-${file.path.hashCode()}.json"))

    init {
        work {
            val configPath = EngineStore(app).configFile
            val config = NativeBridge.objectCall("ReadConfig", "path" to configPath.path)
            NativeHandle(config.getLong("version")).close()
            val dictionary = config.getJSONObject("config").getJSONObject("dictionary")
            fun resolve(parent: File, value: String) = File(value).let { if(it.isAbsolute) it else File(parent, value) }
            val mainTable = resolve(configPath.parentFile!!, dictionary.getString("system_table"))
            val table = NativeBridge.objectCall("ReadTable", "path" to mainTable.path)
            NativeHandle(table.getLong("version")).close()
            val temporary = table.getJSONObject("config").getJSONObject("temporary")
            val available = mutableListOf("主码表" to mainTable)
            for ((key, label) in listOf("pinyin_table" to "拼音辅助表", "english_table" to "英文辅助表")) {
                val reference = temporary.getString(key)
                if (reference.isNotEmpty()) available.add(label to resolve(mainTable.parentFile!!, reference))
            }
            available.add("用户词库" to resolve(configPath.parentFile!!, dictionary.getString("user_table")))
            val publish: () -> Unit = { paths = available }
            publish
        }
    }

    private fun work(operation: () -> (() -> Unit)) {
        if (busy) return
        busy = true
        worker.execute {
            try {
                val publish = operation()
                main.post { publish(); busy = false; if (closed) version?.close() }
            } catch (e: Exception) { main.post { if (!closed) { message = e.message ?: "操作失败，草稿保留"; busy = false } } }
        }
    }

    fun open(index: Int, discard: Boolean = false) {
        if (busy) return
        if (index !in paths.indices || (dirty && !discard)) return
        val target = paths[index].second
        val targetUser = index == paths.lastIndex
        work {
            val loaded = NativeBridge.objectCall(if(targetUser) "ReadUser" else "ReadTable", "path" to target.path)
            val next = NativeHandle(loaded.getLong("version"))
            try {
            val hash = digest(target)
            val draft = if (!discard && draftFile(target).baseFile.exists()) JSONObject(draftFile(target).openRead().bufferedReader().use { it.readText() }) else null
            val restored = draft?.getJSONObject("form") ?: JSONObject().put("index", -1).put("code", "").put("word", "").put("weight", "100").put("header", loaded.optJSONObject("config") ?: JSONObject())
            val publish: () -> Unit = {
                version?.close(); version = next
                selected = index; data = loaded; form = restored
                baseline = draft?.getString("baseline") ?: hash; dirty = draft != null
                message = if (baseline != hash) "版本冲突：磁盘已更改，草稿保留；请重新载入后合并" else if(dirty) "已恢复未保存草稿" else ""
                if(discard) draftFile().delete()
            }
            publish
            } catch (e: Exception) { next.close(); throw e }
        }
    }

    fun choose(index: Int) {
        if (dirty || busy) return
        val entry = data.getJSONArray("entries").optJSONObject(index)
        form = JSONObject().put("index", index).put("code", entry?.getString("code") ?: "")
            .put("word", entry?.getString("word") ?: "").put("weight", entry?.get("weight")?.toString() ?: "100")
            .put("header", data.optJSONObject("config") ?: JSONObject())
    }

    fun change(key: String, value: Any, section: String? = null) {
        val copy = JSONObject(form.toString())
        when (section) {
            "header" -> copy.getJSONObject("header").put(key, value)
            "temporary" -> copy.getJSONObject("header").getJSONObject("temporary").put(key, value)
            else -> copy.put(key, value)
        }
        form = copy; dirty = true
        val file = draftFile()
        val out = try { file.startWrite() } catch (e: Exception) {
            message = "草稿未持久保存：${e.message}"; return
        }
        try { out.write(JSONObject().put("baseline", baseline).put("form", copy).toString().toByteArray()); file.finishWrite(out) }
        catch(e: Exception) { file.failWrite(out); message = "草稿未持久保存：${e.message}" }
    }

    fun save(remove: Boolean = false) {
        val draft = JSONObject(form.toString())
        val expected = version ?: return
        work {
            check(digest(path) == baseline) { "版本冲突：文件已修改，当前草稿未丢弃" }
            val index = draft.getInt("index")
            val entries = JSONArray(data.getJSONArray("entries").toString())
            val entry = JSONObject().put("code", draft.getString("code")).put("word", draft.getString("word"))
                .put("weight", draft.getString("weight").toLongOrNull() ?: error("权重必须是整数"))
            val result = if(user) {
                val edit = if(index >= 0) {
                    val old = entries.getJSONObject(index)
                    val id = JSONObject().put("code", old.getString("code")).put("word", old.getString("word"))
                    if(remove) JSONObject().put("Remove", id) else JSONObject().put("Update", JSONArray().put(id).put(entry))
                } else JSONObject().put("Add", entry)
                NativeBridge.objectCall("EditUser", "path" to path.path, "expected" to expected.value, "edit" to edit)
            } else {
                if(remove) entries.remove(index)
                else if(index >= 0) entries.put(index, entry)
                else if(entry.getString("code").isNotEmpty() || entry.getString("word").isNotEmpty()) entries.put(entry)
                val header = draft.getJSONObject("header")
                header.put("max_code_len", header.getString("max_code_len").toIntOrNull() ?: error("最大码长必须是整数"))
                if(header.optString("wildcard_key").isEmpty()) header.put("wildcard_key", JSONObject.NULL)
                NativeBridge.objectCall("SaveTable", "original" to path.path, "target" to path.path,
                    "config" to header, "entries" to entries, "expected" to expected.value)
            }
            NativeHandle(result.getLong("version")).close()
            EngineUpdates.request(app)
            val loaded = NativeBridge.objectCall(if(user) "ReadUser" else "ReadTable", "path" to path.path)
            val next = NativeHandle(loaded.getLong("version"))
            val hash = digest(path)
            val publish: () -> Unit = {
                version?.close(); version = next; baseline = hash; data = loaded
                dirty = false; draftFile().delete()
                form = JSONObject().put("index", -1).put("code", "").put("word", "").put("weight", "100").put("header", loaded.optJSONObject("config") ?: JSONObject())
                message = "已保存，等待输入法应用"
            }
            publish
        }
    }

    fun transfer(uri: Uri, importing: Boolean) {
        val expected = version ?: return
        work {
            val temp = File.createTempFile("user-transfer-", ".dict", app.cacheDir)
            try {
                if(importing) {
                    check(digest(path) == baseline) { "版本冲突：请重新载入词库" }
                    app.contentResolver.openInputStream(uri)?.use { input -> temp.outputStream().use { output ->
                        val bytes = input.readBytesLimit(16 * 1024 * 1024); output.write(bytes)
                    } } ?: error("无法读取选中文件")
                    val result = NativeBridge.objectCall("EditUser", "path" to path.path, "expected" to expected.value, "edit" to JSONObject().put("Import", temp.path))
                    NativeHandle(result.getLong("version")).close(); EngineUpdates.request(app)
                } else {
                    NativeHandle(NativeBridge.objectCall("FileVersion", "path" to temp.path).getLong("version")).use { destination ->
                        val result = NativeBridge.objectCall("ExportUser", "source" to path.path, "target" to temp.path, "expected" to destination.value)
                        NativeHandle(result.getLong("version")).close()
                    }
                    app.contentResolver.openOutputStream(uri, "wt")?.use { output -> temp.inputStream().use { it.copyTo(output) } } ?: error("无法写入导出文件")
                }
            } finally { temp.delete() }
            val publish: () -> Unit = { message = if(importing) "已导入，请重新载入查看词库" else "已导出用户词库" }
            publish
        }
    }

    override fun onCleared() { closed = true; worker.execute { main.post { version?.close() } }; worker.shutdown() }
}

private fun java.io.InputStream.readBytesLimit(limit: Int): ByteArray {
    val output = java.io.ByteArrayOutputStream()
    val buffer = ByteArray(8192)
    while(true) { val n = read(buffer); if(n < 0) break; require(output.size() + n <= limit) { "导入文件超过 16 MiB" }; output.write(buffer, 0, n) }
    return output.toByteArray()
}
