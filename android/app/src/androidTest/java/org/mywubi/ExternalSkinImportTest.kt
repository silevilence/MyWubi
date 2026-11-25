package org.mywubi

import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.junit.Assert.*
import org.junit.Test
import java.io.File

class ExternalSkinImportTest {
    @Test fun systemFilesOpensADownloadIntoTheSameConfirmationPage() {
        val ins = InstrumentationRegistry.getInstrumentation()
        val context = ins.targetContext
        val device = UiDevice.getInstance(ins)
        val name = "MyWubi-test-${System.nanoTime()}.mywubi-skin"
        val resolver = context.contentResolver
        val uri = requireNotNull(resolver.insert(android.provider.MediaStore.Downloads.EXTERNAL_CONTENT_URI,
            android.content.ContentValues().apply {
                put(android.provider.MediaStore.MediaColumns.DISPLAY_NAME, name)
                put(android.provider.MediaStore.MediaColumns.RELATIVE_PATH, "Download")
                put(android.provider.MediaStore.MediaColumns.MIME_TYPE, "application/octet-stream")
            }))
        try {
            ins.context.assets.open("skins/editor-jade.mywubi-skin").use { source ->
                requireNotNull(resolver.openOutputStream(uri)).use { source.copyTo(it) }
            }
            val downloads = android.provider.DocumentsContract.buildDocumentUri("com.android.externalstorage.documents", "primary:Download")
            context.startActivity(android.content.Intent(android.content.Intent.ACTION_VIEW)
                .setDataAndType(downloads, android.provider.DocumentsContract.Document.MIME_TYPE_DIR)
                .addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK))
            requireNotNull(findFresh(device, By.textStartsWith(name), 12000)) { "Download missing from system Files" }.click()
            val end = android.os.SystemClock.uptimeMillis() + 15000
            while (android.os.SystemClock.uptimeMillis() < end && !device.hasObject(By.text("确认导入"))) {
                device.findObject(By.text("MyWubi"))?.actionNode()?.click()
                device.findObject(By.res("android", "button_once"))?.click()
                android.os.SystemClock.sleep(150)
                ins.uiAutomation.clearCache()
            }
            requireNotNull(findFresh(device, By.text("确认导入"), 10000))
            requireNotNull(findFresh(device, By.text("取消"), 5000)).actionNode().click()
        } catch (failure: Throwable) {
            device.dumpWindowHierarchy(File(context.filesDir, "skin-files-failure.xml"))
            throw failure
        } finally { resolver.delete(uri, null, null) }
    }

    @Test fun openShareAndChatHandoffsRequireConfirmationAndPreserveTheActiveSkin() {
        val ins = InstrumentationRegistry.getInstrumentation()
        val context = ins.targetContext
        val device = UiDevice.getInstance(ins)
        val path = Resources.prepare(context)
        val config = path.readBytes()
        val installed = File(path.parentFile, "skins/mywubi.bamboo.android@1.mywubi-skin")
        val before = installed.takeIf { it.exists() }?.readBytes()
        fun send(mime: String, share: Boolean = false, chooser: Boolean = false, deny: Boolean = false) {
            device.executeShellCommand("am start -W -f 0x18000000 -n org.mywubi.acceptancehost/.SkinSenderActivity --es mime $mime" +
                " --ez share $share --ez chooser $chooser --ez deny $deny")
        }
        fun node(label: String) = requireNotNull(findFresh(device, By.text(label), 12000)) { "Missing $label" }
        fun action(label: String) = node(label).actionNode().click()
        try {
            send(SKIN_MIME)
            node("竹纸 · 1")
            action("取消")
            if (before == null) assertFalse(installed.exists()) else assertArrayEquals(before, installed.readBytes())

            // Sharesheet, then two chat attachment handoffs modeled on Android's VIEW/SEND contracts.
            send(SKIN_MIME, share = true, chooser = true)
            action("MyWubi")
            action("确认导入")
            node(if (before == null) "皮肤已导入" else "皮肤已在库中")
            assertTrue(installed.isFile)
            action("预览")
            node("预览 · 竹纸")
            action("关闭预览")

            for ((mime, share) in listOf("application/octet-stream" to false, "application/zip" to true)) {
                send(mime, share)
                node("竹纸 · 1")
                action("确认导入")
                node("皮肤已在库中")
                action("关闭")
            }
            send("application/octet-stream", deny = true)
            node("没有文件读取权限。请重新分享，或将文件保存到下载目录后从设置中导入。")
            action("关闭")
            assertArrayEquals(config, path.readBytes())
        } catch (failure: Throwable) {
            device.dumpWindowHierarchy(File(context.filesDir, "skin-import-failure.xml"))
            throw failure
        } finally {
            if (before == null && installed.exists()) {
                NativeBridge.objectCall("ManageSkin", "request" to keyboardRequest(path, "Delete")
                    .put("id", "mywubi.bamboo.android").put("version", "1"))
            }
        }
    }
}
