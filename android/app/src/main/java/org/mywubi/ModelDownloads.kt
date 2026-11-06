package org.mywubi

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.unit.dp
import org.json.JSONObject
import kotlinx.coroutines.flow.first

@Composable
fun DownloadModels(model:SettingsModel) {
    var source by rememberSaveable { mutableStateOf(model.ui.getString("model-download-source","hf_mirror") ?: "hf_mirror") }
    LaunchedEffect(Unit) {
        if (model.modelBackends.optBoolean("llama") || model.modelBackends.optBoolean("onnx")) {
            snapshotFlow { model.busy }.first { !it }
            model.ensureModelsLoaded()
        }
        while(true) {model.refreshDownloadStatus();kotlinx.coroutines.delay(500)}
    }
    val state=model.downloadStatus
    val phase=state.optString("phase","idle")
    val running=phase in listOf("starting","downloading","verifying","cancelling")
    Text("下载模型（实验性）",style=MaterialTheme.typography.titleMedium)
    Text("下载完成后仍需选择、保存并主动开启预测。部分文件会保留供重试续传。")
    val sources=listOf("Hugging Face" to "hugging_face", "HF 镜像" to "hf_mirror", "ModelScope" to "model_scope")
    TableChoice("下载源", sources.first {it.second==source}.first, sources, !running) {
        source=it;model.ui.edit().putString("model-download-source",it).apply()
    }
    for(i in 0 until model.downloadableModels.length()) {
        val entry=model.downloadableModels.getJSONObject(i)
        val p=entry.getJSONObject("manifest")
        val id=p.getString("id");val version=p.getString("version")
        val installed=model.models.any {it.getString("id")==id && it.getString("version")==version}
        val backend=model.modelBackends.optBoolean(if(p.getString("backend")=="LlamaCpp") "llama" else "onnx")
        val remote=entry.getJSONArray("remote")
        val mapped=source!="model_scope" || (0 until remote.length()).all {!remote.getJSONObject(it).isNull("modelscope")}
        val files=p.getJSONArray("files")
        val bytes=(0 until files.length()).sumOf {files.getJSONObject(it).getLong("bytes")}
        Card(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(12.dp),verticalArrangement=Arrangement.spacedBy(6.dp)) {
                Text(entry.getString("title"),style=MaterialTheme.typography.titleMedium)
                Text("下载 ${"%.1f".format(bytes/1048576.0)} MiB · 预估内存 ${entry.getLong("memory_bytes")/1048576} MiB")
                SettingsDisclosure("模型详情与来源") {
                    Text("${p.getString("backend")} · ${if(p.getJSONObject("capabilities").optBoolean("proposes_words")) "生成短字词" else "候选评分"} · ${p.getString("license")}")
                    Text(entry.getString("test_status"))
                    Text("来源：${p.getString("source")}")
                }
                if(!backend) {
                    Text("当前版本无法运行此模型。请收起模型资源，选用词典预测并保存；如需实验模型，请先安装包含运行组件的模型版。")
                    ModelBuildGuide()
                }
                if(!mapped) Text("此源没有同版本制品映射，请选择其他源。")
                if (backend && !model.modelsLoaded) {
                    Text(if (model.busy) "正在确认已安装模型；校验完成后可下载。" else "模型校验未完成，请重试。")
                    if (!model.busy) OutlinedButton(onClick = model::refreshModels) { Text("重新校验模型") }
                }
                if (backend) Button(onClick={model.startDownload(JSONObject().put("id",id).put("version",version),source)},enabled=model.modelsLoaded && !model.busy && mapped && !installed && !running,
                    modifier=Modifier.semantics {contentDescription="下载模型 $id @ $version"}) {
                    Text(if(installed) "已校验，可选择" else if(phase in listOf("failed","cancelled")) "重试 / 续传下载" else "下载模型")
                }
                if (installed) Text("下一步：展开「导入与选择模型」，选择此模型，再开启预测并点击底部「保存设置」。")
            }
        }
    }
    Text(when(phase) {
        "starting" -> "正在准备下载"; "downloading" -> "正在下载"; "verifying" -> "下载完成，正在校验并安装"
        "ready" -> "校验通过，可选择；总开关未改变"; "cancelled" -> "已取消，部分文件保留"
        "cancelling" -> "正在取消，等待当前网络读取结束"; "failed" -> "下载失败，可重试或换源续传"
        else -> "尚未开始下载"
    })
    state.optJSONObject("progress")?.let {
        val completed=it.optLong("completed");val total=it.optLong("total",1).coerceAtLeast(1)
        LinearProgressIndicator(progress={completed.toFloat()/total},modifier=Modifier.fillMaxWidth())
        Text("${"%.1f".format(completed/1048576.0)} / ${"%.1f".format(total/1048576.0)} MiB")
    }
    if(running) OutlinedButton(onClick=model::cancelDownload,enabled=phase!="cancelling") {Text("取消下载")}
    if(!state.isNull("error")) Text(state.getString("error"),color=MaterialTheme.colorScheme.error)
    if(model.downloadError.isNotEmpty()) Text(model.downloadError,color=MaterialTheme.colorScheme.error)
}
