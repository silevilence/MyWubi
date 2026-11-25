package org.mywubi

import java.io.File
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.security.MessageDigest
import java.util.concurrent.CancellationException
import java.util.zip.ZipFile

/** Only the APK's pinned manifest supplies production download and extraction parameters. */
internal class OnnxInstaller(
    private val directory: File,
    private val payload: OnnxComponent.Payload,
    private val source: URL = URL(OnnxComponent.url),
    private val archiveSize: Long = OnnxComponent.archiveBytes,
    private val archiveHash: String = OnnxComponent.archiveHash,
    private val available: (File) -> Long = File::getUsableSpace
) {
    private val part = File(directory, "onnx-${OnnxComponent.version}.aar.part")
    val library = File(directory, "onnx-${OnnxComponent.version}/${payload.abi}/${OnnxComponent.library}")

    fun install(cancelled: () -> Boolean, progress: (Long, Boolean) -> Unit) {
        fun checkpoint() { if (cancelled()) throw CancellationException("已取消，可继续下载") }
        check(directory.isDirectory || directory.mkdirs()) { "无法创建组件目录" }
        if (part.length() > archiveSize) Files.delete(part.toPath())
        val stage = File(library.parentFile, "runtime.partial")
        Files.deleteIfExists(stage.toPath())
        check(available(directory) >= archiveSize - part.length() + payload.bytes) { "组件下载及安装空间不足" }
        checkpoint()
        if (part.length() < archiveSize) {
            val connection = source.openConnection() as HttpURLConnection
            connection.connectTimeout = 15_000
            connection.readTimeout = 20_000
            connection.instanceFollowRedirects = false
            connection.setRequestProperty("Accept-Encoding", "identity")
            val offset = part.length()
            if (offset > 0) connection.setRequestProperty("Range", "bytes=$offset-")
            try {
                val append = when (val code = connection.responseCode) {
                    HttpURLConnection.HTTP_OK -> false
                    HttpURLConnection.HTTP_PARTIAL -> {
                        check(connection.getHeaderField("Content-Range") == "bytes $offset-${archiveSize - 1}/$archiveSize") {
                            "续传位置不匹配"
                        }
                        true
                    }
                    else -> throw IOException("下载服务器返回 HTTP $code")
                }
                var received = if (append) offset else 0L
                connection.inputStream.use { input ->
                    java.io.FileOutputStream(part, append).use { output ->
                        val buffer = ByteArray(64 * 1024)
                        while (true) {
                            checkpoint()
                            val count = input.read(buffer)
                            if (count < 0) break
                            check(received + count <= archiveSize) { "下载内容超出清单大小" }
                            output.write(buffer, 0, count)
                            received += count
                            progress(received, false)
                        }
                    }
                }
                check(received == archiveSize) { "下载中断，可继续下载" }
            } finally { connection.disconnect() }
        }
        checkpoint()
        progress(archiveSize, true)
        if (!matches(part, archiveSize, archiveHash)) {
            Files.delete(part.toPath())
            error("AAR 校验失败，请重试下载")
        }
        check(library.parentFile!!.isDirectory || library.parentFile!!.mkdirs()) { "无法创建安装目录" }
        try {
            ZipFile(part).use { archive ->
                // Only this exact member is read. Archive names never become filesystem paths.
                val entry = archive.getEntry("jni/${payload.abi}/${OnnxComponent.library}")
                check(entry != null && entry.size == payload.bytes) { "运行库架构或大小不匹配" }
                archive.getInputStream(entry).use { input ->
                    stage.outputStream().use { output ->
                        val buffer = ByteArray(64 * 1024)
                        var size = 0L
                        while (true) {
                            checkpoint()
                            val count = input.read(buffer)
                            if (count < 0) break
                            size += count
                            check(size <= payload.bytes) { "运行库超出清单大小" }
                            output.write(buffer, 0, count)
                        }
                        output.fd.sync()
                    }
                }
            }
            check(matches(stage, payload.bytes, payload.hash)) { "运行库校验失败" }
            check(stage.setReadOnly()) { "无法保护已校验运行库" }
            checkpoint()
            Files.move(stage.toPath(), library.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
        } finally { Files.deleteIfExists(stage.toPath()) }
        Files.deleteIfExists(part.toPath())
    }

    companion object {
        fun matches(file: File, bytes: Long, expected: String): Boolean {
            if (!file.isFile || file.length() != bytes) return false
            val digest = MessageDigest.getInstance("SHA-256")
            file.inputStream().use { input ->
                val buffer = ByteArray(64 * 1024)
                while (true) {
                    val count = input.read(buffer)
                    if (count < 0) break
                    digest.update(buffer, 0, count)
                }
            }
            return digest.digest().joinToString("") { "%02x".format(it) } == expected
        }
    }
}
