package org.mywubi

import android.content.res.Configuration
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.File

internal data class PreviewKeyboard(val label: String, val mode: String, val layout: KeyboardSelection?,
    val mapping: KeyboardSelection? = null, val reason: String? = null)

internal data class SkinPreview(val name: String, val keyboard: PreviewKeyboard, val appearance: JSONObject,
    val combination: KeyboardCombination?)

private val modeLabels = mapOf("Main" to "形码", "Pinyin" to "拼音", "English" to "英文")

internal fun previewKeyboards(path: File, skin: SkinOption): List<PreviewKeyboard> {
    val catalog = NativeBridge.objectCall("ManageKeyboard", "request" to keyboardRequest(path, "List").put("mode", "Main"))
    val items = catalog.getJSONArray("items")
    val choices = buildList {
        for (i in 0 until items.length()) {
            val item = items.getJSONObject(i)
            val layout = KeyboardSelection.decode(item.getJSONObject("selection"))
            if (skin.selection?.samePackage(layout) != true) continue
            val reason = if (item.getBoolean("enabled")) null else item.getString("reason")
            val modes = item.optJSONArray("modes")
            if (modes == null) add(PreviewKeyboard(layout.component, "Main", layout, reason = reason ?: "布局不可用"))
            else for (m in 0 until modes.length()) {
                val mode = modes.getString(m)
                add(PreviewKeyboard("${layout.component} · ${modeLabels[mode] ?: mode}", mode, layout, reason = reason))
            }
        }
    }
    if (choices.isNotEmpty()) return choices
    val current = catalog.optJSONObject("choice")
    val layout = current?.getJSONObject("layout")?.let(KeyboardSelection::decode)
    return listOf(PreviewKeyboard("当前形码键盘 · ${layout?.component ?: "内置全键盘"}", "Main", layout,
        current?.optJSONObject("mapping")?.let(KeyboardSelection::decode)))
}

internal fun prepareSkinPreview(path: File, config: JSONObject, option: SkinOption, keyboard: PreviewKeyboard): SkinPreview {
    val previewConfig = option.inConfig(config)
    val combination = keyboard.layout?.let { layout ->
        val choice = JSONObject().put("layout", layout.json()).put("mapping", keyboard.mapping?.json() ?: JSONObject.NULL)
            .put("appearance", previewConfig.getJSONObject("skins").getJSONObject("android"))
        KeyboardCombination.decode(NativeBridge.objectCall("ManageKeyboard", "request" to keyboardRequest(path, "Prepare")
            .put("choice", choice).put("mode", keyboard.mode).put("charset", keyboardCharset(path, previewConfig, keyboard.mode))))
    }
    return SkinPreview(option.name, keyboard, combination?.appearance ?: loadSkinAppearance(path, previewConfig), combination)
}

