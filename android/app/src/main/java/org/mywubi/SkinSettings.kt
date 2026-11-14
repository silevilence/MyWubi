package org.mywubi

import android.content.Intent
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.content.FileProvider
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.UUID

/** The library imports immutable versions. Selection changes only the persisted settings draft. */
@Composable
internal fun SkinManagement(model: SettingsModel, config: JSONObject) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var items by remember { mutableStateOf(JSONArray()) }
    var busy by remember { mutableStateOf(false) }
    var message by remember { mutableStateOf("") }
    var preview by remember { mutableStateOf<JSONObject?>(null) }
    val preference = config.getJSONObject("skins").getJSONObject("android")
    fun request(action: String) = JSONObject().put("action", action).put("path", model.store.configFile.path)
    suspend fun refresh() {
        items = withContext(Dispatchers.IO) {
            NativeBridge.objectCall("ManageSkin", "request" to request("List").put("platform", "android")).getJSONArray("items")
        }
    }
    LaunchedEffect(Unit) { runCatching { refresh() }.onFailure { message = it.message ?: "读取皮肤库失败" } }
    // A preview must never keep displaying a previously selected draft after a failed resolution.
    LaunchedEffect(preference.toString()) { preview = null }
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) scope.launch {
            busy = true
            runCatching {
                withContext(Dispatchers.IO) {
                    val temporary = File.createTempFile("skin-import-", ".zip", context.cacheDir)
                    try {
                        context.contentResolver.openInputStream(uri).use { input ->
                            requireNotNull(input) { "无法读取所选文件" }
                            temporary.outputStream().use { output ->
                                val buffer = ByteArray(65536)
                                var total = 0L
                                while (true) {
                                    val count = input.read(buffer)
                                    if (count < 0) break
                                    total += count
                                    require(total <= 64L * 1024 * 1024) { "皮肤包超过 64 MiB" }
                                    output.write(buffer, 0, count)
                                }
                            }
                        }
                        NativeBridge.objectCall("ManageSkin", "request" to request("Import")
                            .put("source", temporary.path).put("platform", "android"))
                    } finally { temporary.delete() }
                }
                refresh()
                message = "已导入皮肤库。选择 Android 组件后预览并保存设置。"
            }.onFailure { message = it.message ?: "导入失败，当前皮肤未改变" }
            busy = false
        }
    }
    HorizontalDivider()
    Text("皮肤包", style = MaterialTheme.typography.titleMedium)
    Text("导入只添加到皮肤库；选择后为草稿，点击底部「保存设置」应用。", style = MaterialTheme.typography.bodySmall)
    val selection = preference.optJSONObject("selection")
    Text(if (selection == null) "草稿选择：内置方案" else "草稿选择：${selection.optString("id")} @ ${selection.optString("version")} / ${selection.optString("component")}")
    if (message.isNotBlank()) Text(message, modifier = Modifier.fillMaxWidth())
    if (busy) LinearProgressIndicator(Modifier.fillMaxWidth())
    OutlinedButton(onClick = { picker.launch(arrayOf("*/*")) }, enabled = !busy && !model.busy) { Text("从文件导入皮肤包") }
    for (index in 0 until items.length()) {
        val item = items.getJSONObject(index)
        Card(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text("${item.getString("name")} · ${item.optString("version")}", style = MaterialTheme.typography.titleSmall)
                Text("已导入 · 支持 ${item.optJSONArray("platforms")?.let { a -> (0 until a.length()).joinToString(" / ") { a.getString(it) } } ?: "未知"}")
                if (!item.isNull("reason")) Text(item.getString("reason"))
                val components = item.getJSONArray("components")
                for (c in 0 until components.length()) {
                    val component = components.getJSONObject(c)
                    TextButton(onClick = {
                        val selected = JSONObject().put("id", item.getString("id")).put("version", item.getString("version")).put("component", component.getString("id"))
                        model.change("skins", "android", JSONObject(preference.toString()).put("selection", selected))
                    }, modifier=Modifier.semantics { contentDescription="选用皮肤 ${item.getString("name")}，组件 ${component.getString("id")}" }, enabled = component.getBoolean("enabled") && !busy && !model.busy) { Text("选用 ${component.getString("id")}") }
                    if (!component.isNull("reason")) Text(component.getString("reason"), style = MaterialTheme.typography.bodySmall)
                }
                OutlinedButton(onClick = {
                    scope.launch {
                        busy = true
                        runCatching {
                            val output = withContext(Dispatchers.IO) {
                                val directory = File(context.cacheDir, "skin-exports").apply { mkdirs() }
                                val target = File(directory, "${item.getString("id")}-${item.getString("version")}-${UUID.randomUUID()}.mywubi-skin")
                                val selected = JSONObject().put("id", item.getString("id")).put("version", item.getString("version")).put("component", "export")
                                NativeBridge.objectCall("ManageSkin", "request" to request("Export").put("selection", selected).put("target", target.path))
                                target
                            }
                            val uri = FileProvider.getUriForFile(context, "${context.packageName}.skins", output)
                            context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("application/zip")
                                .putExtra(Intent.EXTRA_STREAM, uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION), "分享完整皮肤包"))
                            message = "已打开系统分享。导出保留两个平台及共享资源。"
                        }.onFailure { message = it.message ?: "导出失败" }
                        busy = false
                    }
                }, modifier=Modifier.semantics { contentDescription="分享皮肤 ${item.getString("name")} 完整包" }, enabled = item.has("id") && !busy && !model.busy) { Text("导出并分享完整包") }
            }
        }
    }
    Button(onClick = {
        val draft = JSONObject(config.toString())
        scope.launch {
            busy = true
            preview = null
            runCatching { withContext(Dispatchers.IO) { loadSkinAppearance(model.store.configFile, draft) } }
                .onSuccess { preview = it; message = "原生预览使用固定样例，触摸不会发送输入。" }
                .onFailure { message = it.message ?: "预览失败，当前皮肤未改变" }
            busy = false
        }
    }, enabled = !busy && !model.busy) { Text("预览当前草稿") }
    preview?.let { appearance ->
        Text("Android 原生键盘预览", style = MaterialTheme.typography.titleSmall)
        Keyboard(KeyboardModel(skin = appearance, enabled = true, modesAvailable = true,
            spelling = "a", status = "预览", total = 3,
            candidates = listOf(CandidateItem(1, "工", "aaaa"), CandidateItem(2, "长候选与反查", "abc"), CandidateItem(3, "👩🏽‍💻", "emoji"))),
            dispatch = {}, window = {}, switchIme = {})
    }
}
