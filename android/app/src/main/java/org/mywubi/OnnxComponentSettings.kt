package org.mywubi

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext

@Composable
internal fun OnnxComponentSettings() {
    val context = LocalContext.current.applicationContext
    val state by OnnxRuntime.status.collectAsState()
    val transfer by OnnxTransfers.status.collectAsState()
    val busy = transfer.phase in listOf("downloading", "verifying")
    var feedback by remember { mutableStateOf("") }
    var confirmRemove by remember { mutableStateOf(false) }
    val bundled = remember { OnnxComponent.bundled(context).isFile }
    val payload = remember { OnnxComponent.payload() }
    Column {
        Text("ONNX Runtime ${OnnxComponent.version} · ${state.label}", style = MaterialTheme.typography.titleMedium)
        if (bundled) Text("随完整安装包提供") else {
            Text("来源：Microsoft · Maven Central")
            Text("下载 %.1f MiB · 安装 %.1f MiB（%s）".format(
                OnnxComponent.archiveBytes / 1048576.0, payload.bytes / 1048576.0, payload.abi))
        }
        if (busy) {
            Text(if (transfer.phase == "verifying") "正在校验并安装" else "正在下载运行组件")
            LinearProgressIndicator(progress = { transfer.bytes.toFloat() / OnnxComponent.archiveBytes }, modifier = Modifier.fillMaxWidth())
            OutlinedButton(onClick = OnnxTransfers::cancel) { Text("取消组件下载") }
        } else if (!bundled && state in listOf(OnnxRuntime.State.Missing, OnnxRuntime.State.UpdateAvailable, OnnxRuntime.State.Failed, OnnxRuntime.State.Corrupt)) {
            Button(onClick = {
                feedback = ""
                OnnxTransfers.start(context) { OnnxRuntime.refresh(context) }
            }) {
                Text(when {
                    transfer.phase in listOf("cancelled", "failed") -> "重试或续传组件"
                    state == OnnxRuntime.State.UpdateAvailable -> "更新 ONNX 运行组件"
                    else -> "获取 ONNX 运行组件"
                })
            }
        }
        if (!busy && !bundled && state in listOf(OnnxRuntime.State.Ready, OnnxRuntime.State.Corrupt, OnnxRuntime.State.Failed)) {
            OutlinedButton(onClick = { confirmRemove = true }) { Text("移除 ONNX 运行组件") }
        }
        if (!bundled && (state == OnnxRuntime.State.Failed || transfer.phase == "failed")) {
            Text("也可在正式发布页安装同版本 Android-full.apk。完整包内置 ONNX，后续更新沿用完整包。")
            val uri = androidx.compose.ui.platform.LocalUriHandler.current
            OutlinedButton(onClick = { uri.openUri("https://github.com/silevilence/MyWubi/releases") }) {
                Text("查看完整安装包")
            }
        }
        if (transfer.message.isNotEmpty()) Text(transfer.message)
        if (feedback.isNotEmpty()) Text(feedback)
    }
    if (confirmRemove) AlertDialog(onDismissRequest = { confirmRemove = false },
        title = { Text("移除 ONNX 运行组件？") },
        text = { Text("模型文件会保留。正在使用的运行库将在应用进程下次启动时移除。") },
        confirmButton = { TextButton(onClick = {
            confirmRemove = false
            OnnxRuntime.remove(context) { message -> android.os.Handler(context.mainLooper).post { feedback = message } }
        }) { Text("移除组件") } },
        dismissButton = { TextButton(onClick = { confirmRemove = false }) { Text("取消") } })
}
