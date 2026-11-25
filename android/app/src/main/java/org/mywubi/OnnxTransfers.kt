package org.mywubi

import android.content.Context
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import java.util.concurrent.CancellationException
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

/** Settings and the IME share one application process. Jobs survive activity recreation. */
internal object OnnxTransfers {
    data class Status(val phase: String = "idle", val bytes: Long = 0, val message: String = "")
    private val current = MutableStateFlow(Status())
    val status = current.asStateFlow()
    private val executor = Executors.newSingleThreadExecutor()
    private var cancellation: AtomicBoolean? = null

    @Synchronized
    fun start(context: Context, finished: () -> Unit) {
        if (cancellation != null) return
        val stop = AtomicBoolean()
        cancellation = stop
        current.value = Status("downloading")
        val app = context.applicationContext
        executor.execute {
            val outcome = try {
                val root = java.io.File(app.noBackupFilesDir, "components")
                OnnxInstaller(root, OnnxComponent.payload()).install(stop::get) { bytes, verifying ->
                    current.value = Status(if (verifying) "verifying" else "downloading", bytes)
                }
                Status("ready", OnnxComponent.archiveBytes)
            } catch (e: CancellationException) {
                Status("cancelled", current.value.bytes, "已取消，可续传")
            } catch (e: Exception) {
                Status("failed", current.value.bytes, e.localizedMessage ?: "组件安装失败")
            }
            synchronized(this) {
                cancellation = null
                current.value = outcome
            }
            if (outcome.phase == "ready") finished()
        }
    }

    @Synchronized
    fun cancel() { cancellation?.set(true) }
}
