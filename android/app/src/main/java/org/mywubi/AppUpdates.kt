package org.mywubi

import android.content.Context
import android.os.Build
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import org.json.JSONArray
import org.json.JSONObject
import java.net.URL
import javax.net.ssl.HttpsURLConnection

internal const val RELEASE_ROOT = "https://github.com/silevilence/MyWubi/releases/download/"
internal const val RELEASE_API = "https://api.github.com/repos/silevilence/MyWubi/releases"

data class AppUpdate(
    val versionName: String, val versionCode: Long, val minSdk: Int,
    val notes: String, val apkUrl: String, val sha256: String, val abis: List<String>
) {
    val fullRuntime get() = apkUrl.endsWith("-Android-full.apk")
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
            require(name.matches(Regex("[0-9]+\\.[0-9]+\\.[0-9]+")) && code in 1..2_100_000_000L && min >= 26)
            require(url.matches(Regex(Regex.escape(RELEASE_ROOT) + "[vV]" + Regex.escape(name) + "/MyWubi-" + Regex.escape(name) + "-Android(?:-full)?\\.apk")))
            require(sha.matches(Regex("[a-f0-9]{64}")) && abis.isNotEmpty())
            return AppUpdate(name, code, min, json.getString("notes"), url, sha, abis)
        }
    }
}

internal fun applicableUpdate(update: AppUpdate, installed: Long, sdk: Int, abis: List<String>) =
    update.versionCode > installed && update.minSdk <= sdk && update.abis.any { it in abis }

/** No editor text, input state, or engine handles cross this boundary. */
internal fun releaseManifestUrl(release: JSONObject, full: Boolean = false): String? {
    if (release.getBoolean("draft") || release.getBoolean("prerelease")) return null
    require(!release.isNull("published_at") && release.getString("published_at").isNotBlank())
    val tag = release.getString("tag_name")
    if (!tag.matches(Regex("[vV][0-9]+\\.[0-9]+\\.[0-9]+"))) return null
    val assets = release.getJSONArray("assets")
    val name = if (full) "android-update-full.json" else "android-update.json"
    val names = (0 until assets.length()).map { assets.getJSONObject(it).getString("name") }
    val scopes = names.filter { it.startsWith("release-scope-") }
    when (scopes) {
        listOf("release-scope-windows.json") -> return null
        listOf("release-scope-android.json"), listOf("release-scope-both.json") -> {
            val suffix = if (full) "-full" else ""
            require("MyWubi-${tag.drop(1)}-Android$suffix.apk" in names) { "Android 发布缺少对应安装包" }
        }
        emptyList<String>() -> if (name !in names) return null
        else -> error("发布范围记录无效")
    }
    val urls = (0 until assets.length()).map(assets::getJSONObject)
        .filter { it.getString("name") == name }
        .map { it.getString("browser_download_url") }
    require(urls.size == 1) { "此版本尚未提供应用内更新信息，请前往发布页手动升级" }
    return urls.single().also { require(it == "${RELEASE_ROOT}$tag/$name") }
}

internal fun parseReleaseUpdate(manifestUrl: String, json: JSONObject): AppUpdate = AppUpdate.parse(json).also {
    require(it.fullRuntime == manifestUrl.endsWith("/android-update-full.json")) { "更新包类型不匹配" }
    require(it.apkUrl.substringBeforeLast('/') == manifestUrl.substringBeforeLast('/')) {
        "安装包不属于已确认的正式发布版本"
    }
}

private fun readUpdateText(address: String, maxBytes: Int): String {
    val connection = URL(address).openConnection() as HttpsURLConnection
    connection.connectTimeout = 10_000
    connection.readTimeout = 10_000
    connection.setRequestProperty("Accept", "application/json")
    connection.setRequestProperty("User-Agent", "MyWubi-Android-Updater")
    try {
        if (connection.responseCode != 200) throw java.io.IOException("更新服务暂不可用（HTTP ${connection.responseCode}），请重试")
        val bytes = connection.inputStream.use { input ->
            val output = java.io.ByteArrayOutputStream()
            val buffer = ByteArray(8192)
            val deadline = android.os.SystemClock.elapsedRealtime() + 20_000
            while (true) {
                require(android.os.SystemClock.elapsedRealtime() < deadline) { "检查超时" }
                val count = input.read(buffer)
                if (count < 0) break
                require(output.size() + count <= maxBytes) { "更新信息过大" }
                output.write(buffer, 0, count)
            }
            output.toByteArray()
        }
        return bytes.toString(Charsets.UTF_8)
    } finally { connection.disconnect() }
}

