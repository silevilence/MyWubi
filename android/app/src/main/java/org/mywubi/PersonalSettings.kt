package org.mywubi

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.text.input.PasswordVisualTransformation
import org.json.JSONObject

@Composable
fun PersonalSettings(model:SettingsModel,config:JSONObject) {
    var expanded by remember { mutableStateOf(false) }
    var password by remember { mutableStateOf("") }
    var encrypted by remember { mutableStateOf(false) }
    var replace by remember { mutableStateOf(false) }
    var clearModels by remember { mutableStateOf(false) }
    var clearing by remember { mutableStateOf(false) }
    val export=rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")){ uri->uri?.let {model.exportPersonal(it,if(encrypted)password else null)} }
    val import=rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()){uri->uri?.let {model.previewPersonal(it,password.takeIf{it.isNotEmpty()},replace)} }
    TextButton(onClick={expanded=!expanded}){Text("个人用预测模型（实验性）")}
    if(!expanded)return
    val personal=config.optJSONObject("personal") ?: JSONObject()
    Text("数据收集、学习与预测均在本地。只记录宿主能可靠提供的输入与编辑信息，不收集密码及禁止个性化学习的字段。")
    fun change(key:String,value:Any)=model.change("personal",key,value)
    @Composable fun toggle(label:String,value:Boolean,update:(Boolean)->Unit){Row{Switch(value,update,enabled=!model.busy);Text(label)}}
    toggle("个人输入数据收集",personal.optBoolean("collect")){change("collect",it)}
    toggle("后台学习",personal.optBoolean("learn")){change("learn",it)}
    toggle("使用个人用预测模型",personal.optBoolean("predict")){change("predict",it)}
    Text("三个开关独立，保存后生效。停止收集仍可使用已有模型；算法尚未通过真实个人数据的质量验收。")
    Text("记录容量 ${personal.optInt("max_mb",64)} MB")
    Slider(personal.optInt("max_mb",64).toFloat(),{change("max_mb",it.toInt())},valueRange=1f..128f,enabled=!model.busy)
    Text("保留 ${personal.optInt("retention_days",90)} 天")
    Slider(personal.optInt("retention_days",90).toFloat(),{change("retention_days",it.toInt())},valueRange=1f..3650f,enabled=!model.busy)
    OutlinedButton(onClick={model.personalAction("Status")},enabled=!model.busy){Text("刷新记录与模型状态")}
    Text(model.personalStatus)
    Row {
        OutlinedButton(onClick={model.personalAction("Schedule")},enabled=!model.busy){Text("请求立即更新")}
        OutlinedButton(onClick={model.personalAction("Rollback")},enabled=!model.busy){Text("回退上一版本")}
    }
    toggle("密码加密导出",encrypted){encrypted=it}
    OutlinedTextField(password,{password=it},label={Text("导出或导入密码")},visualTransformation=PasswordVisualTransformation(),singleLine=true)
    Text("密码不可恢复。明文包含输入与上下文，请自行保管；文件交换无需上传服务。")
    OutlinedButton(onClick={export.launch("personal.mwp")},enabled=!model.busy){Text("导出个人数据")}
    toggle("覆盖个人数据（否则合并）",replace){replace=it}
    OutlinedButton(onClick={import.launch(arrayOf("*/*"))},enabled=!model.busy){Text("选择导入文件并预览")}
    model.personalPreview?.let { preview ->
        Text("新增 ${preview.optInt("added")}，重复 ${preview.optInt("duplicates")}，冲突 ${preview.optInt("conflicts")}，替换 ${preview.optInt("replaced")} 条")
        OutlinedButton(onClick=model::confirmPersonalImport,enabled=!model.busy && preview.optInt("conflicts")==0){Text("确认执行导入")}
    }
    toggle("清除时同时删除个人模型及上一版本",clearModels){clearModels=it}
    Text("仅清除记录和训练缓存会保留已有模型知识。彻底清除请勾选删除模型。请先关闭并保存收集开关。")
    OutlinedButton(onClick={clearing=true},enabled=!model.busy){Text("清除个人数据…")}
    if(clearing)AlertDialog(onDismissRequest={clearing=false},title={Text("确认清除")},text={Text(if(clearModels)"删除记录、训练缓存及个人模型。" else "删除记录与训练缓存，保留已有模型。")},confirmButton={TextButton(onClick={clearing=false;model.personalAction("Clear",clearModels)}){Text("清除")}},dismissButton={TextButton(onClick={clearing=false}){Text("取消")}})
}