@Composable
internal fun SkinPreviewDialog(option: SkinOption, config: JSONObject, model: SettingsModel, onDismiss: () -> Unit) {
    val context = androidx.compose.ui.platform.LocalContext.current
    val path = model.store.configFile
    var keyboards by remember(option, config) { mutableStateOf(emptyList<PreviewKeyboard>()) }
    var selected by rememberSaveable(option.name) { mutableIntStateOf(0) }
    var expanded by remember { mutableStateOf(false) }
    val device = LocalConfiguration.current
    var landscape by rememberSaveable { mutableStateOf(device.orientation == Configuration.ORIENTATION_LANDSCAPE) }
    val keyboard = keyboards.getOrNull(selected)
    var error by remember(option, config, keyboard) { mutableStateOf("") }
    var preview by remember(option, config, keyboard) { mutableStateOf<SkinPreview?>(null) }
    LaunchedEffect(option, config) {
        runCatching { withContext(Dispatchers.IO) { previewKeyboards(path, option) } }
            .onSuccess { options ->
                if (options.getOrNull(selected)?.reason != null || selected !in options.indices)
                    selected = options.indexOfFirst { it.reason == null }.coerceAtLeast(0)
                keyboards = options
            }.onFailure { error = it.localizedMessage ?: "读取键盘列表失败" }
    }
    LaunchedEffect(option, config, keyboard) {
        if (keyboard != null && keyboard.reason == null) {
            runCatching { withContext(Dispatchers.IO) { prepareSkinPreview(path, config, option, keyboard) } }
                .onSuccess { preview = it }.onFailure { error = it.localizedMessage ?: "预览加载失败" }
        }
    }
    Dialog(onDismissRequest = onDismiss, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Surface(modifier = Modifier.fillMaxWidth().padding(8.dp), shape = MaterialTheme.shapes.large) {
            Column(Modifier.verticalScroll(rememberScrollState()).padding(8.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text("预览 · ${option.name}", style = MaterialTheme.typography.titleMedium)
                if (keyboards.size > 1) Box {
                    OutlinedButton(onClick = { expanded = true }) { Text("键盘 · ${keyboard?.label.orEmpty()} ▾") }
                    DropdownMenu(expanded, onDismissRequest = { expanded = false }) {
                        keyboards.forEachIndexed { index, item ->
                            DropdownMenuItem(text = { Text(listOfNotNull(item.label, item.reason).joinToString(" · ")) },
                                enabled = item.reason == null, onClick = { selected = index; expanded = false })
                        }
                    }
                } else keyboard?.let { Text(it.label) }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    FilterChip(selected = !landscape, onClick = { landscape = false }, label = { Text("竖屏") })
                    FilterChip(selected = landscape, onClick = { landscape = true }, label = { Text("横屏") })
                }
                val ready = preview
                when {
                    ready != null -> key(keyboard, landscape) { PreviewKeyboardView(ready, landscape) }
                    error.isNotEmpty() -> Text(error)
                    keyboard?.reason != null -> Text(keyboard.reason)
                    else -> LinearProgressIndicator()
                }
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                    TextButton(onClick = { ready?.let { SkinTrials.start(context, it) } }, enabled = ready != null) { Text("试打") }
                    TextButton(onClick = onDismiss) { Text("关闭预览") }
                    if (!option.matches(model.savedAndroidSkin)) TextButton(onClick = {
                        model.enableSkin(option); onDismiss()
                    }, enabled = ready != null && !model.busy) { Text("启用此皮肤") }
                }
            }
        }
    }
}

@Composable
private fun PreviewKeyboardView(preview: SkinPreview, landscape: Boolean) {
    val actual = LocalConfiguration.current
    val frame = Configuration(actual).apply {
        orientation = if (landscape) Configuration.ORIENTATION_LANDSCAPE else Configuration.ORIENTATION_PORTRAIT
        val short = minOf(actual.screenWidthDp, actual.screenHeightDp).coerceAtLeast(1)
        val long = maxOf(actual.screenWidthDp, actual.screenHeightDp).coerceAtLeast(short)
        screenWidthDp = if (landscape) long else short
        screenHeightDp = if (landscape) short else long
    }
    BoxWithConstraints(Modifier.fillMaxWidth(), contentAlignment = androidx.compose.ui.Alignment.Center) {
        val density = LocalDensity.current
        val scale = (maxWidth.value / frame.screenWidthDp).coerceAtMost(1f)
        CompositionLocalProvider(LocalConfiguration provides frame,
            LocalDensity provides Density(density.density * scale, density.fontScale)) {
            Box(Modifier.width(frame.screenWidthDp.dp)) {
                Keyboard(KeyboardModel(
                    skin = preview.appearance, customLayout = preview.combination?.let { if (landscape) it.landscape else it.portrait },
                    base = if (preview.keyboard.mode == "English") "English" else "Main",
                    temporary = "Pinyin".takeIf { preview.keyboard.mode == it },
                    enabled = true, modesAvailable = true, spelling = "a",
                    candidates = listOf(CandidateItem(1, "工", "aaaa"), CandidateItem(2, "工作", "aa"))),
                    dispatch = {}, window = {}, switchIme = {})
            }
        }
    }
}
