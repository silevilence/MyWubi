package org.mywubi

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

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
        if (state == OnnxRuntime.State.NativeApkRequired || !bundled && (state == OnnxRuntime.State.Failed || transfer.phase == "failed")) {
            Text(if (state == OnnxRuntime.State.NativeApkRequired)
                "请按系统默认方式覆盖安装双架构完整包，使用本机的 x86_64 运行库。配置和模型会保留。"
            else "完整安装包内置 ONNX，后续更新沿用完整包。安装时保留配置和模型。")
            val uri = androidx.compose.ui.platform.LocalUriHandler.current
            val scope = rememberCoroutineScope()
            var lookingUp by remember { mutableStateOf(false) }
            OutlinedButton(enabled = !lookingUp, onClick = {
                lookingUp = true
                scope.launch {
                    try {
                        val update = withContext(Dispatchers.IO) {
                            val manifest = releaseManifestUrl(readUpdateJson(RELEASE_API), full = true)
                                ?: error("发布页尚无正式完整包")
                            parseReleaseUpdate(manifest, readUpdateJson(manifest))
                        }
                        val requiredAbi = if (state == OnnxRuntime.State.NativeApkRequired)
                            android.os.Build.SUPPORTED_ABIS.first() else payload.abi
                        check(update.versionCode >= AppUpdates.get(context).installedCode &&
                            update.minSdk <= android.os.Build.VERSION.SDK_INT && requiredAbi in update.abis) {
                            "正式完整包版本或架构与本机不兼容"
                        }
                        uri.openUri(update.apkUrl)
                    } catch (cancelled: kotlinx.coroutines.CancellationException) {
                        throw cancelled
                    } catch (error: Exception) {
                        feedback = error.localizedMessage ?: "无法获取正式完整包，请稍后重试"
                    } finally { lookingUp = false }
                }
            }) { Text(if (lookingUp) "正在查询完整包" else "下载正式完整安装包") }
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
