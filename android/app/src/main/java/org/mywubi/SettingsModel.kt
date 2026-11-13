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
    private val downloadControl = Executors.newSingleThreadExecutor()
    private var downloadPollPending = false
    private var downloadListPending = false
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
    var tables by mutableStateOf<List<TableCatalog.Table>>(emptyList()); private set
    var expressionResources by mutableStateOf<Map<String, List<JSONObject>>>(emptyMap()); private set
    var expressionError by mutableStateOf(""); private set
    var specialSearch by mutableStateOf("")
    var specialSelection by mutableStateOf("emoji")
    var specialDetail by mutableStateOf(false)
    val specialListScroll = androidx.compose.foundation.ScrollState(0)
    val specialDetailScroll = androidx.compose.foundation.ScrollState(0)
    val specialTrials = specialCategories.associate {it.first to ExpressionTrialDraft()}
    var personalStatus by mutableStateOf(""); private set
    var personalPreview by mutableStateOf<JSONObject?>(null); private set
    private var personalImport: File? = null
    private var personalImportPassword: String? = null
    private var personalImportMode = "Merge"

    private fun personalRequest(op: String, vararg fields: Pair<String,Any?>): JSONObject {
        val request=JSONObject().put("op",op).put("config",store.configFile.absolutePath)
        fields.forEach { (key,value)->request.put(key,value ?: JSONObject.NULL) }
        val reply=JSONObject(NativeBridge.personalManage(request.toString().toByteArray(Charsets.UTF_8)).toString(Charsets.UTF_8))
        check(reply.getBoolean("ok")) { reply.optString("error","个人数据操作失败") }
        return reply.getJSONObject("value")
    }
    fun personalAction(op: String, models: Boolean = false) = work {
        val result=personalRequest(op,"models" to models)
        val text=if(op=="Status") {
            val data=result.getJSONObject("data")
            val model=result.optJSONObject("model")
            val learning=result.optJSONObject("learning")
            "记录 ${data.optInt("records")} 条，占用 ${data.optLong("bytes")} 字节，缺失 ${data.optLong("gaps")} 条；" +
                (if(model==null) "个人模型待积累/未就绪" else "版本 ${model.optString("version")}，样本 ${model.optInt("samples")}，模型 ${model.optLong("bytes")} 字节，更新时间 ${model.optLong("updated")}") +
                (if(learning==null) "；等待输入法启动" else "；学习：${learning.optString("phase")} ${learning.optString("reason")}，上一批 ${learning.optLong("elapsed_ms")} ms；1 线程 / 2 秒批次 / 8 MB 模型")
        } else result.optString("message","操作完成")
        val publish:()->Unit={personalStatus=text;personalPreview=null};publish
    }
    fun exportPersonal(uri: Uri, password: String?) = work {
        val temporary=File.createTempFile("personal-export-",".mwp",app.cacheDir)
        try {
            personalRequest("Export","path" to temporary.absolutePath,"password" to password)
            app.contentResolver.openOutputStream(uri,"wt")?.use { out->temporary.inputStream().use { it.copyTo(out) } } ?: error("无法写入目标文件")
        } finally { temporary.delete() }
        val publish:()->Unit={personalStatus="个人数据已导出"};publish
    }
    fun previewPersonal(uri: Uri, password: String?, replace: Boolean) = work {
        val temporary=File.createTempFile("personal-import-",".mwp",app.cacheDir)
        try {
            app.contentResolver.openInputStream(uri)?.use { input -> temporary.outputStream().use { out ->
                val buffer=ByteArray(8192);var total=0L
                while(true){val n=input.read(buffer);if(n<0)break;total+=n;require(total<=128L*1024*1024+53){"个人数据包超过 128 MB"};out.write(buffer,0,n)}
            } } ?: error("无法读取数据包")
            val mode=if(replace)"Replace" else "Merge"
            val preview=personalRequest("Preview","path" to temporary.absolutePath,"password" to password,"mode" to mode)
            val publish:()->Unit={
                personalImport?.delete();personalImport=temporary;personalImportPassword=password;personalImportMode=mode
                personalPreview=preview;personalStatus="请核对导入影响后确认"
            };publish
        } catch(e:Exception){temporary.delete();throw e}
    }
    fun confirmPersonalImport() {
        val file=personalImport ?: return
        val revision=personalPreview?.optString("revision") ?: return
        val password=personalImportPassword;val mode=personalImportMode
        work {
            personalRequest("Import","path" to file.absolutePath,"password" to password,"mode" to mode,"revision" to revision)
            file.delete()
            val publish:()->Unit={personalPreview=null;personalImport=null;personalImportPassword=null;personalStatus="个人数据已导入"};publish
        }
    }

    fun refreshExpressions() {
        val draft = JSONObject(config?.toString() ?: return)
        work {
            val result = runCatching { ExpressionCatalog(app).list(draft) }
            val publish: () -> Unit = {
                expressionResources = result.getOrDefault(emptyMap())
                expressionError = result.exceptionOrNull()?.message ?: ""
            }; publish
        }
    }

    fun selectExpression(kind: String, resource: JSONObject?) {
        val selection = JSONObject(config?.optJSONObject("expressions")?.optJSONObject(kind)?.toString() ?: "{}")
        val path = if (resource == null || resource.optBoolean("builtin")) JSONObject.NULL
            else File(resource.getString("path")).relativeToOrSelf(store.configFile.parentFile!!).path
        selection.put("resource", path)
        change("expressions", kind, selection)
    }

    fun importExpression(kind: String, uri: Uri) = work {
        val resource = ExpressionCatalog(app).import(kind, uri)
        val publish: () -> Unit = {
            expressionResources = expressionResources + (kind to (expressionResources[kind].orEmpty() + resource))
            expressionError = ""
            selectExpression(kind, resource)
            message = "词库已导入并选中；点击保存设置生效"
        }; publish
    }
    var models by mutableStateOf<List<JSONObject>>(emptyList()); private set
    var modelsLoaded by mutableStateOf(false); private set
    val downloadableModels = JSONObject(NativeBridge.modelDownloads("{\"op\":\"Available\"}".toByteArray()).toString(Charsets.UTF_8)).getJSONArray("value")
    var downloadStatus by mutableStateOf(JSONObject().put("phase","idle")); private set
    var downloadError by mutableStateOf(""); private set
    private fun downloadCommand(op:String, model:JSONObject?=null, source:String?=null) {
        if(closed || (op=="Status" && downloadPollPending)) return
        if(op=="Status") downloadPollPending=true
        downloadControl.execute {
            val result=runCatching {
                val request=JSONObject().put("op",op).put("config",store.configFile.absolutePath)
                model?.let {request.put("model",it)}; source?.let {request.put("source",it)}
                val reply=JSONObject(NativeBridge.modelDownloads(request.toString().toByteArray(Charsets.UTF_8)).toString(Charsets.UTF_8))
                check(reply.getBoolean("ok")) {reply.getString("error")}
                reply.getJSONObject("value")
            }
            main.post {
                downloadPollPending=false
                if(!closed) {
                    result.onSuccess { next ->
                        if(next.optString("phase")=="ready" && downloadStatus.optString("phase")!="ready") downloadListPending=true
                        downloadStatus=next;downloadError=""
                    }.onFailure {downloadError=it.message ?: "下载管理器不可用"}
                    if(downloadListPending && !busy) {downloadListPending=false;refreshModels()}
                }
            }
        }
    }
    fun refreshDownloadStatus()=downloadCommand("Status")
    fun startDownload(model:JSONObject, source:String)=downloadCommand("Start",model,source)
    fun cancelDownload()=downloadCommand("Cancel")
    val modelBackends = JSONObject(NativeBridge.inference("{\"op\":\"Features\"}".toByteArray()).toString(Charsets.UTF_8)).getJSONObject("value")
    var modelStatus by mutableStateOf("尚未查询运行状态"); private set
    fun refreshModelStatus() {
        if(closed) return
        downloadControl.execute {
            val status=runCatching {
                val result=JSONObject(NativeBridge.inference("{\"op\":\"Status\"}".toByteArray()).toString(Charsets.UTF_8))
                if(!result.getBoolean("ok")) "模型工作线程未启动" else when(result.optString("value")) {
                    "Loading" -> "正在加载和预热"; "Ready" -> "模型已就绪"; "Paused" -> "已暂停，释放资源"; "Idle" -> "空闲，尚未加载"
                    else -> "加载或推理失败：${result.opt("value")}"
                }
            }.getOrElse { "无法查询运行状态：${it.message}" }
            main.post {if(!closed) modelStatus=status}
        }
    }
    private fun modelList(array: JSONArray)= (0 until array.length()).map { array.getJSONObject(it) }
    fun ensureModelsLoaded() { if (!modelsLoaded && !busy) refreshModels() }
    fun refreshModels()=work {
        val next=modelList(ModelCatalog(app).call("List"))
        val publish:()->Unit={models=next;modelsLoaded=true;message="模型包校验完成"};publish
    }
    fun importModel(uri:Uri)=work {
        val next=modelList(ModelCatalog(app).importTree(uri))
        val publish:()->Unit={models=next;modelsLoaded=true;message="模型已导入；选择后保存设置"};publish
    }
    fun removeModel(selection:JSONObject)=work {
        val next=modelList(ModelCatalog(app).call("Remove","selection" to selection))
        val publish:()->Unit={models=next;modelsLoaded=true;message="未选模型已移除"};publish
    }
    var trialResult by mutableStateOf(""); private set
    var trialRunning by mutableStateOf(false); private set
    fun clearTrialResult() { trialResult="" }
    fun trialModel(name:String,prefix:String) {
      if(busy) return
      trialResult=""
      trialRunning=true
      work {
        check(models.any {"${it.getString("id")}--${it.getString("version")}"==name}) {"请选择已校验模型"}
        val request=JSONObject().put("package",File(store.configFile.parentFile,"models/$name").absolutePath)
            .put("prefix",prefix).put("wait_ms",1000).put("threads",4)
        val reply=JSONObject(NativeBridge.modelTrial(request.toString().toByteArray()).toString(Charsets.UTF_8))
        val text=if(reply.optBoolean("ok")) reply.getJSONObject("value").let {
            "候选：${it.getJSONArray("words")}\n加载与预热 ${it.getLong("load_ms")} ms；推理 ${it.getLong("predict_ms")} ms\n${it.getString("notice")}" }
        else "${reply.optString("error")}（总耗时 ${reply.optLong("elapsed_ms")} ms）"
        val publish:()->Unit={trialResult=text};publish
      }
    }
    val ui = app.getSharedPreferences("android-ui", 0)

    val tableLabel: String get() {
        val path = config?.getJSONObject("dictionary")?.getString("system_table") ?: return "正在读取…"
        return TableCatalog(app).label(path, tables)
    }

    init { reload() }

    private fun fingerprint() = MessageDigest.getInstance("SHA-256").digest(store.configFile.readBytes()).joinToString("") { "%02x".format(it) }

    fun work(operation: () -> (() -> Unit)) {
        if (busy) return
        busy = true
        worker.execute {
            try {
                val publish = operation()
                main.post { publish(); busy = false; trialRunning = false; if (closed) expected?.close() }
            } catch (e: Exception) {
                main.post { if (!closed) { message = e.message ?: "操作失败，草稿已保留"; busy = false; trialRunning = false } }
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
        val available = TableCatalog(app).tables(restored.getJSONObject("dictionary").getString("system_table"))
        val expressions = runCatching { ExpressionCatalog(app).list(restored) }
        val imported = saved?.optJSONObject("pack")?.let { p ->
            val files = p.getJSONArray("files")
            EngineStore.ImportSet(File(p.getString("root")), (0 until files.length()).map { i ->
                files.getJSONObject(i).let {
                    EngineStore.ImportFile(it.getString("label"), File(it.getString("path")),
                        it.optString("source_name", it.getString("label")))
                }
            })
        }
        val publish: () -> Unit = {
            expected?.close(); expected = version
            baseline = saved?.optString("baseline") ?: disk
            config = restored; dirty = saved != null; pack = imported
            tables = available
            expressionResources = expressions.getOrDefault(emptyMap())
            expressionError = expressions.exceptionOrNull()?.message ?: ""
            // Large model checksums are only needed when the user opens model management.
            models = emptyList(); modelsLoaded = false
            message = if (saved != null && baseline != disk) "配置已被其他操作修改。草稿保留，请重新载入后合并。" else if (saved != null) "已恢复未保存草稿" else ""
            if (discard) draftFile.delete()
        }
        publish
        } catch (e: Exception) { version.close(); throw e }
    }

    fun change(section: String, key: String, value: Any) {
        val copy = JSONObject(config?.toString() ?: return)
        val target = copy.optJSONObject(section) ?: JSONObject().also { copy.put(section, it) }
        target.put(key, value)
        config = copy; dirty = true
        persistDraft()
    }

    private fun persistDraft() {
        val draft = JSONObject().put("baseline", baseline).put("config", config)
        pack?.let { p -> draft.put("pack", JSONObject().put("root", p.root.path).put("files", JSONArray().also { a ->
            p.files.forEach { a.put(JSONObject().put("label", it.label).put("path", it.file.path).put("source_name", it.sourceName)) }
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
            val result = store.activateConfig(draft, version.value)
            val next = NativeHandle(result.getLong("version"))
            val disk = fingerprint()
            val publish: () -> Unit = {
                expected?.close(); expected = next; baseline = disk
                result.optJSONObject("config")?.let { config = JSONObject(it.toString()) }
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
            val publish: () -> Unit = {
                pack = staged; dirty = true; persistDraft()
                message = if (staged.files.size == 1) "已选择 ${staged.files.single().label}，点击「校验并导入」完成导入"
                    else "请选择主表及其直接引用的辅助表"
            }
            publish
        }
    }

    fun importPackage(mainIndex: Int, pinyin: Int?, english: Int?) {
        val imported = pack ?: return
        work {
            store.importPackage(imported, mainIndex, pinyin, english)
            val available = TableCatalog(app).tables(config!!.getJSONObject("dictionary").getString("system_table"))
            val disk = store.readConfig()
            NativeHandle(disk.getLong("version")).close()
            val unchanged = baseline == fingerprint() && config.toString() == disk.getJSONObject("config").toString()
            val publish: () -> Unit = {
                tables = available; pack = null; dirty = !unchanged
                if (dirty) persistDraft() else draftFile.delete()
                message = "已导入；当前码表未改变，请选择码表后保存设置"
            }
            publish
        }
    }

    fun selectTable(table: TableCatalog.Table) {
        if (!busy && pack == null) change("dictionary", "system_table", table.file.path)
    }

    fun cancelImport() {
        if (busy || pack == null) return
        pack = null
        persistDraft()
        message = "已取消待导入码表，设置草稿已保留"
    }

    override fun onCleared() {
        cancelDownload()
        closed = true
        worker.execute { main.post { expected?.close() } }
        worker.shutdown()
        downloadControl.shutdown()
    }
}
