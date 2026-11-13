package org.mywubi

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/** Edits the persisted draft; validation and normalization are owned by the shared core. */
@Composable
fun ShortcutSettings(model: SettingsModel, config: JSONObject) {
    val rules = config.optJSONObject("shortcuts")?.optJSONArray("rules") ?: JSONArray()
    val serialized = rules.toString()
    val tablePath = config.getJSONObject("dictionary").getString("system_table")
    var inspection by remember { mutableStateOf<JSONObject?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(serialized, tablePath) {
        inspection = null
        val result = withContext(Dispatchers.IO) { runCatching {
            val table = File(tablePath).let { if(it.isAbsolute) it else File(model.store.configFile.parentFile, tablePath) }
            val header = NativeBridge.objectCall("ReadTableConfig", "path" to table.path)
            NativeBridge.objectCall("InspectShortcuts", "rules" to JSONArray(serialized), "table" to header)
        } }
        inspection = result.getOrNull()
        error = result.exceptionOrNull()?.message
    }
    fun update(index: Int, key: String, value: Any) {
        val next = JSONArray(serialized)
        next.getJSONObject(index).put(key, value)
        model.change("shortcuts", "rules", next)
    }
    Text("日期时间快捷输入", style=MaterialTheme.typography.titleMedium)
    Text("编码为 1～32 个英文字母，保存后显示小写。全局位置从 1 开始，下一页接着编号。", style=MaterialTheme.typography.bodySmall)
    for(index in 0 until rules.length()) {
        val rule = rules.getJSONObject(index)
        Column(Modifier.fillMaxWidth().padding(vertical=8.dp), verticalArrangement=Arrangement.spacedBy(8.dp)) {
            SettingToggle("用户启用", rule.optBoolean("enabled"), !model.busy) { update(index, "enabled", it) }
            OutlinedTextField(rule.optString("code"), {update(index,"code",it)}, label={Text("编码")}, enabled=!model.busy, singleLine=true, modifier=Modifier.fillMaxWidth())
            OutlinedTextField(rule.optString("position"), {update(index,"position",it.toLongOrNull() ?: it)}, label={Text("全局候选位置")}, enabled=!model.busy, singleLine=true, modifier=Modifier.fillMaxWidth())
            OutlinedTextField(rule.optString("format"), {update(index,"format",it)}, label={Text("日期时间格式")}, enabled=!model.busy, modifier=Modifier.fillMaxWidth())
            inspection?.getJSONArray("states")?.optJSONObject(index)?.let { status ->
                Text(if(!status.optBoolean("compatible")) status.optString("reason") else if(status.optBoolean("effective")) "当前可用" else "用户未启用", style=MaterialTheme.typography.bodySmall)
            }
            TextButton(onClick={val next=JSONArray(serialized);next.remove(index);model.change("shortcuts","rules",next)}, enabled=!model.busy) {Text("删除此格式")}
            HorizontalDivider()
        }
    }
    OutlinedButton(onClick={
        val next=JSONArray(serialized)
        val positions=(0 until next.length()).map(next::getJSONObject).filter { it.optString("code").equals("jjad",true) }.map {it.optLong("position")}.toSet()
        val position=(1L..10000L).firstOrNull {it !in positions} ?: 1L
        next.put(JSONObject().put("code","jjad").put("position",position).put("kind","date_time").put("format","yyyy年MM月dd日").put("enabled",true))
        model.change("shortcuts","rules",next)
    }, enabled=!model.busy) { Text("新增格式") }
    error?.let {Text(it, color=MaterialTheme.colorScheme.error)}
    Text("格式：yyyy 年、MM 月、dd 日、HH 时、mm 分、ss 秒。英文字面量使用单引号；两个单引号表示单引号。示例 yyyy-MM-dd'T'HH:mm:ss。", style=MaterialTheme.typography.bodySmall)
}
