package org.mywubi

import android.content.Context
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import org.json.JSONObject
import java.io.File
import java.net.URL
import javax.net.ssl.HttpsURLConnection

enum class DownloadPhase { Restoring, Idle, Downloading, Cancelling, Verifying, Ready, Cancelled, Failed }
data class UpdateDownloadState(val phase: DownloadPhase = DownloadPhase.Restoring, val update: AppUpdate? = null,
    val received: Long = 0, val total: Long = 0, val message: String = "正在恢复下载状态…") {
    val busy get() = phase in setOf(DownloadPhase.Restoring, DownloadPhase.Downloading, DownloadPhase.Cancelling, DownloadPhase.Verifying)
}

/** Only the settings Activity creates this controller; keyboard checks cannot start a download. */
class UpdateDownload internal constructor(context: Context,
    private val directory: File = File(context.filesDir, "app-update"),
    private val transfer: suspend (AppUpdate, File, (Long, Long) -> Unit) -> Unit = ::downloadApk) {
    private val app = context.applicationContext
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val mutable = MutableStateFlow(UpdateDownloadState())
    val state = mutable.asStateFlow()
    private var job: Job? = null
    // A different verified payload must also have a different FileProvider URI.
    private fun apk(update: AppUpdate): File {
        require(update.sha256.matches(Regex("[a-f0-9]{64}")))
        return File(directory, "MyWubi-${update.versionCode}-${update.sha256}.apk")
    }
    private val legacyApk get() = File(directory, "update.apk")
    private fun removeObsoleteApks(keep: File? = null) {
        val name = Regex("MyWubi-[0-9]+-[a-f0-9]{64}[.]apk")
        directory.listFiles()?.filter { it != keep && (it == legacyApk || it.name.matches(name)) }
            ?.forEach { check(it.delete()) { "无法清理旧安装包" } }
    }
    private val partial get() = File(directory, "update.part")
    private val record get() = File(directory, "update.json")

    init {
        job = scope.launch {
            val result = withContext(Dispatchers.IO) { runCatching {
                check(directory.isDirectory || directory.mkdirs()) { "无法创建下载目录" }
                if (partial.exists()) check(partial.delete()) { "无法清理中断的下载" }
                if (!record.exists()) null else {
                    require(record.length() <= 256 * 1024)
                    val update = AppUpdate.parse(JSONObject(record.readText()))
                    @Suppress("DEPRECATION")
                    val installed = packageVersion(app.packageManager.getPackageInfo(app.packageName, 0))
                    if (update.versionCode <= installed) {
                        check(record.delete())
                        removeObsoleteApks()
                        null
                    } else update.also {
                        val current = apk(it)
                        if (!current.exists() && legacyApk.isFile) {
                            verifyApk(app, legacyApk, it)
                            java.nio.file.Files.move(legacyApk.toPath(), current.toPath())
                        }
                        verifyApk(app, current, it)
                    }
                }
            } }
            mutable.value = result.fold(
                { if (it == null) UpdateDownloadState(DownloadPhase.Idle, message="尚未下载") else UpdateDownloadState(DownloadPhase.Ready, it, message="安装包已校验，可继续安装") },
                { UpdateDownloadState(DownloadPhase.Failed, message="上次下载不可用或已完成升级，请重新检查更新") })
            job = null
        }
    }

    fun start(update: AppUpdate) {
        if (job != null) return
        mutable.value = UpdateDownloadState(DownloadPhase.Downloading, update, message="正在下载…")
        job = scope.launch {
            var completed: UpdateDownloadState
            try {
                withContext(Dispatchers.IO) {
                    check(directory.isDirectory || directory.mkdirs())
                    // Invalidate the previous receipt before replacing any bytes.
                    if (record.exists()) check(record.delete())
                    transfer(update, partial) { received, total ->
                        mutable.update { current ->
                            if (current.phase == DownloadPhase.Cancelling) current
                            else UpdateDownloadState(DownloadPhase.Downloading, update, received, total, "正在下载…")
                        }
                    }
                    ensureActive()
                    mutable.value = mutable.value.copy(phase=DownloadPhase.Verifying, message="正在校验安装包…")
                    verifyApk(app, partial, update)
                    ensureActive()
                    val verified = apk(update)
                    java.nio.file.Files.move(partial.toPath(), verified.toPath(), java.nio.file.StandardCopyOption.REPLACE_EXISTING)
                    // A missing/partial receipt after process death fails closed on restore.
                    record.writeText(update.json().toString())
                    removeObsoleteApks(verified)
                }
                completed = UpdateDownloadState(DownloadPhase.Ready, update, message="下载完成，安装包已校验")
            } catch (_: CancellationException) {
                completed = UpdateDownloadState(DownloadPhase.Cancelled, update, message="已取消下载，可重新下载")
            } catch (error: Exception) {
                completed = UpdateDownloadState(DownloadPhase.Failed, update, message="下载或校验失败：${error.message ?: "请重试"}")
            }
            // Keep cleanup AND final publication outside the cancelled job. Returning from an
            // IO-only NonCancellable block otherwise rethrows before the UI leaves Downloading.
            withContext(NonCancellable) {
                val cleaned = withContext(Dispatchers.IO) { !partial.exists() || partial.delete() }
                if (!cleaned) {
                    completed = UpdateDownloadState(DownloadPhase.Failed, update, message="无法清理未完成的安装包，请重试")
                }
                job = null
                mutable.value = completed
            }
        }
    }

    fun cancel() {
        if (mutable.value.phase == DownloadPhase.Downloading) {
            mutable.value = mutable.value.copy(phase=DownloadPhase.Cancelling, message="正在取消下载…")
            job?.cancel()
        }
    }

    /** Revalidate immediately before granting a system installer read access. */
    suspend fun fileForInstall(): File {
        val ready = state.value
        check(ready.phase == DownloadPhase.Ready && job == null) { "安装包尚未准备好" }
        return withContext(Dispatchers.IO) {
            val update = requireNotNull(ready.update)
            apk(update).also { verifyApk(app, it, update) }
        }
    }

    fun installResult(message: String) {
        if (mutable.value.phase == DownloadPhase.Ready) mutable.value = mutable.value.copy(message=message)
    }

    companion object {
        // Only applicationContext is retained for process-wide download ownership.
        @android.annotation.SuppressLint("StaticFieldLeak")
        @Volatile private var instance: UpdateDownload? = null
        fun get(context: Context): UpdateDownload = instance ?: synchronized(this) {
            instance ?: UpdateDownload(context).also { instance = it }
        }
    }
}

