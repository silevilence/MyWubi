package org.mywubi

import android.content.Intent
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.rememberScrollState
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
import java.io.InputStream
import java.util.UUID
import java.util.zip.ZipFile

internal fun importSkinDocument(config: File, cacheDir: File, input: InputStream): JSONObject {
    val temporary = File.createTempFile("skin-import-", ".zip", cacheDir)
    return try {
        temporary.outputStream().use { output ->
            val buffer = ByteArray(65536)
            var total = 0L
            while (true) {
                val count = input.read(buffer)
                if (count < 0) break
                total += count
                require(total <= 64L * 1024 * 1024) {
                    "所选文件超过 64 MiB。如果选了整套下载包，请先解压，再选择里面的 .mywubi-skin 文件。"
                }
                output.write(buffer, 0, count)
            }
        }

        try {
            NativeBridge.objectCall("ManageSkin", "request" to JSONObject().put("action", "Import").put("path", config.path)
                .put("source", temporary.path).put("platform", "android"))
        } catch (error: ProtocolException) {
            val isBundle = runCatching {
                ZipFile(temporary).use { zip ->
                    zip.getEntry("metadata.toml") == null && zip.entries().asSequence().any {
                        !it.isDirectory && it.name.endsWith(".mywubi-skin", ignoreCase = true)
                    }
                }
            }.getOrDefault(false)
            if (isBundle) throw IllegalArgumentException("选中的是整套下载包。请先解压，再选择里面的 .mywubi-skin 文件。", error)
            throw error
        }
    } finally { temporary.delete() }
}

internal data class SkinOption(val name: String, val preset: String? = null, val selection: KeyboardSelection? = null) {
    fun matches(preference: JSONObject?): Boolean {
        if (preference == null) return false
        val selected = preference.optJSONObject("selection")?.let(KeyboardSelection::decode)
        return if (selection != null) selected == selection else selected == null && preference.optString("preset") == preset
    }

    fun inConfig(config: JSONObject): JSONObject = JSONObject(config.toString()).also {
        val skin = it.getJSONObject("skins").getJSONObject("android")
        if (selection == null) {
            skin.remove("selection")
            skin.put("preset", requireNotNull(preset))
        } else skin.put("selection", selection.json())
    }
}

private val builtinSkins = listOf(SkinOption("经典", "legacy"), SkinOption("墨绿", "jade"),
    SkinOption("素简", "plain"), SkinOption("高对比", "contrast"))

@Composable
private fun SkinActions(option: SkinOption, model: SettingsModel, enabled: Boolean,
                        preview: (SkinOption) -> Unit) {
    val current = option.matches(model.savedAndroidSkin)
    val draftMatches = option.matches(model.config?.getJSONObject("skins")?.getJSONObject("android"))
    Row(Modifier.fillMaxWidth(), verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
        Text(option.name, Modifier.weight(1f), style = MaterialTheme.typography.titleSmall)
        TextButton(onClick = { preview(option) }, enabled = enabled,
            modifier = Modifier.semantics { contentDescription = "预览「${option.name}」" }) { Text("预览") }
        TextButton(onClick = { model.enableSkin(option) }, enabled = enabled && (!current || !draftMatches),
            modifier = Modifier.semantics { contentDescription = if (current && draftMatches) "正在使用「${option.name}」" else "启用「${option.name}」" }) {
            Text(if (current && draftMatches) "使用中" else "启用")
        }
    }
}

