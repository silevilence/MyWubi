package org.mywubi

import android.content.ClipData
import android.content.ClipboardManager
import android.os.Build
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.platform.LocalContext
import kotlinx.coroutines.flow.first
import org.json.JSONObject
import java.io.File

internal fun npuReason(reason: String): String = when (reason.substringBefore(':')) {
    "GGUF_CPU_ONLY" -> "当前 GGUF 适配器使用 CPU"
    "PACKAGE_CPU_ONLY" -> "模型包仅声明 CPU，或沿用旧版清单"
    "NNAPI_UNAVAILABLE" -> "当前运行组件或系统加速设备不可用"
    "NO_CURRENT_VALIDATION" -> "尚无适用于当前模型和预算的验证结果"
    "BENCHMARK_GATE" -> "固定样例的候选或延迟未达到 CPU 对照要求"
    "PLACEMENT_GATE" -> "未确认全图使用 NPU"
    "NNAPI_TIMEOUT" -> "加速预测连续超时，已暂停使用此组合"
    "NNAPI_EXECUTION", "NNAPI_LOAD", "NNAPI_PREDICT" -> "驱动加载或执行失败，已保留 CPU 路径"
    else -> reason
}

internal object LastPredictionDevice {
    var entry by mutableStateOf<JSONObject?>(null)
        private set

    // Capture only the identity and the backend's execution report, never the request.
    fun record(model: JSONObject, execution: JSONObject) {
        val current = JSONObject().put("id", model.getString("id")).put("version", model.getString("version"))
            .put("actual", execution.opt("actual")).put("reason", execution.opt("reason"))
        if (entry?.toString() != current.toString()) entry = current
    }
}

@Composable
internal fun PredictionDeviceSettings(model: SettingsModel, prediction: JSONObject) {
    val context = LocalContext.current
    val runtime by OnnxRuntime.status.collectAsState()
    var revision by remember { mutableIntStateOf(0) }
    val detection = remember(runtime, revision) { NpuDetection.cached(context, runtime) }
    val selection = prediction.optJSONObject("model")
    val name = selection?.let { "${it.getString("id")}--${it.getString("version")}" }
    val manifest = model.models.firstOrNull { "${it.getString("id")}--${it.getString("version")}" == name }
    LaunchedEffect(name) {
        if (name != null) {
            snapshotFlow { model.busy }.first { !it }
            model.ensureModelsLoaded()
        }
    }
    val task = when {
        name == null -> "词典预测"
        manifest == null -> "模型包待校验"
        manifest.optJSONObject("capabilities")?.optBoolean("proposes_words") == true -> "下一词生成"
        else -> "候选评分"
    }
    val compatibility = when {
        manifest == null -> task
        manifest.optString("backend") == "LlamaCpp" -> "$task · GGUF 使用 CPU"
        manifest.optJSONObject("android_execution")?.optString("provider") == "nnapi" -> "$task · 可验证 ONNX 图的 NNAPI 分配"
        else -> "$task · 使用 ONNX CPU"
    }
    val key = listOf(name, manifest?.opt("files"), prediction.optInt("threads"), prediction.optInt("wait_ms"),
        prediction.optInt("max_candidates"), runtime.name).joinToString("|")
    var sample by remember(key) { mutableStateOf<JSONObject?>(null) }
    val state = detection?.let { NpuDetection.State.valueOf(it.getString("state")).label } ?: "尚未检测"
    val devices = detection?.optJSONArray("devices")
    val drivers = (0 until (devices?.length() ?: 0)).joinToString("\n") { i ->
        val d = devices!!.getJSONObject(i)
        "${d.getString("name")} · ${d.getString("driver")}"
    }
    val last = LastPredictionDevice.entry?.takeIf { it.optString("id") == selection?.optString("id") &&
        it.optString("version") == selection?.optString("version") }
    val actual = last?.optString("actual") ?: "尚无预测记录"
    val fallback = last?.opt("reason") as? String
    val report = sample
    val reason = report?.opt("reason") as? String
    val error = detection?.optString("error").orEmpty()
    val chip = if (Build.VERSION.SDK_INT >= 31) Build.SOC_MODEL else Build.HARDWARE
    val diagnostics = listOf("${Build.MANUFACTURER} ${Build.MODEL} · $chip · Android ${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT})",
        "${NativeBridge.runtimeAbi()} · ONNX Runtime ${OnnxComponent.version} · ${runtime.label}", state, drivers, error,
        "模型 ${name ?: "词典"} · $compatibility", "最近一次键盘预测：$actual", fallback,
        report?.let { "固定样例：${it.optString("actual")} · ${it.optJSONObject("node_events") ?: ""}" }, reason)
        .filterNot { it.isNullOrBlank() }.joinToString("\n")
    Column {
        Text("加速器检测：$state")
        if (drivers.isNotEmpty()) Text(drivers)
        OutlinedButton(enabled = !model.busy && runtime != OnnxRuntime.State.Checking, onClick = {
            model.work {
                NpuDetection.detect(context, runtime)
                val publish: () -> Unit = { revision++; sample = null }
                publish
            }
        }) { Text(if (detection == null) "检测加速器" else "重新检测加速器") }
        Text("${name ?: "仅词典"} · $compatibility")
        Text("最近一次键盘预测：$actual")
        fallback?.let { Text(npuReason(it)) }
        OutlinedButton(enabled = !model.busy && manifest != null && runtime != OnnxRuntime.State.Checking, onClick = {
            model.work {
                val found = NpuDetection.detect(context, runtime)
                val result = runCatching {
                    NpuValidation.run(context, File(model.store.configFile.parentFile, "models/$name"), prediction, found)
                }.getOrElse { JSONObject().put("actual", "执行设备未确认").put("reason", it.message ?: "VALIDATION_FAILED") }
                val publish: () -> Unit = { revision++; sample = result }
                publish
            }
        }) { Text("用固定样例验证模型") }
        report?.let {
            Text("固定样例执行设备：${it.optString("actual")}")
            reason?.let { code -> Text(npuReason(code)) }
            val words = (it.optJSONObject("nnapi") ?: it.optJSONObject("cpu"))?.optJSONArray("words")
            words?.optJSONArray(0)?.let { first -> Text("「北京」的预测：" + (0 until first.length()).joinToString("、") { n -> first.getString(n) }.ifEmpty { "无候选" }) }
        }
        var copied by remember(diagnostics) { mutableStateOf(false) }
        OutlinedButton(onClick = {
            context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("MyWubi 预测诊断", diagnostics))
            copied = true
        }) { Text(if (copied) "已复制诊断信息" else "复制诊断信息") }
    }
}
