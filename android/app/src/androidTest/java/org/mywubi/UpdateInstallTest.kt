package org.mywubi

import android.content.Intent
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

class UpdateInstallTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context = instrumentation.targetContext
    private val device = UiDevice.getInstance(instrumentation)
    private fun press(text: String) {
        val node = device.wait(Until.findObject(By.text(text)), 10000) ?: error("Missing $text")
        node.click(); device.waitForIdle()
    }
    private fun prepare() {
        val file = File("/data/local/tmp/au-new.apk")
        assumeTrue("Run the update fixture build first", file.isFile)
        @Suppress("DEPRECATION")
        val info = requireNotNull(context.packageManager.getPackageArchiveInfo(file.path,0))
        val name = info.versionName!!
        val update = AppUpdate(name, packageVersion(info), info.applicationInfo!!.minSdkVersion, "签名覆盖安装验收", "${RELEASE_ROOT}v$name/MyWubi-$name-Android.apk", sha256(file), listOf("x86_64", "arm64-v8a"))
        lateinit var checks: AppUpdates
        lateinit var download: UpdateDownload
        instrumentation.runOnMainSync {
            checks = AppUpdates(context, "install-check-test") { update }
            checks.check(manual=true)
            download = UpdateDownload(context) { _, target, progress ->
                file.copyTo(target, overwrite=true); progress(file.length(), file.length())
            }
        }
        fun await(condition: () -> Boolean) {
            val end = android.os.SystemClock.uptimeMillis() + 20000
            while (!condition() && android.os.SystemClock.uptimeMillis() < end) Thread.sleep(30)
            assertTrue(condition())
        }
        await { !checks.state.value.checking && !download.state.value.busy }
        assertEquals(update, checks.state.value.available)
        instrumentation.runOnMainSync { download.start(update) }
        await { !download.state.value.busy }
        assertEquals("${download.state.value}", DownloadPhase.Ready, download.state.value.phase)
    }

    @Test fun permissionDenialAndInstallerCancellationRemainRecoverable() {
        prepare()
        assertFalse("Revoke permission before starting instrumentation; revocation kills the app process", context.packageManager.canRequestPackageInstalls())
        ActivityScenario.launch(UpdateActivity::class.java).use { scenario ->
            press("安装更新"); press("继续安装")
            assertTrue(device.wait(Until.hasObject(By.text("Allow from this source")), 10000))
            device.pressBack()
            assertTrue(device.wait(Until.hasObject(By.textContains("未授予安装权限")), 10000))
            press("安装更新"); press("继续安装")
            press("Allow from this source")
            device.pressBack()
            assertTrue(device.wait(Until.hasObject(By.textContains("已允许安装")), 10000))
            press("安装更新"); press("继续安装")
            assertTrue(device.wait(Until.hasObject(By.text("Cancel").pkg("com.google.android.packageinstaller")), 10000))
            device.takeScreenshot(File(context.filesDir,"au-install-confirmation.png"))
            press("Cancel")
            assertTrue(device.wait(Until.hasObject(By.textContains("安装未完成或已取消")), 10000))
            scenario.recreate()
            assertTrue(device.wait(Until.hasObject(By.text("安装更新")), 10000))
        }
    }

    @Test fun completeSystemUpgrade() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("perform_upgrade") == "true")
        prepare()
        val engine = File(context.filesDir,"engine")
        Resources.prepare(context)
        val store = EngineStore(context)
        val importedRoot = File(engine,"packages/au-upgrade-test").apply {mkdirs()}
        val files = listOf("wubi86.dict","pinyin.dict","english.dict").map { name ->
            val target = File(importedRoot,name)
            File(engine,"tables/$name").copyTo(target,overwrite=true)
            EngineStore.ImportFile(name,target)
        }
        val config = store.readConfig()
        NativeHandle(config.getLong("version")).use { version ->
            val activated = store.activate(EngineStore.ImportSet(importedRoot,files),0,1,2,version.value)
            NativeHandle(activated.getLong("version")).close()
        }
        val user = File(engine,"user.dict")
        if (!user.exists()) user.writeText("zz\t升级保留词\t1\n")
        assertTrue(File(engine,"user.dict").exists())
        assertTrue(File(engine,"packages").walkTopDown().any { it.isFile })
        val hashes = JSONObject()
        engine.walkTopDown().filter { it.isFile }.forEach { hashes.put(it.relativeTo(engine).path,sha256(it)) }
        File(context.filesDir,"au-data-before.json").writeText(hashes.toString())
        context.startActivity(Intent(context,UpdateActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        press("安装更新"); press("继续安装")
        // Updating this package ends its instrumentation process; post-upgrade assertions run separately.
        press("Update")
        Thread.sleep(15000)
    }

    @Test fun upgradePreservedUserData() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("verify_upgrade") == "true")
        @Suppress("DEPRECATION")
        val installed = context.packageManager.getPackageInfo(context.packageName, 0)
        @Suppress("DEPRECATION")
        val expected = requireNotNull(context.packageManager.getPackageArchiveInfo("/data/local/tmp/au-new.apk",0))
        assertEquals(packageVersion(expected), packageVersion(installed))
        val hashes = JSONObject(File(context.filesDir,"au-data-before.json").readText())
        val engine = File(context.filesDir,"engine")
        for (path in hashes.keys()) assertEquals(path, hashes.getString(path), sha256(File(engine,path)))
        lateinit var download: UpdateDownload
        instrumentation.runOnMainSync { download = UpdateDownload.get(context) }
        val end = android.os.SystemClock.uptimeMillis() + 15000
        while(download.state.value.busy && android.os.SystemClock.uptimeMillis()<end) Thread.sleep(50)
        assertEquals(DownloadPhase.Idle,download.state.value.phase)
        assertFalse(File(context.filesDir,"app-update/update.apk").exists())
        assertNull(AppUpdates.get(context).state.value.available)
    }
}
