package org.mywubi

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.semantics.*
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject

fun shortcutExampleTime() = JSONObject().put("year",2026).put("month",11).put("day",12)
    .put("hour",7).put("minute",8).put("second",9).put("utc_offset_seconds",28800)
    .put("timezone","Asia/Shanghai（固定示例）")

/** Draft ownership stays in the ViewModel; all resource and date calculations use core. */
@Composable
fun ShortcutSettings(model: SettingsModel, config: JSONObject, selected: Int) {
    val rules = config.optJSONObject("shortcuts")?.optJSONArray("rules") ?: JSONArray()
    val code = rules.getJSONObject(selected).optString("code")
    val indices=(0 until rules.length()).filter {rules.getJSONObject(it).optString("code").equals(code,true)}
    val serialized = rules.toString()
    val draft = config.toString()
    var preview by remember { mutableStateOf<JSONObject?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var refresh by remember { mutableIntStateOf(0) }
    LaunchedEffect(draft,code,refresh) {
        preview=null;error=null
        delay(300)
        val result=withContext(Dispatchers.IO) {runCatching {
            NativeBridge.objectCall("PreviewShortcuts","path" to model.store.configFile.absolutePath,
                "config" to JSONObject(draft),"code" to code,"time" to shortcutExampleTime())
        }}
        preview=result.getOrNull();error=result.exceptionOrNull()?.message
    }
    fun update(index:Int,key:String,value:Any) {
        val next=JSONArray(serialized);next.getJSONObject(index).put(key,value)
        model.change("shortcuts","rules",next)
    }
    fun add(source:JSONObject?) {
        val next=JSONArray(serialized)
        val positions=indices.map {next.getJSONObject(it).optLong("position")}.toSet()
        val position=positions.maxOrNull()?.plus(1)?.takeIf {it in 1L..10000L} ?: (1L..10000L).first {it !in positions}
        val row=source?.let {JSONObject(it.toString())} ?: JSONObject().put("code",code).put("kind","date_time").put("format","yyyy-MM-dd").put("enabled",true)
        next.put(row.put("position",position));model.change("shortcuts","rules",next)
    }
    Text("日期时间快捷输入",style=MaterialTheme.typography.titleLarge)
    OutlinedTextField(code,{value->
        val next=JSONArray(serialized);indices.forEach {next.getJSONObject(it).put("code",value)}
        model.change("shortcuts","rules",next)
    },label={Text("编码（1～32 个英文字母）")},singleLine=true,enabled=!model.busy,modifier=Modifier.fillMaxWidth().shortcutFieldFocus(model,selected,"code"),
        isError=indices.any {error?.contains("rules[$it].code")==true})
    Text("保存后的规范编码：${code.lowercase(java.util.Locale.ROOT)}",style=MaterialTheme.typography.bodySmall)
    Text("全局候选位置从 1 开始，下一页接着编号。候选不足时靠后排列，不留空项。",style=MaterialTheme.typography.bodySmall)
    for(index in indices) {
        val rule=rules.getJSONObject(index)
        Card(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(12.dp),verticalArrangement=Arrangement.spacedBy(8.dp)) {
                Text("格式 ${index+1} · 日期时间",style=MaterialTheme.typography.titleMedium)
                SettingToggle("用户启用",rule.optBoolean("enabled"),!model.busy) {update(index,"enabled",it)}
                val position=rule.optString("position")
                val invalidPosition=position.toLongOrNull()?.let {it !in 1L..10000L} ?: true
                OutlinedTextField(position,{update(index,"position",it.toLongOrNull() ?: it)},label={Text("全局候选位置")},
                    isError=invalidPosition || error?.contains("rules[$index].position")==true,
                    supportingText={if(invalidPosition)Text("请输入 1～10000 的整数") else if(error?.contains("rules[$index].position")==true)Text(error.orEmpty())},
                    keyboardOptions=KeyboardOptions(keyboardType=KeyboardType.Number),singleLine=true,enabled=!model.busy,modifier=Modifier.fillMaxWidth().shortcutFieldFocus(model,index,"position"))
                OutlinedTextField(rule.optString("format"),{update(index,"format",it)},label={Text("日期时间格式")},
                    enabled=!model.busy,modifier=Modifier.fillMaxWidth().shortcutFieldFocus(model,index,"format"),isError=error?.contains("rules[$index].format")==true,
                    supportingText={if(error?.contains("rules[$index].format")==true)Text(error.orEmpty())})
                preview?.optJSONArray("states")?.optJSONObject(index)?.let {status->
                    Text(if(!status.optBoolean("compatible"))status.optString("reason") else if(status.optBoolean("effective"))"当前可用" else "用户未启用",style=MaterialTheme.typography.bodySmall)
                }
                Row(horizontalArrangement=Arrangement.spacedBy(8.dp)) {
                    TextButton(onClick={add(rule)},enabled=!model.busy && indices.size<32 && rules.length()<1024,modifier=Modifier.weight(1f)) {Text("复制格式")}
                    TextButton(onClick={
                        val next=JSONArray(serialized);next.remove(index)
                        val remaining=(0 until next.length()).firstOrNull {next.getJSONObject(it).optString("code").equals(code,true)}
                        if(remaining!=null)model.specialSelection="rule:$remaining" else model.specialDetail=false
                        model.change("shortcuts","rules",next)
                    },enabled=!model.busy,modifier=Modifier.weight(1f)) {Text("删除格式")}
                }
            }
        }
    }
    OutlinedButton(onClick={add(null)},enabled=!model.busy && indices.size<32 && rules.length()<1024) {Text("新增格式")}
    SettingsDisclosure("格式标记与转义") {
        Text("yyyy 年、MM 月、dd 日、HH 时（24 小时）、mm 分、ss 秒。中文和标点直接输入；英文字面量放在单引号内，两个单引号表示单引号。示例 yyyy-MM-dd'T'HH:mm:ss。")
    }
    HorizontalDivider();Text("候选预览 · 当前设置草稿",style=MaterialTheme.typography.titleMedium)
    Text("固定示例：2026-11-12 07:08:09，Asia/Shanghai（UTC+08:00）。预览不会写入宿主。",style=MaterialTheme.typography.bodySmall)
    error?.let {Text(it,color=MaterialTheme.colorScheme.error,modifier=Modifier.semantics {liveRegion=LiveRegionMode.Polite})}
    if(error==null && preview==null) LinearProgressIndicator(Modifier.fillMaxWidth())
    preview?.let {p->
        Text("共 ${p.optInt("total")} 项 · 本轮每页最多 ${p.optInt("effective_capacity")} 项；全局位置跨页连续编号。")
        if(p.optInt("effective_capacity")==9)Text("动态命中时容量 10 也按 9 分页，全局第 10 项在第二页第 1 项。",style=MaterialTheme.typography.bodySmall)
        val warnings=p.getJSONArray("warnings")
        for(i in 0 until warnings.length())Text(warnings.getString(i),color=MaterialTheme.colorScheme.error)
        val rows=p.getJSONArray("rows")
        if(rows.length()<p.optInt("total"))Text("展示前 256 项和全部日期规则；省略的普通候选保留原位置。",style=MaterialTheme.typography.bodySmall)
        for(i in 0 until rows.length()) {
            val row=rows.getJSONObject(i)
            Text("${row.optInt("actual")}. ${row.getJSONObject("candidate").getString("text")}")
            Text("${row.getString("source")} · 目标 ${if(row.isNull("target"))"—" else row.getInt("target")} · 第 ${row.getInt("page")} 页第 ${row.getInt("slot")} 项",style=MaterialTheme.typography.bodySmall)
        }
        SettingsDisclosure("本次预览的资源快照") {
            val resources=p.getJSONArray("resources")
            for(i in 0 until resources.length())Text(resources.getString(i),style=MaterialTheme.typography.bodySmall)
        }
    }
    OutlinedButton(onClick={refresh++}) {Text("重新读取资源并预览")}
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun Modifier.shortcutFieldFocus(model: SettingsModel, index: Int, field: String): Modifier {
    val focus = remember { FocusRequester() }
    val visible = remember { BringIntoViewRequester() }
    LaunchedEffect(model.shortcutError, index, field) {
        if (model.shortcutError == (index to field)) {
            withFrameNanos { }
            focus.requestFocus()
            visible.bringIntoView()
        }
    }
    return this.focusRequester(focus).bringIntoViewRequester(visible)
}
