package org.mywubi

import android.app.Application
import android.content.Context
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import org.json.JSONObject
import java.io.File
import java.nio.file.Files
import java.util.concurrent.Executors

internal class MyWubiApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        OnnxRuntime.refresh(this)
    }
}

/** Serializes verification, process-lifetime native activation and removal. */
internal object OnnxRuntime {
    enum class State(val label: String) {
        Checking("正在检查组件"), Missing("未安装"), Ready("已安装，可加载"),
        Corrupt("校验失败，请重新获取"), Failed("已安装，加载失败"),
        PendingRemoval("待重启移除"), Unsupported("未编入 ONNX 适配器")
    }
    private val worker = Executors.newSingleThreadExecutor()
    private val current = MutableStateFlow(State.Checking)
    val status = current.asStateFlow()
    private var active: File? = null
    private fun marker(context: Context) = File(OnnxComponent.installed(context).parentFile, "remove-on-restart")

    private fun selected(context: Context): Boolean {
        val saved = EngineStore(context).readConfig()
        NativeHandle(saved.getLong("version")).use {
            val selected = saved.getJSONObject("config").getJSONObject("prediction").optJSONObject("model") ?: return false
            val packages = ModelCatalog(context).call("List")
            return (0 until packages.length()).map(packages::getJSONObject).firstOrNull {
                it.getString("id") == selected.getString("id") && it.getString("version") == selected.getString("version")
            }?.optString("backend") != "LlamaCpp"
        }
    }

    fun refresh(context: Context) {
        val app = context.applicationContext
        worker.execute {
            current.value = runCatching { activate(app) }.getOrDefault(State.Failed)
        }
    }

    private fun activate(app: Context): State {
        val features = JSONObject(NativeBridge.inference("""{"op":"Features"}""".toByteArray()).toString(Charsets.UTF_8))
        if (!features.getJSONObject("value").getBoolean("onnx")) return State.Unsupported
        val bundled = OnnxComponent.bundled(app)
        val path = OnnxComponent.location(app)
        val marker = marker(app)
        if (marker.isFile && path != bundled) {
            if (active != null) return State.PendingRemoval
            if (!selected(app)) Files.deleteIfExists(path.toPath())
            Files.deleteIfExists(marker.toPath())
        }
        if (!path.isFile) return State.Missing
        if (active == path) return State.Ready
        val payload = OnnxComponent.payload()
        if (path != bundled && !OnnxInstaller.matches(path, payload.bytes, payload.hash)) return State.Corrupt
        if (!NativeBridge.probeOnnx(path.absolutePath)) return State.Failed
        active = path
        return State.Ready
    }

    fun remove(context: Context, report: (String) -> Unit) {
        val app = context.applicationContext
        worker.execute {
            val message = runCatching {
                check(!OnnxComponent.bundled(app).isFile) { "此运行库随 APK 提供" }
                check(!selected(app)) { "请先选择词典或 GGUF 模型并保存设置" }
                val path = OnnxComponent.installed(app)
                if (active != null) {
                    marker(app).writeText(OnnxComponent.version)
                    current.value = State.PendingRemoval
                    "重启应用和输入法后移除组件"
                } else {
                    Files.deleteIfExists(path.toPath())
                    current.value = State.Missing
                    "组件已移除"
                }
            }.getOrElse { it.localizedMessage ?: "移除失败" }
            report(message)
        }
    }
}