@Composable
internal fun SkinManagement(model: SettingsModel, config: JSONObject, onLibraryChange: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var items by remember { mutableStateOf(JSONArray()) }
    var loaded by remember { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    var message by remember { mutableStateOf("") }
    var preview by remember { mutableStateOf<SkinOption?>(null) }
    var deleting by remember { mutableStateOf<JSONObject?>(null) }
    val enabled = !busy && !model.busy
    fun request(action: String) = JSONObject().put("action", action).put("path", model.store.configFile.path)
    suspend fun refresh() {
        items = withContext(Dispatchers.IO) {
            NativeBridge.objectCall("ManageSkin", "request" to request("List").put("platform", "android")).getJSONArray("items")
        }
        loaded = true
    }
    LaunchedEffect(Unit) { runCatching { refresh() }.onFailure { message = it.message ?: "读取皮肤库失败" } }
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) scope.launch {
            busy = true
            runCatching {
                val result = withContext(Dispatchers.IO) {
                    context.contentResolver.openInputStream(uri).use { input ->
                        importSkinDocument(model.store.configFile, context.cacheDir, requireNotNull(input) { "无法读取所选文件" })
                    }
                }
                refresh()
                onLibraryChange()
                val item = result.getJSONObject("item")
                val name = item.getString("name")
                val components = item.getJSONArray("components")
                val hasAppearance = (0 until components.length()).any {
                    val component = components.getJSONObject(it)
                    component.optString("kind") == "appearance" && component.getBoolean("enabled")
                }
                val summary = if (result.getBoolean("already_present")) "「$name」已在我的皮肤中。" else "已导入「$name」。"
                message = if (hasAppearance) summary else summary + "这个版本没有可启用的安卓外观。"
            }.onFailure { message = "此次导入失败。${it.message.orEmpty()}" }
            busy = false
        }
    }
    Row(Modifier.fillMaxWidth(), verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
        Text("我的皮肤", Modifier.weight(1f), style = MaterialTheme.typography.titleLarge)
        OutlinedButton(onClick = { picker.launch(arrayOf("*/*")) }, enabled = enabled) { Text("导入皮肤") }
    }
    if (message.isNotBlank()) Text(message)
    if (busy) LinearProgressIndicator(Modifier.fillMaxWidth())
    Text("已导入", style = MaterialTheme.typography.titleMedium)
    if (!loaded && message.isEmpty()) LinearProgressIndicator(Modifier.fillMaxWidth())
    if (loaded && items.length() == 0 && !busy) Text("还没有导入皮肤。选择 .mywubi-skin 文件即可添加。")
    for (index in 0 until items.length()) {
        val item = items.getJSONObject(index)
        val name = item.getString("name")
        val version = item.optString("version")
        val title = if (version.isEmpty()) name else "$name · $version"
        Card(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(horizontal = 12.dp, vertical = 6.dp)) {
                val entries = item.getJSONArray("components")
                val components = (0 until entries.length()).map(entries::getJSONObject)
                val appearances = components.filter { it.optString("kind") == "appearance" }
                if (appearances.size != 1) Text(title, style = MaterialTheme.typography.titleSmall)
                for (appearance in appearances) {
                    val id = appearance.getString("id")
                    val option = SkinOption(if (appearances.size == 1) title else "$title / $id",
                        selection = KeyboardSelection(item.getString("id"), version, id))
                    SkinActions(option, model, enabled && appearance.getBoolean("enabled")) { preview = it }
                    if (!appearance.isNull("reason")) Text(appearance.getString("reason"))
                }
                if (!item.isNull("reason")) Text(item.getString("reason"))
                for (component in components.filter { it.optString("kind") !in setOf("layout", "mapping", "appearance") }) {
                    if (!component.isNull("reason")) Text(component.getString("reason"))
                }
                val layouts = components.filter { it.optString("kind") == "layout" }
                if (layouts.isNotEmpty()) Text("键盘布局：${layouts.joinToString("、") { it.getString("id") }}",
                    style = MaterialTheme.typography.bodySmall)
                val mappings = components.filter { it.optString("kind") == "mapping" }
                if (mappings.isNotEmpty()) Text("字母映射：${mappings.joinToString("、") { it.getString("id") }}",
                    style = MaterialTheme.typography.bodySmall)
                Row {
                    TextButton(enabled = enabled && item.has("id") && !item.optBoolean("unreadable"), onClick = {
                        scope.launch {
                            busy = true
                            runCatching {
                                val target = withContext(Dispatchers.IO) {
                                    val dir = File(context.cacheDir, "skin-exports").apply { mkdirs() }
                                    val file = File(dir, "${item.getString("id")}-$version-${UUID.randomUUID()}.mywubi-skin")
                                    val selected = KeyboardSelection(item.getString("id"), version, "export")
                                    NativeBridge.objectCall("ManageSkin", "request" to request("Export")
                                        .put("selection", selected.json()).put("target", file.path))
                                    file
                                }
                                val uri = FileProvider.getUriForFile(context, "${context.packageName}.skins", target)
                                context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("application/zip")
                                    .putExtra(Intent.EXTRA_STREAM, uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION), "分享完整皮肤包"))
                            }.onFailure { message = it.message ?: "导出失败" }
                            busy = false
                        }
                    }, modifier = Modifier.semantics { contentDescription = "分享皮肤 $title" }) { Text("分享") }
                    TextButton(onClick = { deleting = item }, enabled = enabled && item.has("id"),
                        modifier = Modifier.semantics { contentDescription = "删除皮肤 $title" }) { Text("删除") }
                }
            }
        }
    }
    Text("内置", style = MaterialTheme.typography.titleMedium)
    for (option in builtinSkins) SkinActions(option, model, enabled) { preview = it }

    deleting?.let { item ->
        val title = "${item.getString("name")} · ${item.getString("version")}"
        val draft = config.getJSONObject("skins").getJSONObject("android").optJSONObject("selection")
        val saved = model.savedAndroidSkin?.optJSONObject("selection")
        fun refersToPackage(s: JSONObject?) = s != null && s.optString("id") == item.getString("id") && s.optString("version") == item.getString("version")
        val inUse = refersToPackage(saved) || refersToPackage(draft)
        AlertDialog(onDismissRequest = { deleting = null }, title = { Text("删除「$title」？") },
            text = { Text(if (inUse) "当前外观或设置草稿正在使用这个版本。请先启用其他皮肤。" else "将从本机皮肤库移除这个版本。以后可以用原文件重新导入。") },
            confirmButton = {
                if (inUse) TextButton(onClick = { deleting = null }) { Text("知道了") }
                else TextButton(onClick = {
                    deleting = null
                    scope.launch {
                        busy = true
                        runCatching {
                            withContext(Dispatchers.IO) { model.store.deleteSkin(item.getString("id"), item.getString("version")) }
                            refresh(); onLibraryChange()
                            message = "已删除「$title」。"
                        }.onFailure { message = it.message ?: "删除失败" }
                        busy = false
                    }
                }) { Text("确认删除") }
            }, dismissButton = { if (!inUse) TextButton(onClick = { deleting = null }) { Text("取消") } })
    }
    preview?.let { option ->
        SkinPreviewDialog(option, config, model, onDismiss = { preview = null })
    }
}

