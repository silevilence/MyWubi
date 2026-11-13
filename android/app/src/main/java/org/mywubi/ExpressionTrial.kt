package org.mywubi

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.*
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject

class ExpressionTrialDraft {
    var input by mutableStateOf("Words")
    var text by mutableStateOf("")
    var actions by mutableStateOf("[]")
    fun append(action:Any) {
        val next=JSONArray(actions)
        if(text.isNotEmpty())next.put(JSONObject().put("Input",text))
        next.put(action);text="";actions=next.toString()
    }
    fun clear() {text="";actions="[]"}
}

@Composable
fun ExpressionTrial(model:SettingsModel,config:JSONObject,kind:String) {
    val draft=model.specialTrials.getValue(kind)
    val serialized=config.toString()
    var preview by remember(kind) {mutableStateOf<JSONObject?>(null)}
    var error by remember(kind) {mutableStateOf<String?>(null)}
    LaunchedEffect(serialized,kind,draft.input,draft.text,draft.actions) {
        preview=null;error=null
        delay(300)
        val actions=JSONArray(draft.actions)
        if(draft.text.isNotEmpty())actions.put(JSONObject().put("Input",draft.text))
        val input=draft.input
        val result=withContext(Dispatchers.IO) {runCatching {
            NativeBridge.objectCall("PreviewExpression","path" to model.store.configFile.absolutePath,
                "config" to JSONObject(serialized),"kind" to kind,"input" to input,"actions" to actions)
        }}
        preview=result.getOrNull();error=result.exceptionOrNull()?.message
    }
    HorizontalDivider();Text("检索试用",style=MaterialTheme.typography.titleMedium)
    Text("使用当前草稿及所选资源，只在设置中预览，不写入宿主。",style=MaterialTheme.typography.bodySmall)
    val empty=draft.text.isEmpty() && draft.actions=="[]"
    Column {
        FilterChip(selected=draft.input=="Words",onClick={draft.input="Words"},label={Text("主码表组词")},enabled=empty)
        FilterChip(selected=draft.input=="Literal",onClick={draft.input="Literal"},label={Text("直接输入别名")},enabled=empty)
    }
    if(!empty)Text("切换查询方式前请先清空。",style=MaterialTheme.typography.bodySmall)
    val query=preview?.optJSONObject("view")?.optJSONObject("expression")
    val selected=preview?.optString("selected")?.takeUnless {it=="null" || it.isEmpty()}
    val editing=selected==null && query?.optString("phase")!="Results"
    OutlinedTextField(draft.text,{draft.text=it},label={Text(if(draft.input=="Literal")"别名（如 shi，数字也是查询文本）" else "主码表编码（选择下方候选组成查询）")},
        enabled=editing,singleLine=true,modifier=Modifier.fillMaxWidth())
    if(error==null && preview==null)LinearProgressIndicator(Modifier.fillMaxWidth())
    error?.let {Text(it,color=MaterialTheme.colorScheme.error,modifier=Modifier.semantics {liveRegion=LiveRegionMode.Polite})}
    query?.let {q->
        Text("查询：${q.optString("text")}")
        if(!q.isNull("message"))Text(q.getString("message"))
    }
    selected?.let {Text("选中结果：$it",style=MaterialTheme.typography.titleMedium)}
    preview?.optJSONObject("window")?.let {window->
        val candidates=window.getJSONArray("candidates")
        if(window.optInt("total")>candidates.length())Text("共 ${window.optInt("total")} 项，试用展示前 256 项。",style=MaterialTheme.typography.bodySmall)
        for(i in 0 until candidates.length()) {
            val candidate=candidates.getJSONObject(i)
            OutlinedButton(onClick={draft.append(JSONObject().put("Select",candidate.getLong("id")))},modifier=Modifier.fillMaxWidth()) {Text(candidate.getString("text"))}
        }
    }
    Row(horizontalArrangement=Arrangement.spacedBy(8.dp)) {
        Button(onClick={draft.append("Find")},enabled=editing && preview!=null,modifier=Modifier.weight(1f)) {Text("查找")}
        OutlinedButton(onClick={draft.append("Edit")},enabled=!editing && selected==null && preview!=null,modifier=Modifier.weight(1f)) {Text("修改查询")}
    }
    OutlinedButton(onClick=draft::clear) {Text("清空试用")}
    preview?.let {p->SettingsDisclosure("试用资源") {
        val resources=p.getJSONArray("resources")
        for(i in 0 until resources.length())Text(resources.getString(i),style=MaterialTheme.typography.bodySmall)
    }}
}