private const val PAGE_SIZE = 10

internal fun discoverAndroidUpdate(
    full: Boolean, minimumCode: Long, sdk: Int, abis: List<String>,
    page: (Int) -> JSONArray = { JSONArray(readUpdateText("$RELEASE_API?per_page=$PAGE_SIZE&page=$it", 2 * 1024 * 1024)) },
    manifest: (String) -> JSONObject = { JSONObject(readUpdateText(it, 256 * 1024)) },
): AppUpdate? {
    var index = 1
    while (true) {
        val releases = page(index)
        for (i in 0 until releases.length()) {
            val release = releases.getJSONObject(i)
            val url = releaseManifestUrl(release, full) ?: continue
            val update = try {
                parseReleaseUpdate(url, manifest(url))
            } catch (e: org.json.JSONException) {
                throw IllegalArgumentException("Android 更新清单损坏（${release.getString("tag_name")}）", e)
            } catch (e: IllegalArgumentException) {
                throw IllegalArgumentException("Android 更新清单无效（${release.getString("tag_name")}）：${e.message}", e)
            }
            // Stable releases and Android versionCode are published in increasing order.
            if (update.versionCode < minimumCode) return null
            if (update.minSdk <= sdk && update.abis.any { it in abis }) return update
        }
        if (releases.length() < PAGE_SIZE) return null
        index++
    }
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
    private val fetch: (() -> AppUpdate?)? = null) {
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
        ?.takeIf { it.fullRuntime == BuildConfig.BUNDLED_ONNX && applicableUpdate(it, installedCode, Build.VERSION.SDK_INT, Build.SUPPORTED_ABIS.toList()) }
    private val mutable = MutableStateFlow(UpdateCheckState(available = cached))
    val state = mutable.asStateFlow()

    /** Called on the main thread at keyboard-open or from the settings button, never per key. */
    fun check(manual: Boolean = false) {
        if (mutable.value.checking || (!manual && !schedule.permits(System.currentTimeMillis()))) return
        // Persist a short reservation before IO so process death cannot cause a request storm.
        schedule = schedule.copy(next = System.currentTimeMillis() + 15 * 60 * 1000L)
        prefs.edit().putLong("next", schedule.next).apply()
        mutable.value = mutable.value.copy(checking = true, message = "正在检查更新…")
        scope.launch {
            val result = runCatching { withContext(Dispatchers.IO) {
                val abis = Build.SUPPORTED_ABIS.toList()
                val candidate = if (fetch == null) discoverAndroidUpdate(
                    BuildConfig.BUNDLED_ONNX, installedCode + 1, Build.VERSION.SDK_INT, abis
                ) else fetch.invoke()
                candidate?.takeIf {
                    applicableUpdate(it, installedCode, Build.VERSION.SDK_INT, Build.SUPPORTED_ABIS.toList())
                }
            } }
            schedule = schedule.completed(System.currentTimeMillis(), result.isSuccess)
            prefs.edit().putLong("next", schedule.next).putInt("failures", schedule.failures).apply()
            result.fold(onSuccess = { update ->
                prefs.edit().putString("available", update?.json()?.toString()).apply()
                mutable.value = UpdateCheckState(available = update, message = if (update == null) "当前没有适合此设备的新版本" else "发现新版本 ${update.versionName}")
            }, onFailure = {
                mutable.value = mutable.value.copy(checking = false, message = "检查失败，未能确认最新版本。${it.localizedMessage ?: "请重试"}")
            })
        }
    }

    companion object {
        // The controller retains applicationContext only, never an Activity or IME service.
        @android.annotation.SuppressLint("StaticFieldLeak")
        @Volatile private var instance: AppUpdates? = null
        fun get(context: Context): AppUpdates = instance ?: synchronized(this) {
            instance ?: AppUpdates(context).also { instance = it }
        }
    }
}
