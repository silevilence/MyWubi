package org.mywubi
import org.json.JSONObject
import kotlinx.coroutines.flow.first

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
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
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
@OptIn(ExperimentalMaterial3Api::class)
private fun SettingsScreen(model: SettingsModel, enable: () -> Unit, choose: () -> Unit, leave: () -> Unit) {
    var confirmLeave by remember { mutableStateOf(false) }
    var confirmReload by remember { mutableStateOf(false) }
    var main by rememberSaveable { mutableIntStateOf(-1) }
    var pinyin by rememberSaveable { mutableIntStateOf(-1) }
    var english by rememberSaveable { mutableIntStateOf(-1) }
    var vibration by remember { mutableStateOf(model.ui.getBoolean("vibration", false)) }
    var localPreedit by remember { mutableStateOf(model.ui.getBoolean("local_preedit", false)) }
    var clipboard by remember { mutableStateOf(model.ui.getBoolean("clipboard_suggestion", true)) }
    var section by rememberSaveable { mutableIntStateOf(0) }
    val snackbar = remember { SnackbarHostState() }
    LaunchedEffect(model.message) {
        if (model.message.isNotEmpty()) snackbar.showSnackbar(model.message)
    }
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        if (uris.isNotEmpty()) { main = -1; pinyin = -1; english = -1; model.stage(uris) }
    }
    val modelPicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
        uri?.let(model::importModel)
    }
    BackHandler(model.dirty) { confirmLeave = true }
    Scaffold(modifier = Modifier.safeDrawingPadding().imePadding(),
        snackbarHost = { SnackbarHost(snackbar) },
        topBar = {
            Column {
                Row(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 12.dp),
                    verticalAlignment = androidx.compose.ui.Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Image(painterResource(R.drawable.mywubi_brand), null, Modifier.size(36.dp))
                    Text("MyWubi 设置", style = MaterialTheme.typography.titleLarge)
                }
                ScrollableTabRow(selectedTabIndex = section, edgePadding = 12.dp) {
                    listOf("常用", "键盘", "码表", "预测", "特殊输入").forEachIndexed { index, label ->
                        Tab(selected = section == index, onClick = { section = index }, text = { Text(label) })
                    }
                }
            }
        },
        bottomBar = {
            Surface(tonalElevation = 3.dp) {
                Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 10.dp),
                    verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(if (model.dirty) "有未保存更改 · 草稿会保留" else "没有未保存更改",
                        style = MaterialTheme.typography.bodySmall)
                    ApplyStatus(model)
                    if (model.pack != null) TextButton(onClick = { section = 2 }) { Text("请先完成或取消码表导入") }
                    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        OutlinedButton(onClick = { confirmReload = true }, enabled = !model.busy) { Text("重新载入") }
                        Button(onClick = model::save, enabled = model.dirty && model.pack == null && !model.busy,
                            modifier = Modifier.weight(1f)) { Text("保存设置") }
                    }
                }
            }
        }) { padding ->
        // Each category keeps its position; changing tabs never discards the ViewModel draft.
        val scrollStates = List(5) { rememberScrollState() }
        Column(Modifier.fillMaxSize().padding(padding).then(if(section == 4) Modifier else Modifier.verticalScroll(scrollStates[section]))
            .semantics { contentDescription = "设置内容" }.padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            if (model.busy && !model.trialRunning) LinearProgressIndicator(Modifier.fillMaxWidth())
            if (model.message.isNotEmpty()) Text(model.message, color = MaterialTheme.colorScheme.primary,
                modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
            val config = model.config
            if (section == 4 && config != null) SpecialInputSettings(model, config, Modifier.weight(1f))
            if (section == 0) {
                Text("形码 · 拼音 · 英文", style = MaterialTheme.typography.titleMedium)
                ImeSetup(enable, choose)
                HorizontalDivider()
                Text("输入习惯", style = MaterialTheme.typography.titleLarge)
                Text("修改后，点击底部「保存设置」应用。", style = MaterialTheme.typography.bodySmall)
                if (config == null) Text("正在读取设置…") else {
                    val basic = config.getJSONObject("basic")
                    val dict = config.getJSONObject("dictionary")
                    for ((key, label) in listOf("auto_commit_unique" to "唯一候选自动上屏", "commit_on_max_code_overflow" to "超过最大码长时顶字", "show_code_hints" to "显示候选编码提示")) {
                        SettingToggle(label, basic.getBoolean(key), !model.busy) { model.change("basic", key, it) }
                    }
                    SettingToggle("启用用户词库", dict.getBoolean("enable_user_dict"), !model.busy) { model.change("dictionary", "enable_user_dict", it) }
                    HorizontalDivider()
                    PersonalSettings(model, config)
                }
                OutlinedButton(onClick = { model.getApplication<android.app.Application>().startActivity(Intent(model.getApplication(), UpdateActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }) { Text("应用更新") }
            }
            if (section == 2) {
                Text("码表", style = MaterialTheme.typography.titleLarge)
                Text("当前码表：${model.tableLabel}")
                if (model.pack == null) {
                TableChoice("选择当前码表", model.tableLabel, model.tables.filter { it.main }.map { it.label to it.file.path },
                    model.config != null && !model.busy && model.pack == null) { path ->
                    model.selectTable(model.tables.first { it.file.path == path })
                }
                Text("选择后点击「保存设置」生效；导入只添加码表，不切换当前码表。", style = MaterialTheme.typography.bodySmall)
                Text("把码表放到手机的「下载」目录，再点下方按钮，从系统文件选择器中打开。")
                Button(onClick = { picker.launch(arrayOf("*/*")) }, enabled = model.config != null && !model.busy) { Text("从手机导入码表") }
                SettingsDisclosure("导入格式与多文件说明") {
                    Text("支持 MyWubi UTF-8 文本码表（编码、词条、可选权重，以 Tab 分隔）。不支持直接读取搜狗 .scel、RIME .yaml 等其它格式。单文件自动作为主表；有拼音或英文引用时，长按多选所需文件。", style = MaterialTheme.typography.bodySmall)
                }
                }
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
                if (model.pack == null) OutlinedButton(onClick = { model.getApplication<android.app.Application>().startActivity(Intent(model.getApplication(), LibraryActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }, enabled = !model.busy) { Text("编辑码表与用户词库") }
            }
            if (section == 3) {
                Text("字词预测", style = MaterialTheme.typography.titleLarge)
                if (config == null) Text("正在读取设置…") else {
                    val prediction = config.optJSONObject("prediction") ?: JSONObject().put("enabled", false)
                    SettingToggle("上屏后显示字词预测（实验性）", prediction.optBoolean("enabled"), !model.busy) { model.change("prediction", "enabled", it) }
                    Text("本地词典续接与常用搭配；点击或数字选词，空格选第一项，继续输入编码即忽略。默认关闭，编码候选顺序不变。")
                    Card(Modifier.fillMaxWidth()) {
                        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text("先用词典预测", style = MaterialTheme.typography.titleMedium)
                            Text("无需下载模型。开启后，输入并上屏一个词，候选栏会显示可用的续接词。")
                            Button(onClick = {
                                model.change("prediction", "model", JSONObject.NULL)
                                model.change("prediction", "enabled", true)
                            }, enabled = !model.busy) { Text("选用词典预测") }
                            Text("选用后点击底部「保存设置」。", style = MaterialTheme.typography.bodySmall)
                        }
                    }
                    PredictionModels(model, prediction) { modelPicker.launch(null) }
                }
            }
            if (section == 1) {
                Text("手机键盘", style = MaterialTheme.typography.titleLarge)
                Text("本页开关自动保存，无需点击底部保存。", style = MaterialTheme.typography.bodySmall)
                SettingToggle("按键振动", vibration) { vibration = it; model.ui.edit().putBoolean("vibration", it).apply() }
                SettingToggle("唤起键盘时显示剪贴板建议", clipboard) { clipboard = it; model.ui.edit().putBoolean("clipboard_suggestion", it).apply() }
                Text("点击建议即可粘贴，开始输入或关闭建议后隐藏。密码框和标记为敏感的剪贴板内容不显示预览。", style = MaterialTheme.typography.bodySmall)
                SettingToggle("仅在候选栏显示预编辑", localPreedit) { localPreedit = it; model.ui.edit().putBoolean("local_preedit", it).apply() }
                Text("预编辑兼容模式在下次进入输入框时生效。", style = MaterialTheme.typography.bodySmall)
                HorizontalDivider()
                Text("删除手势", style = MaterialTheme.typography.titleMedium)
                Text("轻点删除一个字；长按连续删除；长按后向上滑，看到「松手清空」再松手，可清空当前输入框。滑回按键可取消清空。")
            }
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
    Row(Modifier.fillMaxWidth().heightIn(min = 56.dp)
        .toggleable(value = checked, enabled = enabled, role = Role.Switch, onValueChange = onChange)
        .semantics { contentDescription = label },
        horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
        Text(label, Modifier.weight(1f))
        Switch(checked = checked, onCheckedChange = null, enabled = enabled)
    }
}

@Composable
private fun ImeSetup(enable: () -> Unit, choose: () -> Unit) {
    val context = androidx.compose.ui.platform.LocalContext.current
    val lifecycle = androidx.lifecycle.compose.LocalLifecycleOwner.current.lifecycle
    var enabled by remember { mutableStateOf(false) }
    var selected by remember { mutableStateOf(false) }
    DisposableEffect(lifecycle) {
        fun refresh() {
            enabled = context.getSystemService(InputMethodManager::class.java).enabledInputMethodList.any { it.packageName == context.packageName }
            selected = Settings.Secure.getString(context.contentResolver, Settings.Secure.DEFAULT_INPUT_METHOD)
                ?.substringBefore('/') == context.packageName
        }
        refresh()
        val observer = androidx.lifecycle.LifecycleEventObserver { _, event ->
            if (event == androidx.lifecycle.Lifecycle.Event.ON_RESUME) refresh()
        }
        lifecycle.addObserver(observer)
        onDispose { lifecycle.removeObserver(observer) }
    }
    OutlinedCard(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(if (selected) "MyWubi 已就绪" else "开始使用", style = MaterialTheme.typography.titleMedium)
            Text(if (selected) "当前码表可在「码表」页切换。" else if (enabled) "已启用，选择 MyWubi 后即可输入。" else "先在系统设置中启用，再选择 MyWubi。")
            if (!enabled) Button(onClick = enable) { Text("1. 启用 MyWubi") }
            OutlinedButton(onClick = choose, enabled = enabled) { Text(if (selected) "切换输入法" else "2. 选择输入法") }
        }
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
    Text(if (model.dirty) "已保存版本：$status" else status, style = MaterialTheme.typography.bodySmall,
        modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
}

@Composable
private fun PredictionModels(model:SettingsModel, prediction:JSONObject, import:()->Unit) {
    var expanded by rememberSaveable { mutableStateOf(false) }
    OutlinedButton(onClick={expanded=!expanded}) { Text(if(expanded) "收起预测模型与资源（实验性）" else "预测模型与资源（实验性）") }
    if(!expanded) return
    LaunchedEffect(expanded) { while(true) { model.refreshModelStatus(); kotlinx.coroutines.delay(2000) } }
    Text("模型仅在本地运行。基础词典预测无需模型；尚无通过质量验收的推荐模型。")
    if (!model.modelBackends.optBoolean("llama") && !model.modelBackends.optBoolean("onnx")) {
        Card(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("当前版本仅支持词典预测", style = MaterialTheme.typography.titleMedium)
                Text("你没有漏掉操作。这个安装包未包含模型运行组件，下载模型也无法启用。可先使用上方的词典预测；如需测试模型，需安装自行构建的实验性模型版。")
                ModelBuildGuide()
            }
        }
    }
    SettingsDisclosure("下载模型") { DownloadModels(model) }
    SettingsDisclosure("导入与选择模型") {
        LaunchedEffect(Unit) {
            snapshotFlow { model.busy }.first { !it }
            model.ensureModelsLoaded()
        }
        val selected=prediction.optJSONObject("model")
        Text("当前选择："+(selected?.let {"${it.getString("id")} @ ${it.getString("version")}"} ?: "仅词典预测"))
        OutlinedButton(onClick={model.change("prediction","model",JSONObject.NULL)},enabled=!model.busy) {Text("使用词典预测")}
        Row(horizontalArrangement=Arrangement.spacedBy(8.dp)) {
            Button(onClick=import,enabled=!model.busy) {Text("导入模型包目录")}
            OutlinedButton(onClick=model::refreshModels,enabled=!model.busy) {Text("刷新模型")}
        }
        if (!model.modelsLoaded) Text(if (model.busy) "正在校验模型包，完成后可选择。" else "尚未完成模型校验，请点击「刷新模型」重试。")
        else if(model.models.isEmpty()) Text("尚无模型：请导入受支持模型包。不会自动下载或开启预测。")
        if(model.modelsLoaded && selected!=null && model.models.none { it.optString("id")==selected.optString("id") && it.optString("version")==selected.optString("version") })
            Text("已选模型缺失或校验失败：请导入同版本或明确选择词典预测，原选择已保留。")
        if(!model.modelBackends.optBoolean("llama") || !model.modelBackends.optBoolean("onnx")) {
            ModelBuildGuide()
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
                OutlinedButton(onClick={model.change("prediction","model",identity)},enabled=!model.busy && supported && !active,
                    modifier=Modifier.semantics {contentDescription="选择模型 ${p.getString("id")} @ ${p.getString("version")}"}) {Text("选择此模型")}
                OutlinedButton(onClick={model.removeModel(identity)},enabled=!model.busy && !active) {Text("移除未选模型")}
            }
        }
    }
    SettingsDisclosure("运行状态与性能") {
        Text("本安装包运行库：GGUF ${if(model.modelBackends.optBoolean("llama")) "可用" else "未包含"}；ONNX ${if(model.modelBackends.optBoolean("onnx")) "可用" else "未包含"}")
        OutlinedButton(onClick=model::refreshModelStatus,enabled=!model.busy) {Text("刷新运行状态")}
        Text(model.modelStatus)
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
        Text("请求设备：${prediction.optString("device", "cpu")}；Android 当前仅支持 CPU，桌面 GPU 配置在此回退 CPU。")
        OutlinedButton(onClick={model.change("prediction","device","cpu"); model.change("prediction","device_name","")}, enabled=!model.busy) {Text("使用 CPU")}
        Text("提前加载仅在输入法前台允许时生效，不能常驻后台。")
        Text("选择与预算需保存后生效。已保存不代表模型已加载；不可用或超时隐藏本轮预测。支持的包规范见项目文档。")
    }
    ModelTrial(model)
}

@Composable
internal fun SettingsDisclosure(title: String, content: @Composable ColumnScope.() -> Unit) {
    var expanded by rememberSaveable { mutableStateOf(false) }
    OutlinedCard(Modifier.fillMaxWidth()) {
        TextButton(onClick = { expanded = !expanded }, modifier = Modifier.fillMaxWidth()) {
            Text(title, Modifier.weight(1f), style = MaterialTheme.typography.titleMedium)
            Text(if (expanded) "收起" else "展开")
        }
        if (expanded) Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp), content = content)
    }
}

@Composable
internal fun ModelBuildGuide() {
    val context = androidx.compose.ui.platform.LocalContext.current
    var failed by remember { mutableStateOf(false) }
    OutlinedButton(onClick = {
        failed = runCatching { context.startActivity(Intent(Intent.ACTION_VIEW,
            android.net.Uri.parse("https://github.com/silevilence/MyWubi/blob/master/docs/model-input-integration.md"))) }.isFailure
    }) { Text("查看实验性模型版构建说明") }
    if (failed) Text("无法打开浏览器。请到项目 GitHub 的 docs/model-input-integration.md 查看构建说明。")
}
private data class Budget(val key:String,val label:String,val range:IntRange,val default:Int)


@Composable
fun ExpressionSettings(model: SettingsModel, config: JSONObject, selectedKind: String) {
    var importing by rememberSaveable { mutableStateOf("emoji") }
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri?.let { model.importExpression(importing, it) }
    }
    Text("特殊码表", style = MaterialTheme.typography.titleMedium)
    if (model.expressionError.isNotEmpty()) Text(model.expressionError, color = MaterialTheme.colorScheme.error)
    for ((kind, label) in listOf("emoji" to "Emoji", "kaomoji" to "颜文字", "kana" to "假名", "math" to "数学符号", "punctuation" to "标点", "symbols" to "其他符号")) {
        if(kind != selectedKind) continue
        val selection = config.optJSONObject("expressions")?.optJSONObject(kind) ?: JSONObject()
        fun change(key: String, value: Any) {
            model.change("expressions", kind, JSONObject(selection.toString()).put(key, value))
        }
        val entries = model.expressionResources[kind].orEmpty()
        val resource = if (selection.isNull("resource")) null else selection.optString("resource")
        val current = entries.firstOrNull { info ->
            if (resource == null) info.optBoolean("builtin")
            else java.io.File(info.getString("path")) == java.io.File(resource).let {
                if (it.isAbsolute) it else java.io.File(model.store.configFile.parentFile, resource)
            }
        }
        Text(label, style = MaterialTheme.typography.titleMedium)
        SettingToggle("普通输入时追加", selection.optBoolean("append"), !model.busy) { change("append", it) }
        SettingToggle("专用模式可用", selection.optBoolean("search"), !model.busy) { change("search", it) }
        TableChoice("选择${label}词库", current?.optString("name") ?: "尚未检查所选词库",
            entries.map { info ->
                val prefix = if (info.optBoolean("builtin")) "内置 · " else ""
                val error = if (!info.isNull("error")) "（不可用）" else ""
                "$prefix${info.getString("name")}$error" to info.getString("path")
            }, !model.busy && entries.isNotEmpty()) { path ->
            model.selectExpression(kind, entries.first { it.getString("path") == path })
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = { importing = kind; picker.launch(arrayOf("*/*")) }, enabled = !model.busy) { Text("导入词库") }
            TextButton(onClick = { model.selectExpression(kind, null) }, enabled = !model.busy) { Text("使用内置词库") }
        }
        if (current != null) {
            if (!current.isNull("error")) Text(current.getString("error"), color = MaterialTheme.colorScheme.error)
            else {
                Text("校验通过 · ${current.getInt("entries")} 条 · 版本 ${current.getString("version")}")
                Text(if (selection.optBoolean("append") || selection.optBoolean("search"))
                    "保存后由输入法加载；当前宿主是否已采用请查看底部应用状态。" else "两个用途均关闭，输入法不加载此词库。",
                    style = MaterialTheme.typography.bodySmall)
                SettingsDisclosure("${label}来源与许可") {
                    val sources = current.getJSONArray("sources")
                    for (index in 0 until sources.length()) {
                        val source = sources.getJSONObject(index)
                        Text("${source.getString("id")} · ${source.getString("revision")} · ${source.getString("license")}")
                        Text(source.getString("url"), style = MaterialTheme.typography.bodySmall)
                        SettingsDisclosure("完整声明：${source.getString("id")}") { Text(source.getString("notice")) }
                    }
                }
            }
        }
        HorizontalDivider()
    }
    TextButton(onClick = model::refreshExpressions, enabled = !model.busy) { Text("重新检查词库") }
    ExpressionPreview()
    Text("配置独立于主码表。导入会复制到应用私有目录，选择和开关需保存后生效。点击键盘工具栏的“特殊输入”，选择类别后可用主码表组词或直接输入别名查找。", style = MaterialTheme.typography.bodySmall)
}
