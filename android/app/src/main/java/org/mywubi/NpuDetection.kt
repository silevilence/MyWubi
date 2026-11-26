package org.mywubi

import android.content.Context
import android.os.Build
import org.json.JSONObject

internal object NpuDetection {
    enum class State(val label: String) {
        Available("发现可供推理后端调用的加速器"), Hardware("系统报告了加速器，模型后端不可用"),
        Unknown("加速能力未确认"), Unavailable("推理后端不可用")
    }

    fun classify(accelerator: Boolean, backend: Boolean, queried: Boolean): State = when {
        !queried && backend -> State.Unknown
        accelerator && backend -> State.Available
        accelerator -> State.Hardware
        !backend -> State.Unavailable
        else -> State.Unknown
    }

    private fun key(runtime: OnnxRuntime.State) =
        listOf(Build.FINGERPRINT, NativeBridge.runtimeAbi(), OnnxComponent.version, runtime.name).joinToString("|")

    fun cached(context: Context, runtime: OnnxRuntime.State): JSONObject? {
        val prefs = context.getSharedPreferences("npu-detection", Context.MODE_PRIVATE)
        return prefs.getString("report", null)?.let(::JSONObject)?.takeIf { it.optString("key") == key(runtime) }
    }

    /** Only called by a settings worker on explicit detection or validation. */
    fun detect(context: Context, runtime: OnnxRuntime.State): JSONObject {
        val report = JSONObject(NativeBridge.nnapiDevices())
        val devices = report.optJSONArray("devices")
        val accelerator = devices != null && (0 until devices.length()).any { devices.getJSONObject(it).getInt("type") == 4 }
        val backend = runtime == OnnxRuntime.State.Ready && NativeBridge.hasNnapiProvider()
        report.put("backend", backend).put("state", classify(accelerator, backend, devices != null).name)
            .put("runtime", runtime.name).put("key", key(runtime))
        context.getSharedPreferences("npu-detection", Context.MODE_PRIVATE).edit().putString("report", report.toString()).apply()
        return report
    }
}
