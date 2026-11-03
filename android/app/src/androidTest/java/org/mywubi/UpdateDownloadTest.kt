package org.mywubi

import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import org.junit.Before
import org.junit.Assume.assumeTrue
import java.io.File
import java.util.concurrent.atomic.AtomicInteger

/** Fixtures are separately built/signed APKs, pushed to /data/local/tmp; never release assets. */
class UpdateDownloadTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context = instrumentation.targetContext
    private val fixture get() = File("/data/local/tmp/au-new.apk")
    @Before fun requireFixtures() { assumeTrue("Run the update fixture build first", fixture.isFile) }
    @Suppress("DEPRECATION")
    private fun fixtureInfo() = requireNotNull(context.packageManager.getPackageArchiveInfo(fixture.path, 0))
    private fun update(file: File = fixture, code: Long = packageVersion(fixtureInfo())): AppUpdate {
        val info = fixtureInfo()
        val name = info.versionName!!
        return AppUpdate(name, code, info.applicationInfo!!.minSdkVersion, "更新验收专用",
            "${RELEASE_ROOT}v$name/MyWubi-$name-Android.apk", sha256(file), listOf("arm64-v8a", "x86_64"))
    }
    private fun awaitIdle(download: UpdateDownload) {
        val deadline = android.os.SystemClock.uptimeMillis() + 20000
        while (download.state.value.busy && android.os.SystemClock.uptimeMillis() < deadline) Thread.sleep(30)
        assertFalse("${download.state.value}", download.state.value.busy)
    }
    private fun create(directory: File, transfer: suspend (AppUpdate, File, (Long, Long) -> Unit) -> Unit): UpdateDownload {
        lateinit var result: UpdateDownload
        instrumentation.runOnMainSync { result = UpdateDownload(context, directory, transfer) }
        awaitIdle(result)
        return result
    }

    @Test fun archiveValidationRejectsHashPackageVersionAndSignerMismatch() {
        verifyApk(context, fixture, update())
        fun rejected(file: File, info: AppUpdate, reason: String) {
            val failure = runCatching { verifyApk(context, file, info) }.exceptionOrNull()
            assertNotNull("Expected $reason", failure)
            assertTrue("${failure?.message}", failure!!.message!!.contains(reason))
        }
        rejected(fixture, update().copy(sha256="0".repeat(64)), "校验和")
        val wrongPackage = File("/data/local/tmp/au-wrong-package.apk")
        rejected(wrongPackage, update(wrongPackage), "包名")
        val wrongSignature = File("/data/local/tmp/au-wrong-signature.apk")
        rejected(wrongSignature, update(wrongSignature), "签名")
        val old = File("/data/local/tmp/au-old.apk")
        @Suppress("DEPRECATION")
        val oldCode = packageVersion(context.packageManager.getPackageInfo(context.packageName, 0))
        rejected(old, update(old, oldCode), "相同或更旧")
        rejected(fixture, update().copy(versionCode=update().versionCode + 1), "版本")
        rejected(fixture, update().copy(minSdk=99), "系统")
    }

    @Test fun failedDownloadRetryDeduplicationRestoreAndTamperCheck() {
        val directory = File(context.cacheDir, "au-download-${System.nanoTime()}")
        val calls = AtomicInteger()
        val info = update()
        val download = create(directory) { _, target, progress ->
            if (calls.incrementAndGet() == 1) error("offline")
            delay(150)
            fixture.copyTo(target, overwrite=true)
            progress(target.length(), target.length())
        }
        instrumentation.runOnMainSync { download.start(info); download.start(info) }
        awaitIdle(download)
        assertEquals(1, calls.get())
        assertEquals(DownloadPhase.Failed, download.state.value.phase)
        instrumentation.runOnMainSync { download.start(info) }
        awaitIdle(download)
        assertEquals(DownloadPhase.Ready, download.state.value.phase)
        assertEquals(2, calls.get())
        val restored = create(directory) { _, _, _ -> error("Restore must not download") }
        assertEquals(DownloadPhase.Ready, restored.state.value.phase)
        runBlocking { assertTrue(restored.fileForInstall().isFile) }
        File(directory, "update.apk").appendBytes(byteArrayOf(1))
        assertTrue(runCatching { runBlocking { restored.fileForInstall() } }.isFailure)
        val tampered = create(directory) { _, _, _ -> error("Must not download") }
        assertEquals(DownloadPhase.Failed, tampered.state.value.phase)
        directory.deleteRecursively()
    }

    @Test fun cancelAndInterruptedDownloadRecoverWithoutAutoRestart() {
        val directory = File(context.cacheDir, "au-cancel-${System.nanoTime()}")
        val started = java.util.concurrent.CountDownLatch(1)
        val download = create(directory) { _, target, progress ->
            target.writeText("partial")
            progress(7, 100)
            started.countDown()
            delay(30000)
        }
        instrumentation.runOnMainSync { download.start(update()) }
        assertTrue(started.await(5, java.util.concurrent.TimeUnit.SECONDS))
        instrumentation.runOnMainSync { download.cancel() }
        awaitIdle(download)
        assertEquals(DownloadPhase.Cancelled, download.state.value.phase)
        assertFalse(File(directory,"update.part").exists())
        File(directory,"update.part").writeText("process died")
        val restored = create(directory) { _, _, _ -> error("Must not auto-restart") }
        assertEquals(DownloadPhase.Idle, restored.state.value.phase)
        assertFalse(File(directory,"update.part").exists())
        directory.deleteRecursively()
    }
}
