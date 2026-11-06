package org.mywubi

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics

@Composable
fun ModelTrial(model:SettingsModel) {
    var expanded by rememberSaveable {mutableStateOf(false)}
    var guide by remember {mutableStateOf(false)}
    var sample by rememberSaveable {mutableStateOf("北京")}
    var selected by rememberSaveable {mutableStateOf("")}
    OutlinedButton(onClick={expanded=!expanded}) {Text("自定义模型指南与试运行（实验性）")}
    if(!expanded) return
    OutlinedButton(onClick={guide=true}) {Text("接入指南（离线可读）")}
    if(guide) {
        val text=remember {NativeBridge.modelGuide()}
        AlertDialog(onDismissRequest={guide=false},title={Text("自定义模型接入")},
            text={Text(text,Modifier.verticalScroll(rememberScrollState()))},
            confirmButton={TextButton(onClick={guide=false}) {Text("关闭指南")}})
    }
    val choices=model.models.map {val name="${it.getString("id")}--${it.getString("version")}";name to name}
    TableChoice("试运行模型",selected.ifEmpty {"请选择模型"},choices,!model.busy) {selected=it;model.clearTrialResult()}
    OutlinedTextField(value=sample,onValueChange={if(it.codePointCount(0,it.length)<=256) {sample=it;model.clearTrialResult()}},
        enabled=!model.busy,label={Text("测试前文")},modifier=Modifier.fillMaxWidth().semantics {contentDescription="测试前文"})
    Text("仅测试明确输入的样例，不读取编辑器或更改活动模型。")
    Button(onClick={model.trialModel(selected,sample)},enabled=!model.busy && choices.any {it.second==selected} && sample.isNotBlank()) {
        Text("试运行（1000 ms，4 线程）")
    }
    if(model.busy) Text("正在校验、加载或试运行…")
    if(model.trialResult.isNotEmpty()) Text(model.trialResult,Modifier.semantics {contentDescription="模型试运行结果"})
    Text("通过仅表示此样例可运行；不会自动开启预测或标为推荐。")
}
