package org.mywubi

import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.unit.dp
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

val specialCategories = listOf("emoji" to "Emoji", "kaomoji" to "颜文字", "kana" to "假名", "math" to "数学符号", "punctuation" to "标点", "symbols" to "其他符号")

@Composable
fun SpecialInputSettings(model: SettingsModel, config: JSONObject, modifier: Modifier = Modifier) {
    BoxWithConstraints(modifier.fillMaxWidth()) {
        val wide = maxWidth >= 600.dp
        BackHandler(model.specialDetail && !wide) { model.specialDetail = false }
        if(wide) {
            Row(Modifier.fillMaxSize(), horizontalArrangement=Arrangement.spacedBy(20.dp)) {
                SpecialInputList(model,config,Modifier.width(240.dp).fillMaxHeight())
                SpecialInputDetail(model,config,Modifier.weight(1f).fillMaxHeight(),showBack=false)
            }
        } else if(model.specialDetail) SpecialInputDetail(model,config,Modifier.fillMaxSize())
        else SpecialInputList(model,config,Modifier.fillMaxSize())
    }
}

@Composable
private fun SpecialInputList(model: SettingsModel, config: JSONObject, modifier: Modifier) {
    Column(modifier.verticalScroll(model.specialListScroll).semantics {contentDescription="特殊输入项目列表"},verticalArrangement=Arrangement.spacedBy(8.dp)) {
        Text("特殊输入",style=MaterialTheme.typography.titleLarge)
        SpecialInputBackup(model)
        OutlinedTextField(model.specialSearch,{model.specialSearch=it},label={Text("搜索类别或编码")},singleLine=true,modifier=Modifier.fillMaxWidth())
        Text("特殊码表",style=MaterialTheme.typography.titleMedium)
        for((kind,label) in specialCategories) {
            if(!label.contains(model.specialSearch,true) && !kind.contains(model.specialSearch,true)) continue
            val selection=config.optJSONObject("expressions")?.optJSONObject(kind) ?: JSONObject()
            val uses=listOfNotNull("追加候选".takeIf {selection.optBoolean("append")},"可检索".takeIf {selection.optBoolean("search")}).joinToString(" · ").ifEmpty {"未启用"}
            val reference=selection.optString("resource").takeUnless {it.isEmpty() || it=="null"}
            val current=model.expressionResources[kind]?.firstOrNull {info->
                if(reference==null) info.optBoolean("builtin") else File(info.getString("path"))==File(reference).let {if(it.isAbsolute)it else File(model.store.configFile.parentFile,reference)}
            }
            val status=if(current==null) "待检查" else if(!current.isNull("error")) "资源不可用" else ""
            ItemButton(label,listOf(uses,status).filter {it.isNotEmpty()}.joinToString(" · "),model.specialSelection==kind) {model.specialSelection=kind;model.specialDetail=true}
        }
        HorizontalDivider();Text("快捷输入",style=MaterialTheme.typography.titleMedium)
        val rules=config.optJSONObject("shortcuts")?.optJSONArray("rules") ?: JSONArray()
        val groups=(0 until rules.length()).groupBy {rules.getJSONObject(it).optString("code").lowercase(java.util.Locale.ROOT)}
        val selectedCode=model.specialSelection.removePrefix("rule:").toIntOrNull()?.let {rules.optJSONObject(it)?.optString("code")}
        for((code,indices) in groups) {
            if(!code.contains(model.specialSearch,true)) continue
            val enabled=indices.count {rules.getJSONObject(it).optBoolean("enabled")}
            val id="rule:${indices.first()}"
            ItemButton(code,"${indices.size} 个格式 · $enabled 个用户启用",selectedCode?.equals(code,true)==true) {model.specialSelection=id;model.specialDetail=true}
        }
        OutlinedButton(onClick={
            var code="jjad"
            var suffix=0
            while(groups.containsKey(code)) {
                val n=suffix++
                code="jjad" + ('a'.code+n/676%26).toChar()+('a'.code+n/26%26).toChar()+('a'.code+n%26).toChar()
            }
            val next=JSONArray(rules.toString())
            model.specialSelection="rule:${next.length()}";model.specialDetail=true
            next.put(JSONObject().put("code",code).put("position",2).put("kind","date_time").put("format","yyyy年MM月dd日").put("enabled",true))
            model.change("shortcuts","rules",next)
        },enabled=!model.busy && rules.length()<1024) {Text("新增快捷编码")}
    }
}

@Composable
private fun SpecialInputBackup(model: SettingsModel) {
    val export = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri ->
        uri?.let(model::exportSpecialInput)
    }
    val restore = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri?.let(model::restoreSpecialInput)
    }
    SettingsDisclosure("特殊输入备份与恢复") {
        Text("包含当前草稿的六类所选资源、开关和日期规则；不包含主码表、个人词库或其他设置。恢复会替换特殊输入草稿，检查后再保存。")
        OutlinedButton(onClick={export.launch("MyWubi-special-input.mwsi")},enabled=!model.busy) {Text("导出特殊输入备份")}
        OutlinedButton(onClick={restore.launch(arrayOf("*/*"))},enabled=!model.busy) {Text("恢复备份到草稿")}
    }
}
@Composable
private fun ItemButton(title:String,summary:String,selected:Boolean,onClick:()->Unit) {
    Surface(onClick=onClick,modifier=Modifier.fillMaxWidth().heightIn(min=56.dp).semantics {this.selected=selected},
        color=if(selected)MaterialTheme.colorScheme.secondaryContainer else MaterialTheme.colorScheme.surface,
        shape=MaterialTheme.shapes.medium) {
        Column(Modifier.padding(12.dp),verticalArrangement=Arrangement.spacedBy(4.dp)) {
            Text(title,style=MaterialTheme.typography.titleSmall)
            Text(summary,style=MaterialTheme.typography.bodySmall)
        }
    }
}
@Composable
private fun SpecialInputDetail(model:SettingsModel,config:JSONObject,modifier:Modifier,showBack:Boolean=true) {
    Column(modifier.verticalScroll(model.specialDetailScroll).semantics {contentDescription="特殊输入项目详情"},verticalArrangement=Arrangement.spacedBy(12.dp)) {
        if(showBack)TextButton(onClick={model.specialDetail=false}) {Text("返回项目列表")}
        val index=model.specialSelection.removePrefix("rule:").toIntOrNull()
        if(index!=null) {
            val rules=config.optJSONObject("shortcuts")?.optJSONArray("rules")
            if(rules!=null && index in 0 until rules.length()) ShortcutSettings(model,config,index)
            else Text("请选择或新增一个快捷编码。")
        } else {
            ExpressionSettings(model,config,model.specialSelection)
            ExpressionTrial(model,config,model.specialSelection)
        }
    }
}
