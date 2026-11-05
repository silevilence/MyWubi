package org.mywubi
import org.json.JSONObject

import android.content.Intent
import android.os.Bundle
import android.provider.Settings
import android.view.inputmethod.InputMethodManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.lifecycle.ViewModelProvider
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.Image
import androidx.compose.ui.res.painterResource
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.unit.dp

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val model = ViewModelProvider(this)[SettingsModel::class.java]
        setContent { MyWubiTheme { SettingsScreen(model,
            { startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS)) },
            { getSystemService(InputMethodManager::class.java).showInputMethodPicker() },
            { finish() }) } }
    }
}

@Composable
private fun SettingsScreen(model: SettingsModel, enable: () -> Unit, choose: () -> Unit, leave: () -> Unit) {
    var confirmLeave by remember { mutableStateOf(false) }
    var confirmReload by remember { mutableStateOf(false) }
    var main by rememberSaveable { mutableIntStateOf(-1) }
    var pinyin by rememberSaveable { mutableIntStateOf(-1) }
    var english by rememberSaveable { mutableIntStateOf(-1) }
    var vibration by remember { mutableStateOf(model.ui.getBoolean("vibration", false)) }
    var localPreedit by remember { mutableStateOf(model.ui.getBoolean("local_preedit", false)) }
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        if (uris.isNotEmpty()) { main = -1; pinyin = -1; english = -1; model.stage(uris) }
    }
    val modelPicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
        uri?.let(model::importModel)
    }
    BackHandler(model.dirty || model.busy) { confirmLeave = true }
    Surface {
        Column(Modifier.fillMaxSize().safeDrawingPadding().imePadding().verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(verticalAlignment = androidx.compose.ui.Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Image(painterResource(R.drawable.mywubi_brand), null, Modifier.size(56.dp))
                Text("MyWubi", style = MaterialTheme.typography.headlineLarge)
            }
            Text("形码 · 拼音 · 英文", style = MaterialTheme.typography.titleMedium)
            Text("内置五笔 86，也可导入自己的码表。")
            Button(onClick = enable) { Text("1. 启用 MyWubi") }
            OutlinedButton(onClick = choose) { Text("2. 选择输入法") }
            OutlinedButton(onClick = { model.getApplication<android.app.Application>().startActivity(Intent(model.getApplication(), UpdateActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }) { Text("应用更新") }
            HorizontalDivider()
            Text("码表", style = MaterialTheme.typography.titleLarge)
            Text("当前码表：${model.tableLabel}")
            TableChoice("选择当前码表", model.tableLabel, model.tables.filter { it.main }.map { it.label to it.file.path },
                model.config != null && !model.busy && model.pack == null) { path ->
                model.selectTable(model.tables.first { it.file.path == path })
            }
            Text("选择后点击「保存设置」生效；导入只添加码表，不切换当前码表。", style = MaterialTheme.typography.bodySmall)
            Text("把码表放到手机的「下载」目录，再点下方按钮，从系统文件选择器中打开。")
            Button(onClick = { picker.launch(arrayOf("*/*")) }, enabled = model.config != null && !model.busy) { Text("从手机导入码表") }
            Text("支持 MyWubi UTF-8 文本码表（编码、词条、可选权重，以 Tab 分隔）。不支持直接读取搜狗 .scel、RIME .yaml 等其它格式。单文件自动作为主表；有拼音或英文引用时，长按多选所需文件。", style = MaterialTheme.typography.bodySmall)
            model.pack?.let { pack ->
                val selectedMain = if (pack.files.size == 1) 0 else main
                Text("已选择 ${pack.files.size} 个文件 · 尚未导入", style = MaterialTheme.typography.titleMedium)
                FileChoice("主码表（必选）", pack, selectedMain, false, !model.busy) { main = it }
                if (pack.files.size > 1) {
                    FileChoice("拼音辅助表", pack, pinyin, true, !model.busy) { pinyin = it }
                    FileChoice("英文辅助表", pack, english, true, !model.busy) { english = it }
                }
                Text("校验成功后添加到码表列表，当前码表和设置保持不变。")
                Button(onClick = { model.importPackage(selectedMain, pinyin.takeIf { it >= 0 }, english.takeIf { it >= 0 }) }, enabled = selectedMain in pack.files.indices && !model.busy) { Text("校验并导入") }
                OutlinedButton(onClick = model::cancelImport, enabled = !model.busy) { Text("取消本次导入") }
            }
            if (model.busy) LinearProgressIndicator(Modifier.fillMaxWidth())
            if (model.message.isNotEmpty()) Text(model.message, color = MaterialTheme.colorScheme.primary, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
            ApplyStatus(model)
            HorizontalDivider()
            Text("输入设置", style = MaterialTheme.typography.titleLarge)
            val config = model.config
            if (config == null) Text("正在读取设置…") else {
                val basic = config.getJSONObject("basic")
                val dict = config.getJSONObject("dictionary")
                val prediction = config.optJSONObject("prediction") ?: JSONObject().put("enabled", false)
                SettingToggle("上屏后显示字词预测", prediction.optBoolean("enabled"), !model.busy) { model.change("prediction", "enabled", it) }
                Text("本地词典续接与常用搭配；点击或数字选词，空格选第一项，继续输入编码即忽略。默认关闭，编码候选顺序不变。")
                PredictionModels(model, prediction) { modelPicker.launch(null) }
                for ((key, label) in listOf("auto_commit_unique" to "唯一候选自动上屏", "commit_on_max_code_overflow" to "超过最大码长时顶字", "show_code_hints" to "显示候选编码提示")) {
                    SettingToggle(label, basic.getBoolean(key), !model.busy) { model.change("basic", key, it) }
                }
                SettingToggle("启用用户词库", dict.getBoolean("enable_user_dict"), !model.busy) { model.change("dictionary", "enable_user_dict", it) }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Button(onClick = model::save, enabled = model.dirty && model.pack == null && !model.busy) { Text("保存设置") }
                    OutlinedButton(onClick = { confirmReload = true }, enabled = !model.busy) { Text("重新载入") }
                }
            }
            Text(if (model.dirty) "有未保存更改 · 草稿会保留" else "没有未保存更改")
            HorizontalDivider()
            Text("手机键盘", style = MaterialTheme.typography.titleLarge)
            SettingToggle("按键振动", vibration) { vibration = it; model.ui.edit().putBoolean("vibration", it).apply() }
            SettingToggle("仅在候选栏显示预编辑", localPreedit) { localPreedit = it; model.ui.edit().putBoolean("local_preedit", it).apply() }
            Text("预编辑兼容模式在下次进入输入框时生效；当前输入不会重放。手机设置单独保存。")
            OutlinedButton(onClick = { model.getApplication<android.app.Application>().startActivity(Intent(model.getApplication(), LibraryActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }, enabled = !model.busy) { Text("编辑码表与用户词库") }
            Spacer(Modifier.height(24.dp))
        }
    }
    if (confirmLeave) AlertDialog(onDismissRequest = { confirmLeave = false }, title = { Text("保留草稿并退出？") }, text = { Text("更改尚未保存到引擎配置，下次打开可继续编辑。") }, confirmButton = { TextButton(onClick = leave, enabled = !model.busy) { Text("保留并退出") } }, dismissButton = { TextButton(onClick = { confirmLeave = false }) { Text("继续编辑") } })
    if (confirmReload) AlertDialog(onDismissRequest = { confirmReload = false }, title = { Text("重新载入磁盘设置？") }, text = { Text("这会放弃当前设置草稿和待启用的导入选择，已保存数据不受影响。") }, confirmButton = { TextButton(onClick = { confirmReload = false; model.reload(true) }) { Text("重新载入") } }, dismissButton = { TextButton(onClick = { confirmReload = false }) { Text("保留草稿") } })
}

@Composable
internal fun TableChoice(label: String, current: String, options: List<Pair<String, String>>, enabled: Boolean, select: (String) -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    Column {
        Text(label, style = MaterialTheme.typography.labelLarge)
        Box {
            OutlinedButton(onClick = { expanded = true }, enabled = enabled,
                modifier = Modifier.fillMaxWidth().semantics { contentDescription = label }) {
                Text(current, Modifier.weight(1f))
                Text("▾")
            }
            DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
                options.forEach { (name, value) ->
                    DropdownMenuItem(text = { Text(name) }, onClick = { select(value); expanded = false })
                }
            }
        }
    }
}

@Composable
internal fun SettingToggle(label: String, checked: Boolean, enabled: Boolean = true, onChange: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth().heightIn(min = 48.dp), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
        Text(label, Modifier.weight(1f))
        Switch(checked = checked, onCheckedChange = onChange, enabled = enabled, modifier = Modifier.semantics { contentDescription = label })
    }
}

@Composable
private fun FileChoice(label: String, pack: EngineStore.ImportSet, index: Int, optional: Boolean, enabled: Boolean, select: (Int) -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        OutlinedButton(onClick = { expanded = true }, enabled = enabled) { Text("$label：${pack.files.getOrNull(index)?.label ?: "未选择"}") }
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            if (optional) DropdownMenuItem(text = { Text("未引用") }, onClick = { select(-1); expanded = false })
            pack.files.forEachIndexed { i, file -> DropdownMenuItem(text = { Text("${i + 1}. ${file.label}") }, onClick = { select(i); expanded = false }) }
        }
    }
}

