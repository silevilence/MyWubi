package org.mywubi

import android.content.Context
import android.os.Build
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import org.json.JSONObject
import java.net.URL
import javax.net.ssl.HttpsURLConnection

internal const val RELEASE_ROOT = "https://github.com/silevilence/MyWubi/releases/download/"
internal const val RELEASE_API = "https://api.github.com/repos/silevilence/MyWubi/releases/latest"

data class AppUpdate(
    val versionName: String, val versionCode: Long, val minSdk: Int,
    val notes: String, val apkUrl: String, val sha256: String, val abis: List<String>
) {
    fun json() = JSONObject().put("schema", 1).put("versionName", versionName)
        .put("versionCode", versionCode).put("minSdk", minSdk).put("notes", notes)
        .put("apkUrl", apkUrl).put("sha256", sha256).put("abis", org.json.JSONArray(abis))

    companion object {
        fun parse(json: JSONObject): AppUpdate {
            require(json.getInt("schema") == 1) { "不支持的更新信息格式" }
            val name = json.getString("versionName")
            val code = json.getLong("versionCode")
            val min = json.getInt("minSdk")
            val url = json.getString("apkUrl")
            val sha = json.getString("sha256")
            val array = json.getJSONArray("abis")
            val abis = (0 until array.length()).map(array::getString)
            require(name.matches(Regex("[0-9]+\\.[0-9]+\\.[0-9]+")) && code > 0 && min >= 26)
            require(url.matches(Regex(Regex.escape(RELEASE_ROOT) + "[vV]" + Regex.escape(name) + "/MyWubi-" + Regex.escape(name) + "-Android\\.apk")))
            require(sha.matches(Regex("[a-f0-9]{64}")) && abis.isNotEmpty())
            return AppUpdate(name, code, min, json.getString("notes"), url, sha, abis)
        }
    }
}

internal fun applicableUpdate(update: AppUpdate, installed: Long, sdk: Int, abis: List<String>) =
    update.versionCode > installed && update.minSdk <= sdk && update.abis.any { it in abis }

/** No editor text, input state, or engine handles cross this boundary. */
internal fun releaseManifestUrl(release: JSONObject): String? {
    if (release.getBoolean("draft") || release.getBoolean("prerelease")) return null
    require(!release.isNull("published_at") && release.getString("published_at").isNotBlank())
    val tag = release.getString("tag_name")
    require(tag.matches(Regex("[vV][0-9]+\\.[0-9]+\\.[0-9]+")))
    val assets = release.getJSONArray("assets")
    val urls = (0 until assets.length()).map(assets::getJSONObject)
        .filter { it.getString("name") == "android-update.json" }
        .map { it.getString("browser_download_url") }
    require(urls.size == 1) { "此版本尚未提供应用内更新信息，请前往发布页手动升级" }
    return urls.single().also { require(it == "${RELEASE_ROOT}$tag/android-update.json") }
}

internal fun readUpdateJson(address: String): JSONObject {
    val connection = URL(address).openConnection() as HttpsURLConnection
    connection.connectTimeout = 10_000
    connection.readTimeout = 10_000
    connection.setRequestProperty("Accept", "application/json")
    connection.setRequestProperty("User-Agent", "MyWubi-Android-Updater")
    try {
        require(connection.responseCode == 200) { "更新服务暂不可用（HTTP ${connection.responseCode}），请重试" }
        val bytes = connection.inputStream.use { input ->
            val output = java.io.ByteArrayOutputStream()
            val buffer = ByteArray(8192)
            val deadline = android.os.SystemClock.elapsedRealtime() + 20_000
            while (true) {
                require(android.os.SystemClock.elapsedRealtime() < deadline) { "检查超时" }
                val count = input.read(buffer)
                if (count < 0) break
                require(output.size() + count <= 256 * 1024) { "更新信息过大" }
                output.write(buffer, 0, count)
            }
            output.toByteArray()
        }
        return JSONObject(bytes.toString(Charsets.UTF_8))
    } finally { connection.disconnect() }
}

internal data class CheckSchedule(val next: Long = 0, val failures: Int = 0) {
    fun permits(now: Long) = now >= next || next - now > 24 * 60 * 60 * 1000L
    fun completed(now: Long, success: Boolean): CheckSchedule {
        val count = if (success) 0 else (failures + 1).coerceAtMost(7)
        val delay = if (success) 24 * 60 * 60 * 1000L else (15 * 60 * 1000L * (1L shl (count - 1))).coerceAtMost(24 * 60 * 60 * 1000L)
        return CheckSchedule(now + delay, count)
    }
}

data class UpdateCheckState(val checking: Boolean = false, val available: AppUpdate? = null, val message: String = "尚未检查更新")

class AppUpdates internal constructor(context: Context, preferencesName: String = "app-updates",
    private val fetch: () -> AppUpdate? = {
        releaseManifestUrl(readUpdateJson(RELEASE_API))?.let { AppUpdate.parse(readUpdateJson(it)) }
    }) {
    private val app = context.applicationContext
    private val prefs = app.getSharedPreferences(preferencesName, Context.MODE_PRIVATE)
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    @Suppress("DEPRECATION")
    private val installed = app.packageManager.getPackageInfo(app.packageName, 0)
    @Suppress("DEPRECATION")
    val installedCode = if (Build.VERSION.SDK_INT >= 28) installed.longVersionCode else installed.versionCode.toLong()
    val installedName = installed.versionName ?: installedCode.toString()
    private var schedule = CheckSchedule(prefs.getLong("next", 0), prefs.getInt("failures", 0))
    private val cached = runCatching { AppUpdate.parse(JSONObject(prefs.getString("available", "")!!)) }.getOrNull()
        ?.takeIf { applicableUpdate(it, installedCode, Build.VERSION.SDK_INT, Build.SUPPORTED_ABIS.toList()) }
    private val mutable = MutableStateFlow(UpdateCheckState(available = cached))
    val state = mutable.asStateFlow()

    /** Called on the main thread at keyboard-open or from the settings button, never per key. */
    fun check(manual: Boolean = false) {
        if (mutable.value.checking || (!manual && !schedule.permits(System.currentTimeMillis()))) return
        mutable.value = mutable.value.copy(checking = true, message = "正在检查更新…")
        scope.launch {
            val result = runCatching { withContext(Dispatchers.IO) {
                fetch()?.takeIf {
                    applicableUpdate(it, installedCode, Build.VERSION.SDK_INT, Build.SUPPORTED_ABIS.toList())
                }
            } }
            schedule = schedule.completed(System.currentTimeMillis(), result.isSuccess)
            prefs.edit().putLong("next", schedule.next).putInt("failures", schedule.failures).apply()
            result.fold(onSuccess = { update ->
                prefs.edit().putString("available", update?.json()?.toString()).apply()
                mutable.value = UpdateCheckState(available = update, message = if (update == null) "当前没有适合此设备的新版本" else "发现新版本 ${update.versionName}")
            }, onFailure = {
                mutable.value = mutable.value.copy(checking = false, message = "检查失败，请重试；未能确认最新版本")
            })
        }
    }

    companion object {
        @Volatile private var instance: AppUpdates? = null
        fun get(context: Context): AppUpdates = instance ?: synchronized(this) {
            instance ?: AppUpdates(context).also { instance = it }
        }
    }
}
