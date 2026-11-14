package org.mywubi

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject

@Composable
internal fun KeyboardManagement(model: SettingsModel, config: JSONObject) {
    val context=LocalContext.current
    val scope=rememberCoroutineScope()
    var mappings by remember { mutableStateOf(JSONArray()) }
    var items by remember { mutableStateOf(JSONArray()) }
    val drafts=remember { mutableMapOf<String,JSONObject?>() }
    var mode by remember { mutableStateOf("Main") }
    var draft by remember { mutableStateOf<JSONObject?>(null) }
    var preview by remember { mutableStateOf<KeyboardCombination?>(null) }
    var message by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    val path=model.store.configFile
    suspend fun list() {
        val value=withContext(Dispatchers.IO) { NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(path,"List").put("mode",mode)) }
        items=value.getJSONArray("items")
        mappings=value.getJSONArray("mappings")
        message=if(value.optJSONObject("choice")==null) "当前使用内置全键布局" else "已保存自定义组合；输入未结束时等待生效"
    }
    LaunchedEffect(config.getJSONObject("skins").toString()) { preview=null }
    LaunchedEffect(mode) { runCatching { list() }.onFailure { message="读取失败：${it.message}" } }
    HorizontalDivider()
    Text("自定义键盘",style=MaterialTheme.typography.titleMedium)
    Row { for((value,label) in listOf("Main" to "形码","Pinyin" to "拼音","English" to "英文")) {
        FilterChip(selected=mode==value,onClick={drafts[mode]=draft;mode=value;preview=null;draft=drafts[value]},label={Text(label)})
    } }
    Text("从皮肤库选择布局，预览后明确应用。数字、电话、英文、网址和密码使用确定字符布局。")
    Text(message)
    OutlinedButton(enabled=!busy,onClick={scope.launch {
        busy=true
        runCatching { withContext(Dispatchers.IO) {
            NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(path,"InstallTemplates"))
        };list() }.onFailure {message="模板添加失败：${it.message}"}
        busy=false
    }}) {Text("添加内置键盘模板到库") }
    for(index in 0 until items.length()) {
        val item=items.getJSONObject(index)
        if(item.optJSONArray("modes")?.let {modes -> (0 until modes.length()).none {modes.getString(it)==mode}} != false) continue
        OutlinedButton(enabled=!busy && item.getBoolean("enabled"),onClick={
            draft=JSONObject().put("layout",item.getJSONObject("selection"))
                .put("mapping",JSONObject.NULL).put("appearance",config.getJSONObject("skins").getJSONObject("android"))
            preview=null; message="草稿：${item.getString("name")}，尚未应用"
        }) { Text(item.getString("name")) }
        item.optString("reason").takeIf {it.isNotBlank() && it!="null"}?.let { Text(it) }
    }
    if(draft!=null) {
        Text("字母映射：${draft?.optJSONObject("mapping")?.optString("component") ?: "布局推荐"}")
        TextButton(onClick={draft=JSONObject(draft.toString()).put("mapping",JSONObject.NULL);preview=null}) {Text("使用布局推荐映射")}
        for(i in 0 until mappings.length()) {
            val mapping=mappings.getJSONObject(i)
            if((0 until mapping.getJSONArray("modes").length()).any {mapping.getJSONArray("modes").getString(it)==mode}) {
                val selection=mapping.getJSONObject("selection")
                TextButton(onClick={draft=JSONObject(draft.toString()).put("mapping",selection);preview=null}) {
                    Text("映射 ${selection.getString("id")} / ${selection.getString("component")}")
                }
            }
        }
    }
    Text("布局与外观独立保存。外观有未保存修改时，请先保存设置，再应用布局。")
    Row {
        Button(enabled=draft!=null && !busy,onClick={scope.launch {
            busy=true; preview=null
            runCatching { withContext(Dispatchers.IO) {
                KeyboardCombination.decode(NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(path,"Prepare")
                    .put("choice",draft).put("mode",mode).put("charset",keyboardCharset(path,config,mode))))
            } }.onSuccess { preview=it;message="预览已准备，点击应用启用；绑定模式：${it.mode}" }.onFailure { message="无法启用：${it.message}" }
            busy=false
        }}) { Text("预览") }
        Button(enabled=preview!=null && !busy && !model.dirty,onClick={scope.launch {
            busy=true
            runCatching { withContext(Dispatchers.IO) {
                val snapshot=requireNotNull(preview)
                NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(path,"Save")
                    .put("choice",snapshot.wire.getJSONObject("choice")).put("mode",snapshot.mode)
                    .put("charset",keyboardCharset(path,config,snapshot.mode)).put("digest",snapshot.digest))
                EngineUpdates.request(context)
            } }.onSuccess { message="已保存；正在输入或触摸时延后生效" }.onFailure { message="应用失败，原组合保留：${it.message}" }
            busy=false
        }}) { Text("应用布局") }
        TextButton(enabled=!busy,onClick={preview=null;message="已取消应用，保留布局草稿"}) { Text("取消") }
    }
    OutlinedButton(enabled=!busy,onClick={scope.launch {
        busy=true
        runCatching { withContext(Dispatchers.IO) {
            NativeBridge.objectCall("ManageKeyboard","request" to keyboardRequest(path,"Reset").put("mode",mode));EngineUpdates.request(context)
        } }.onSuccess {draft=null;preview=null;message="已恢复内置全键；当前输入结束后生效"}.onFailure{message="恢复失败：${it.message}"}
        busy=false
    }}) { Text("恢复默认全键布局") }
    if(busy) LinearProgressIndicator(Modifier.fillMaxWidth())
    preview?.let { Keyboard(KeyboardModel(customLayout=it.portrait,skin=it.appearance,enabled=true,status="布局预览"),
        dispatch={},window={},switchIme={}) }
}
