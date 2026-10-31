package org.mywubi

import android.app.Application
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.util.AtomicFile
import androidx.lifecycle.AndroidViewModel
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.security.MessageDigest
import java.util.concurrent.Executors

class SettingsModel(application: Application) : AndroidViewModel(application) {
    private val app = application
    val store = EngineStore(app)
    private val worker = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())
    private val draftFile = AtomicFile(File(app.filesDir, "settings-draft.json"))
    private var expected: NativeHandle? = null
    private var baseline = ""
    private var closed = false
    var config by mutableStateOf<JSONObject?>(null); private set
    var dirty by mutableStateOf(false); private set
    var busy by mutableStateOf(false); private set
    var message by mutableStateOf(""); private set
    var pack by mutableStateOf<EngineStore.ImportSet?>(null); private set
    val ui = app.getSharedPreferences("android-ui", 0)

    init { reload() }

    private fun fingerprint() = MessageDigest.getInstance("SHA-256").digest(store.configFile.readBytes()).joinToString("") { "%02x".format(it) }

    fun work(operation: () -> (() -> Unit)) {
        if (busy) return
        busy = true
        worker.execute {
            try {
                val publish = operation()
                main.post { publish(); busy = false; if (closed) expected?.close() }
            } catch (e: Exception) {
                main.post { if (!closed) { message = e.message ?: "操作失败，草稿已保留"; busy = false } }
            }
        }
    }

    fun reload(discard: Boolean = false) = work {
        val loaded = store.readConfig()
        val version = NativeHandle(loaded.getLong("version"))
        try {
        val disk = fingerprint()
        val saved = if (!discard && draftFile.baseFile.exists()) JSONObject(draftFile.openRead().bufferedReader().use { it.readText() }) else null
        val restored = saved?.getJSONObject("config") ?: loaded.getJSONObject("config")
        val imported = saved?.optJSONObject("pack")?.let { p ->
            val files = p.getJSONArray("files")
            EngineStore.ImportSet(File(p.getString("root")), (0 until files.length()).map { i ->
                files.getJSONObject(i).let { EngineStore.ImportFile(it.getString("label"), File(it.getString("path"))) }
            })
        }
        val publish: () -> Unit = {
            expected?.close(); expected = version
            baseline = saved?.optString("baseline") ?: disk
            config = restored; dirty = saved != null; pack = imported
            message = if (saved != null && baseline != disk) "配置已被其他操作修改。草稿保留，请重新载入后合并。" else if (saved != null) "已恢复未保存草稿" else ""
            if (discard) draftFile.delete()
        }
        publish
        } catch (e: Exception) { version.close(); throw e }
    }

    fun change(section: String, key: String, value: Any) {
        val copy = JSONObject(config?.toString() ?: return)
        copy.getJSONObject(section).put(key, value)
        config = copy; dirty = true
        persistDraft()
    }

    private fun persistDraft() {
        val draft = JSONObject().put("baseline", baseline).put("config", config)
        pack?.let { p -> draft.put("pack", JSONObject().put("root", p.root.path).put("files", JSONArray().also { a ->
            p.files.forEach { a.put(JSONObject().put("label", it.label).put("path", it.file.path)) }
        })) }
        val stream = try { draftFile.startWrite() } catch (e: Exception) {
            message = "草稿未能持久保存：${e.message}"; return
        }
        try { stream.write(draft.toString().toByteArray()); draftFile.finishWrite(stream) }
        catch (e: Exception) { draftFile.failWrite(stream); message = "草稿未能持久保存：${e.message}" }
    }

    fun save() {
        val draft = JSONObject(config?.toString() ?: return)
        val version = expected ?: return
        work {
            check(baseline == fingerprint()) { "版本冲突：磁盘配置已修改，草稿仍保留" }
            val result = store.saveConfig(draft, version.value)
            val next = NativeHandle(result.getLong("version"))
            val disk = fingerprint()
            val publish: () -> Unit = {
                expected?.close(); expected = next; baseline = disk
                dirty = false; pack = null; draftFile.delete()
                message = "已保存；输入法启动或重载后应用"
            }
            publish
        }
    }

    fun stage(uris: List<Uri>) {
        if (uris.isEmpty()) return
        work {
            val staged = store.stage(uris)
            val publish: () -> Unit = { pack = staged; dirty = true; persistDraft(); message = "请选择主表及其直接引用的辅助表" }
            publish
        }
    }

    fun activate(mainIndex: Int, pinyin: Int?, english: Int?) {
        val imported = pack ?: return
        val version = expected ?: return
        val draft = JSONObject(config?.toString() ?: return)
        work {
            check(baseline == fingerprint()) { "版本冲突：请保留草稿并重新载入配置" }
            val result = store.activate(imported, mainIndex, pinyin, english, version.value, draft)
            NativeHandle(result.getLong("version")).close()
            val loaded = store.readConfig()
            val next = NativeHandle(loaded.getLong("version"))
            val disk = fingerprint()
            val publish: () -> Unit = {
                expected?.close(); expected = next; baseline = disk
                config = loaded.getJSONObject("config"); dirty = false; pack = null; draftFile.delete()
                message = "码表包已保存；等待输入法应用"
            }
            publish
        }
    }

    override fun onCleared() {
        closed = true
        worker.execute { main.post { expected?.close() } }
        worker.shutdown()
    }
}