private suspend fun downloadApk(update: AppUpdate, target: File, progress: (Long, Long) -> Unit) {
    val connection = URL(update.apkUrl).openConnection() as HttpsURLConnection
    connection.connectTimeout = 10_000
    connection.readTimeout = 10_000
    connection.setRequestProperty("User-Agent", "MyWubi-Android-Updater")
    try {
        require(connection.responseCode == 200) { "HTTP ${connection.responseCode}，请重试" }
        val total = connection.contentLengthLong
        require(total <= MAX_APK_BYTES) { "安装包过大" }
        connection.inputStream.use { input -> target.outputStream().use { output ->
            val buffer = ByteArray(64 * 1024)
            var received = 0L
            var reported = 0L
            val deadline = android.os.SystemClock.elapsedRealtime() + 30 * 60 * 1000L
            while (true) {
                currentCoroutineContext().ensureActive()
                require(android.os.SystemClock.elapsedRealtime() < deadline) { "下载超时，请重试" }
                val count = input.read(buffer)
                if (count < 0) break
                received += count
                require(received <= MAX_APK_BYTES) { "安装包过大" }
                output.write(buffer, 0, count)
                val now = android.os.SystemClock.elapsedRealtime()
                if (now - reported >= 150) { progress(received, total); reported = now }
            }
            require(total < 0 || received == total) { "下载不完整，请重试" }
            output.fd.sync()
            progress(received, total)
        } }
    } finally { connection.disconnect() }
}
