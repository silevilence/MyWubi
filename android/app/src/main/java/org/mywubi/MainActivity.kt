package org.mywubi

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
        setContent { MaterialTheme { SettingsScreen(model,
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
    BackHandler(model.dirty || model.busy) { confirmLeave = true }
    Surface {
        Column(Modifier.fillMaxSize().safeDrawingPadding().imePadding().verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text("MyWubi", style = MaterialTheme.typography.headlineLarge)
            Text("形码 · 拼音 · 英文", style = MaterialTheme.typography.titleMedium)
            Text("内置五笔为开发示例表，可导入自己的完整码表。")
            Button(onClick = enable) { Text("1. 启用 MyWubi") }
            OutlinedButton(onClick = choose) { Text("2. 选择输入法") }
            HorizontalDivider()
            Text("输入设置", style = MaterialTheme.typography.titleLarge)
            val config = model.config
            if (config == null) Text("正在读取设置…") else {
                val basic = config.getJSONObject("basic")
                val dict = config.getJSONObject("dictionary")
                for ((key, label) in listOf("auto_commit_unique" to "唯一候选自动上屏", "commit_on_max_code_overflow" to "超过最大码长时顶字", "show_code_hints" to "显示候选编码提示")) {
                    SettingToggle(label, basic.getBoolean(key), !model.busy) { model.change("basic", key, it) }
                }
                SettingToggle("启用用户词库", dict.getBoolean("enable_user_dict"), !model.busy) { model.change("dictionary", "enable_user_dict", it) }
                Text("当前码表：${dict.getString("system_table").substringAfterLast('/')}")
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Button(onClick = model::save, enabled = model.dirty && !model.busy) { Text("保存设置") }
                    OutlinedButton(onClick = { confirmReload = true }, enabled = !model.busy) { Text("重新载入") }
                }
            }
            Text(if (model.dirty) "有未保存更改 · 草稿会保留" else "没有未保存更改")
            if (model.busy) LinearProgressIndicator(Modifier.fillMaxWidth())
            if (model.message.isNotEmpty()) Text(model.message, color = MaterialTheme.colorScheme.primary, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
            ApplyStatus(model)
            HorizontalDivider()
            Text("手机键盘", style = MaterialTheme.typography.titleLarge)
            SettingToggle("按键振动", vibration) { vibration = it; model.ui.edit().putBoolean("vibration", it).apply() }
            SettingToggle("仅在候选栏显示预编辑", localPreedit) { localPreedit = it; model.ui.edit().putBoolean("local_preedit", it).apply() }
            Text("预编辑兼容模式在下次进入输入框时生效；当前输入不会重放。手机设置单独保存。")
            OutlinedButton(onClick = { model.getApplication<android.app.Application>().startActivity(Intent(model.getApplication(), LibraryActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }, enabled = !model.busy) { Text("编辑码表与用户词库") }
            HorizontalDivider()
            Text("导入码表包", style = MaterialTheme.typography.titleLarge)
            Text("选择主表及直接引用的文件，再明确指定角色。同名文件不会自动匹配；原码表会保留。")
            OutlinedButton(onClick = { picker.launch(arrayOf("*/*")) }, enabled = !model.busy) { Text("选择码表文件") }
            model.pack?.let { pack ->
                FileChoice("主码表（必选）", pack, main, false, !model.busy) { main = it }
                FileChoice("拼音辅助表", pack, pinyin, true, !model.busy) { pinyin = it }
                FileChoice("英文辅助表", pack, english, true, !model.busy) { english = it }
                Button(onClick = { model.activate(main, pinyin.takeIf { it >= 0 }, english.takeIf { it >= 0 }) }, enabled = main >= 0 && !model.busy) { Text("校验并启用码表包") }
            }
            Spacer(Modifier.height(24.dp))
        }
    }
    if (confirmLeave) AlertDialog(onDismissRequest = { confirmLeave = false }, title = { Text("保留草稿并退出？") }, text = { Text("更改尚未保存到引擎配置，下次打开可继续编辑。") }, confirmButton = { TextButton(onClick = leave, enabled = !model.busy) { Text("保留并退出") } }, dismissButton = { TextButton(onClick = { confirmLeave = false }) { Text("继续编辑") } })
    if (confirmReload) AlertDialog(onDismissRequest = { confirmReload = false }, title = { Text("重新载入磁盘设置？") }, text = { Text("这会放弃当前设置草稿和待启用的导入选择，已保存数据不受影响。") }, confirmButton = { TextButton(onClick = { confirmReload = false; model.reload(true) }) { Text("重新载入") } }, dismissButton = { TextButton(onClick = { confirmReload = false }) { Text("保留草稿") } })
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
