package org.mywubi

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.*
import androidx.activity.result.contract.ActivityResultContracts
import androidx.lifecycle.ViewModelProvider
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.unit.dp

class LibraryActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val model = ViewModelProvider(this)[LibraryModel::class.java]
        setContent { MaterialTheme { LibraryScreen(model) { finish() } } }
    }
}

@Composable
private fun LibraryScreen(model: LibraryModel, leave: () -> Unit) {
    var filter by rememberSaveable { mutableStateOf("") }
    var confirmLeave by remember { mutableStateOf(false) }
    var confirmReload by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }
    val importer = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { it?.let { uri -> model.transfer(uri, true) } }
    val exporter = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("text/plain")) { it?.let { uri -> model.transfer(uri, false) } }
    BackHandler(model.dirty || model.busy) { confirmLeave = true }
    val entries = model.data.optJSONArray("entries")
    val matches = remember(model.data, filter) { if(entries == null) emptyList() else (0 until entries.length()).filter { i -> entries.getJSONObject(i).let { filter.isEmpty() || it.getString("code").contains(filter) || it.getString("word").contains(filter) } } }
    Surface {
        LazyColumn(Modifier.fillMaxSize().safeDrawingPadding().imePadding().padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(12.dp), contentPadding = PaddingValues(vertical = 20.dp)) {
            item {
                Text("码表与用户词库", style = MaterialTheme.typography.headlineMedium)
                Text("手动维护词条与权重；不包含自动学习或拼音学习。")
            }
            items(model.paths.indices.toList()) { index ->
                OutlinedButton(onClick = { model.open(index) }, enabled = !model.dirty && !model.busy) { Text(model.paths[index].first) }
            }
            if (model.busy) item { LinearProgressIndicator(Modifier.fillMaxWidth()) }
            if (model.message.isNotEmpty()) item { Text(model.message, color = MaterialTheme.colorScheme.primary, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }) }
            if (model.selected >= 0 && model.form.has("code")) {
                item {
                    Text(model.paths[model.selected].first, style = MaterialTheme.typography.titleLarge)
                    Text(if(model.dirty) "有未保存草稿；保存或重新载入后可切换词条" else "点击词条编辑，或新增词条")
                    OutlinedButton(onClick = { confirmReload = true }, enabled = !model.busy) { Text("重新载入并放弃草稿") }
                }
                if (!model.user) item {
                    val header = model.form.getJSONObject("header")
                    Text("码表元数据", style = MaterialTheme.typography.titleMedium)
                    Row {
                        for ((value, label) in listOf("shape" to "形码", "pinyin" to "拼音")) TextButton(onClick = { model.change("type", value, "header") }, enabled = !model.busy) { Text(if(header.getString("type") == value) "$label · 已选" else label) }
                    }
                    LibraryField("编码字符集", header.getString("charset"), !model.busy) { model.change("charset", it, "header") }
                    LibraryField("最大码长", header.get("max_code_len").toString(), !model.busy) { model.change("max_code_len", it, "header") }
                    LibraryField("万能键（空为关闭）", if(header.isNull("wildcard_key")) "" else header.getString("wildcard_key"), !model.busy) { model.change("wildcard_key", it, "header") }
                    val temporary = header.getJSONObject("temporary")
                    LibraryField("桌面临时入口键", temporary.getString("key"), !model.busy) { model.change("key", it, "temporary") }
                    LibraryField("拼音引用（相对当前码表）", temporary.getString("pinyin_table"), !model.busy) { model.change("pinyin_table", it, "temporary") }
                    LibraryField("英文引用（相对当前码表）", temporary.getString("english_table"), !model.busy) { model.change("english_table", it, "temporary") }
                    SettingToggle("显示主码反查", temporary.getBoolean("reverse_lookup"), !model.busy) { model.change("reverse_lookup", it, "temporary") }
                    Text("拼音编码使用 a～z，音节以单引号分隔；最大码长不含分隔符。保存使用共享校验，类型切换不会自动改写词条。")
                }
                item {
                    Text(if(model.form.optInt("index", -1) < 0) "新增词条" else "编辑词条", style = MaterialTheme.typography.titleMedium)
                    for ((key, label) in listOf("code" to "编码", "word" to "词条", "weight" to "权重")) {
                        LibraryField(label, model.form.getString(key), !model.busy) { model.change(key, it) }
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Button(onClick = { model.save() }, enabled = model.dirty && !model.busy) { Text("保存更改") }
                        OutlinedButton(onClick = { confirmDelete = true }, enabled = model.form.optInt("index", -1) >= 0 && !model.busy) { Text("删除词条") }
                    }
                    OutlinedButton(onClick = { model.choose(-1) }, enabled = !model.dirty && !model.busy) { Text("新增词条") }
                }
                if (model.user) item {
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedButton(onClick = { importer.launch(arrayOf("*/*")) }, enabled = !model.dirty && !model.busy) { Text("导入用户词库") }
                        OutlinedButton(onClick = { exporter.launch("mywubi-user.dict") }, enabled = !model.dirty && !model.busy) { Text("导出用户词库") }
                    }
                }
                item { LibraryField("查找编码或词条", filter, true) { filter = it } }
                item { Text("${matches.size} 条匹配；显示前 200 条，可输入关键词缩小范围") }
                items(matches.take(200), key = { it }) { index ->
                    val entry = entries!!.getJSONObject(index)
                    OutlinedButton(onClick = { model.choose(index) }, enabled = !model.dirty && !model.busy, modifier = Modifier.fillMaxWidth()) {
                        Text("${entry.getString("code")}  ${entry.getString("word")}  · ${entry.get("weight")}")
                    }
                }
            }
        }
    }
    if(confirmLeave) AlertDialog(onDismissRequest = { confirmLeave = false }, title = { Text("保留草稿并退出？") }, text = { Text("下次打开同一码表可恢复草稿。") }, confirmButton = { TextButton(onClick = leave, enabled = !model.busy) { Text("保留并退出") } }, dismissButton = { TextButton(onClick = { confirmLeave = false }) { Text("继续编辑") } })
    if(confirmReload) AlertDialog(onDismissRequest = { confirmReload = false }, title = { Text("放弃草稿并重新载入？") }, confirmButton = { TextButton(onClick = { confirmReload = false; model.open(model.selected, true) }) { Text("重新载入") } }, dismissButton = { TextButton(onClick = { confirmReload = false }) { Text("取消") } })
    if(confirmDelete) AlertDialog(onDismissRequest = { confirmDelete = false }, title = { Text("删除所选词条？") }, confirmButton = { TextButton(onClick = { confirmDelete = false; model.save(true) }) { Text("删除") } }, dismissButton = { TextButton(onClick = { confirmDelete = false }) { Text("取消") } })
}

@Composable
private fun LibraryField(label: String, value: String, enabled: Boolean, change: (String) -> Unit) {
    OutlinedTextField(value = value, onValueChange = change, enabled = enabled, label = { Text(label) }, modifier = Modifier.fillMaxWidth(), singleLine = true)
}