@Composable
private fun SkinPreviewDialog(option: SkinOption, config: JSONObject, model: SettingsModel, onDismiss: () -> Unit) {
    var skin by remember(option) { mutableStateOf<JSONObject?>(null) }
    var error by remember(option) { mutableStateOf("") }
    LaunchedEffect(option, config) {
        skin = null
        runCatching { withContext(Dispatchers.IO) { loadSkinAppearance(model.store.configFile, option.inConfig(config)) } }
            .onSuccess { skin = it }.onFailure { error = it.message ?: "预览加载失败" }
    }
    androidx.compose.ui.window.Dialog(onDismissRequest = onDismiss,
        properties = androidx.compose.ui.window.DialogProperties(usePlatformDefaultWidth = false)) {
        Surface(modifier = Modifier.fillMaxWidth().padding(8.dp), shape = MaterialTheme.shapes.large) {
            Column(Modifier.verticalScroll(rememberScrollState()).padding(8.dp)) {
                Text("预览 · ${option.name}", style = MaterialTheme.typography.titleMedium)
                if (skin != null) Keyboard(KeyboardModel(skin = skin, enabled = true, modesAvailable = true,
                    spelling = "a", candidates = listOf(CandidateItem(1, "工", "aaaa"), CandidateItem(2, "工作", "aa"))),
                    dispatch = {}, window = {}, switchIme = {})
                else if (error.isNotBlank()) Text(error) else LinearProgressIndicator()
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                    TextButton(onClick = onDismiss) { Text("关闭预览") }
                    if (!option.matches(model.savedAndroidSkin)) TextButton(onClick = {
                        model.enableSkin(option); onDismiss()
                    }, enabled = skin != null && !model.busy) { Text("启用此皮肤") }
                }
            }
        }
    }
}
