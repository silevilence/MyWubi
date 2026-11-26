package org.mywubi

import android.content.Context
import org.json.JSONObject
import java.io.File
import java.nio.file.Files

internal object NpuValidation {
    fun run(context: Context, model: File, config: JSONObject, detection: JSONObject): JSONObject {
        val devices = detection.optJSONArray("devices")
        // ORT's CPU_DISABLED mode excludes the NNAPI reference driver. Treat additional
        // CPU, GPU and OTHER devices as ambiguous even if a neural accelerator exists.
        val targets = (0 until (devices?.length() ?: 0)).map { devices!!.getJSONObject(it) }
            .filterNot { it.optString("name") == "nnapi-reference" && it.optInt("type") == 2 }
        val profile = Files.createTempDirectory(context.cacheDir.toPath(), "npu-profile-").toFile()
        try {
            val request = JSONObject().put("package", model.absolutePath).put("profile_dir", profile.absolutePath)
                .put("provider", detection.optBoolean("backend") && devices != null && targets.isNotEmpty())
                .put("accelerator_only", targets.isNotEmpty() && targets.all { it.optInt("type") == 4 })
                .put("threads", config.optInt("threads", 2)).put("wait_ms", config.optInt("wait_ms", 200))
                .put("max_candidates", config.optInt("max_candidates", 16))
            val reply = JSONObject(String(NativeBridge.npuTrial(request.toString().toByteArray()), Charsets.UTF_8))
            check(reply.optBoolean("ok")) { reply.optString("error") }
            return reply.getJSONObject("value")
        } finally {
            profile.deleteRecursively()
        }
    }
}