@Composable
private fun ApplyStatus(model: SettingsModel) {
    val preferences = EngineUpdates.preferences(model.getApplication())
    var revision by remember { mutableIntStateOf(0) }
    DisposableEffect(preferences) {
        val listener = android.content.SharedPreferences.OnSharedPreferenceChangeListener { _, _ -> revision++ }
        preferences.registerOnSharedPreferenceChangeListener(listener)
        onDispose { preferences.unregisterOnSharedPreferenceChangeListener(listener) }
    }
    val status = remember(revision) { preferences.getString("error", null) ?: if (preferences.getString("requested", "initial") == preferences.getString("applied", null)) "运行时已应用" else "已保存的配置等待输入法启动或重载" }
    Text(status, style = MaterialTheme.typography.bodySmall, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
}

@Composable
private fun PredictionModels(model:SettingsModel, prediction:JSONObject, import:()->Unit) {
    var expanded by rememberSaveable { mutableStateOf(false) }
    OutlinedButton(onClick={expanded=!expanded}) { Text(if(expanded) "收起预测模型与资源" else "预测模型与资源") }
    if(!expanded) return
    LaunchedEffect(expanded) { while(true) { model.refreshModelStatus(); kotlinx.coroutines.delay(2000) } }
    Text(if(model.dirty) "预测设置有未保存更改" else "显示已保存设置；实际运行状态单独查询")
    Text("模型仅在本地运行。基础词典预测无需模型；尚无通过质量验收的推荐模型。")
    Text("本安装包运行库：GGUF ${if(model.modelBackends.optBoolean("llama")) "可用" else "未包含"}；ONNX ${if(model.modelBackends.optBoolean("onnx")) "可用" else "未包含"}")
    OutlinedButton(onClick=model::refreshModelStatus,enabled=!model.busy) {Text("刷新运行状态")}
    Text(model.modelStatus)
    val selected=prediction.optJSONObject("model")
    Text("当前选择："+(selected?.let {"${it.getString("id")} @ ${it.getString("version")}"} ?: "仅词典预测"))
    OutlinedButton(onClick={model.change("prediction","model",JSONObject.NULL)},enabled=!model.busy) {Text("使用词典预测")}
    Row(horizontalArrangement=Arrangement.spacedBy(8.dp)) {
        Button(onClick=import,enabled=!model.busy) {Text("导入模型包目录")}
        OutlinedButton(onClick=model::refreshModels,enabled=!model.busy) {Text("刷新模型")}
    }
    if(model.models.isEmpty()) Text("尚无模型：请导入受支持模型包。不会自动下载或开启预测。")
    if(selected!=null && model.models.none { it.optString("id")==selected.optString("id") && it.optString("version")==selected.optString("version") })
        Text("已选模型缺失或校验失败：请导入同版本或明确选择词典预测，原选择已保留。")
    if(!model.modelBackends.optBoolean("llama") || !model.modelBackends.optBoolean("onnx")) {
        val context=androidx.compose.ui.platform.LocalContext.current
        OutlinedButton(onClick={context.startActivity(Intent(Intent.ACTION_VIEW, android.net.Uri.parse("https://github.com/silevilence/MyWubi/releases")))}) { Text("获取模型版发行包") }
    }
    for(p in model.models) {
        val identity=JSONObject().put("id",p.getString("id")).put("version",p.getString("version"))
        val active=selected?.optString("id")==p.getString("id") && selected?.optString("version")==p.getString("version")
        val caps=p.getJSONObject("capabilities")
        val backendAvailable=model.modelBackends.optBoolean(if(p.getString("backend")=="OnnxRuntime") "onnx" else "llama")
        val hasPolicy=(!p.isNull("prediction_policy") && p.optString("prediction_policy").isNotEmpty()) ||
            (!p.isNull("generation_policy") && p.optString("generation_policy").isNotEmpty())
        val supported=backendAvailable && hasPolicy && (0 until caps.getJSONArray("tasks").length()).any {caps.getJSONArray("tasks").getString(it)=="Predict"}
        val files=p.getJSONArray("files")
        val size=(0 until files.length()).sumOf {files.getJSONObject(it).getLong("bytes")}/1048576.0
        Text("${p.getString("id")} @ ${p.getString("version")} · ${p.getString("backend")} · ${"%.1f".format(size)} MiB")
        Text("许可：${p.getString("license")}\n来源：${p.getString("source")}")
        Text(if(hasPolicy) "已提供预测门槛；不代表效果已通过验收。" else "缺少预测门槛：可保留用于离线评测，暂不能用于字词预测。")
        Text("能力：${caps.getJSONArray("tasks")}；前文 ${caps.getInt("max_context_chars")} 字")
        Row(horizontalArrangement=Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick={model.change("prediction","model",identity)},enabled=!model.busy && supported && !active) {Text("选择此模型")}
            OutlinedButton(onClick={model.removeModel(identity)},enabled=!model.busy && !active) {Text("移除未选模型")}
        }
    }
    SettingToggle("提前加载模型（隐藏或资源紧张时释放）", prediction.optString("loading","on_demand")=="preload_keep", !model.busy) {
        model.change("prediction", "loading", if(it) "preload_keep" else "on_demand")
    }
    for((key,label,range,default) in listOf(
        Budget("wait_ms","最长等待（毫秒）",50..1000,200),
        Budget("threads","CPU 线程",1..4,2),
        Budget("max_candidates","预测候选上限",1..64,16),
        Budget("idle_seconds","空闲卸载（秒）",10..600,60))) {
        val value=prediction.optInt(key,default)
        Text("$label：$value")
        Slider(value=value.toFloat(),onValueChange={model.change("prediction",key,it.toInt())},
            valueRange=range.first.toFloat()..range.last.toFloat(),enabled=!model.busy && (key!="idle_seconds" || prediction.optString("loading")!="preload_keep"))
    }
    Text("执行设备：CPU。提前加载仅在输入法前台允许时生效，不能常驻后台。")
    Text("选择与预算需保存后生效。已保存不代表模型已加载；不可用或超时隐藏本轮预测。支持的包规范见项目文档。")
}
private data class Budget(val key:String,val label:String,val range:IntRange,val default:Int)
