package org.mywubi

import java.net.ServerSocket
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.ByteArrayOutputStream
import java.io.File
import kotlin.concurrent.thread
import java.net.URL
import java.security.MessageDigest
import java.util.concurrent.CancellationException
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

class OnnxInstallerTest {
    @get:Rule val temp = TemporaryFolder()
    private val data = ByteArray(200_000).also { java.util.Random(53).nextBytes(it) }
    private fun hash(bytes: ByteArray) = MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }
    private val payload = OnnxComponent.Payload("arm64-v8a", data.size.toLong(), hash(data))
    private fun archive(name: String): ByteArray = ByteArrayOutputStream().also { out ->
        ZipOutputStream(out).use { zip ->
            zip.putNextEntry(ZipEntry(name))
            zip.write(data)
        }
    }.toByteArray()
    private val member = "jni/arm64-v8a/libonnxruntime.so"
    private val root get() = temp.root
    private val partial get() = File(root, "onnx-${OnnxComponent.version}.aar.part")
    private fun installer(bytes: ByteArray, space: Long = Long.MAX_VALUE, url: URL = URL("http://localhost:1")) =
        OnnxInstaller(root, payload, url, bytes.size.toLong(), hash(bytes)) { space }

    @Test fun cancelledDownloadResumesWithExactByteRangeAfterRecreation() {
        val bytes = archive(member)
        val server = ServerSocket(0)
        var range: String? = null
        val responder = thread(isDaemon = true) {
            repeat(2) {
                server.accept().use { socket ->
                    val headers = socket.getInputStream().bufferedReader()
                    range = generateSequence { headers.readLine() }.takeWhile { it.isNotEmpty() }
                        .firstOrNull { it.startsWith("Range:", ignoreCase = true) }?.substringAfter(':')?.trim()
                    val start = range?.removePrefix("bytes=")?.removeSuffix("-")?.toInt() ?: 0
                    val status = if (start > 0) "206 Partial Content" else "200 OK"
                    val contentRange = if (start > 0) "Content-Range: bytes $start-${bytes.lastIndex}/${bytes.size}\r\n" else ""
                    val out = socket.getOutputStream()
                    out.write("HTTP/1.1 $status\r\n${contentRange}Content-Length: ${bytes.size-start}\r\nConnection: close\r\n\r\n".toByteArray())
                    runCatching { out.write(bytes, start, bytes.size - start) }
                }
            }
        }
        try {
            val url = URL("http://127.0.0.1:${server.localPort}/")
            var stop = false
            assertThrows(CancellationException::class.java) {
                installer(bytes, url = url).install({ stop }) { _, _ -> stop = true }
            }
            val offset = partial.length()
            assertTrue(offset in 1 until bytes.size.toLong())
            val resumed = installer(bytes, url = url)
            resumed.install({ false }) { _, _ -> }
            assertEquals("bytes=$offset-", range)
            assertArrayEquals(data, resumed.library.readBytes())
            assertFalse(partial.exists())
        } finally { server.close(); responder.join(5000) }
    }

    @Test fun wrongAbiTraversalAndCorruptionPreservePreviouslyInstalledLibrary() {
        for (memberName in listOf("jni/x86_64/libonnxruntime.so", "../libonnxruntime.so", member)) {
            val archive = archive(memberName)
            partial.writeBytes(archive)
            val instance = installer(archive)
            instance.library.parentFile!!.mkdirs()
            instance.library.writeText("previous")
            if (memberName == member) partial.writeBytes(archive.clone().also { it[0] = 0 })
            assertThrows(IllegalStateException::class.java) { instance.install({ false }) { _, _ -> } }
            assertEquals("previous", instance.library.readText())
            assertFalse(File(root, "libonnxruntime.so").exists())
        }
    }

    @Test fun incompleteDownloadAndInsufficientSpaceNeverPublish() {
        val archive = archive(member)
        partial.writeBytes(archive.copyOf(8))
        val instance = installer(archive, 0)
        assertThrows(IllegalStateException::class.java) { instance.install({ false }) { _, _ -> } }
        assertThrows(java.io.IOException::class.java) { installer(archive).install({ false }) { _, _ -> } }
        assertEquals(8, partial.length())
        assertFalse(instance.library.exists())
    }
}
