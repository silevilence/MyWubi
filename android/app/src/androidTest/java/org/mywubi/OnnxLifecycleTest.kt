package org.mywubi

import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class OnnxLifecycleTest {
    private val app = InstrumentationRegistry.getInstrumentation().targetContext
    private fun isolated() = assumeTrue(app.packageName == "org.mywubi.miaccept")

    private fun await(expected: OnnxRuntime.State) {
        val deadline = android.os.SystemClock.elapsedRealtime() + 10000
        while (OnnxRuntime.status.value != expected && android.os.SystemClock.elapsedRealtime() < deadline) {
            android.os.SystemClock.sleep(30)
        }
        assertEquals(expected, OnnxRuntime.status.value)
    }

    private fun remove(): String {
        val latch = CountDownLatch(1)
        var result = ""
        OnnxRuntime.remove(app) { message -> result = message; latch.countDown() }
        assertTrue(latch.await(20, TimeUnit.SECONDS))
        return result
    }

    @Test fun recordInstalledPackageBytes() {
        isolated()
        val stats = app.getSystemService(android.app.usage.StorageStatsManager::class.java)
            .queryStatsForPackage(android.os.storage.StorageManager.UUID_DEFAULT, app.packageName, android.os.Process.myUserHandle())
        assertTrue(stats.appBytes > 0)
        java.io.File(app.filesDir, "installed-bytes.txt").writeText(stats.appBytes.toString())
    }

    @Test fun rejectDamagedLibraryBeforeFirstLoad() {
        isolated()
        val file = OnnxComponent.installed(app)
        assertFalse(file.exists())
        file.parentFile!!.mkdirs()
        file.writeText("incomplete runtime")
        OnnxRuntime.refresh(app)
        await(OnnxRuntime.State.Corrupt)
        assertEquals("组件已移除", remove())
        await(OnnxRuntime.State.Missing)
        assertFalse(file.exists())
    }

    @Test fun installCachedComponentForProcessAbi() {
        isolated()
        val expected = InstrumentationRegistry.getArguments().getString("expected_abi")
        assumeTrue("Supply expected_abi and the pinned AAR", expected != null)
        val payload = OnnxComponent.payload()
        assertEquals(expected, payload.abi)
        val translated = OnnxComponent.requiresNativeApk()
        OnnxRuntime.refresh(app)
        await(if (translated) OnnxRuntime.State.NativeApkRequired else OnnxRuntime.State.Missing)
        val archive = java.io.File(app.noBackupFilesDir, "components/onnx-${OnnxComponent.version}.aar.part")
        assertTrue(OnnxInstaller.matches(archive, OnnxComponent.archiveBytes, OnnxComponent.archiveHash))
        val installed = CountDownLatch(1)
        OnnxTransfers.start(app) { OnnxRuntime.refresh(app); installed.countDown() }
        val completed = installed.await(30, TimeUnit.SECONDS)
        assertTrue("Component installation failed: ${OnnxTransfers.status.value}", completed)
        await(if (translated) OnnxRuntime.State.NativeApkRequired else OnnxRuntime.State.Ready)
        val library = OnnxComponent.installed(app)
        assertEquals(expected, library.parentFile!!.name)
        assertTrue(OnnxInstaller.matches(library, payload.bytes, payload.hash))
    }

    @Test fun rejectedUpdatePreservesInstalledRuntime() {
        isolated()
        val payload = OnnxComponent.payload()
        val installed = OnnxComponent.installed(app)
        assertFalse(OnnxComponent.bundled(app).isFile)
        assertTrue(OnnxInstaller.matches(installed, payload.bytes, payload.hash))
        val part = java.io.File(app.noBackupFilesDir, "components/onnx-${OnnxComponent.version}.aar.part")
        assertFalse(part.exists())
        try {
            // A complete-length, invalid archive exercises verification without a second download.
            java.io.RandomAccessFile(part, "rw").use { it.setLength(OnnxComponent.archiveBytes) }
            OnnxTransfers.start(app) { OnnxRuntime.refresh(app) }
            val deadline = android.os.SystemClock.elapsedRealtime() + 30000
            while (OnnxTransfers.status.value.phase in setOf("downloading", "verifying") &&
                android.os.SystemClock.elapsedRealtime() < deadline) android.os.SystemClock.sleep(30)
            val result = OnnxTransfers.status.value
            assertEquals("failed", result.phase)
            assertTrue(result.message, result.message.contains("AAR 校验失败"))
            assertFalse(part.exists())
            assertTrue(OnnxInstaller.matches(installed, payload.bytes, payload.hash))
        } finally { part.delete() }
    }

    @Test fun translatedRuntimeRequiresNativeApkBeforeLoadingOnnx() {
        isolated()
        assumeTrue(OnnxComponent.requiresNativeApk())
        assertEquals("arm64-v8a", OnnxComponent.payload().abi)
        OnnxRuntime.refresh(app)
        await(OnnxRuntime.State.NativeApkRequired)
        NativeHandle(EngineStore(app).readConfig().getLong("version")).close()
    }

    @Test fun protectSelectedModelThenDeferRemovalUntilRestart() {
        isolated()
        OnnxRuntime.refresh(app)
        await(OnnxRuntime.State.Ready)
        val store = EngineStore(app)
        val saved = store.readConfig()
        NativeHandle(saved.getLong("version")).close()
        val original = saved.getJSONObject("config")
        fun save(config: JSONObject) {
            NativeHandle(store.readConfig().getLong("version")).use {
                NativeHandle(store.saveConfig(config, it.value).getLong("version")).close()
            }
        }
        try {
            ModelCatalog(app).call("Import", "source" to ModelBackendTest().fixture().absolutePath)
            val selected = JSONObject(original.toString())
            selected.getJSONObject("prediction").put("model", JSONObject().put("id", "instrumented-counts").put("version", "v1"))
            save(selected)
            assertTrue(remove().contains("请先选择"))
            await(OnnxRuntime.State.Ready)
        } finally { save(original) }
        assertTrue(remove().contains("重启"))
        await(OnnxRuntime.State.PendingRemoval)
        assertTrue(OnnxComponent.installed(app).isFile)
    }

    @Test fun nextProcessFinishesDeferredRemoval() {
        isolated()
        OnnxRuntime.refresh(app)
        await(OnnxRuntime.State.Missing)
        assertFalse(OnnxComponent.installed(app).exists())
    }
}
