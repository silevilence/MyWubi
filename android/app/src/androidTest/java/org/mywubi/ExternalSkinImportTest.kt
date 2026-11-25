package org.mywubi

import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.junit.Assert.*
import org.junit.Test
import java.io.File

class ExternalSkinImportTest {
    @Test fun openShareAndChatHandoffsRequireConfirmationAndPreserveTheActiveSkin() {
        val ins = InstrumentationRegistry.getInstrumentation()
        val context = ins.targetContext
        val device = UiDevice.getInstance(ins)
        val path = Resources.prepare(context)
        val config = path.readBytes()
        val installed = File(path.parentFile, "skins/mywubi.bamboo.android@1.mywubi-skin")
        val before = installed.takeIf { it.exists() }?.readBytes()
        fun send(mime: String, share: Boolean = false, chooser: Boolean = false, deny: Boolean = false) {
            device.executeShellCommand("am start -n org.mywubi.acceptancehost/.SkinSenderActivity --es mime $mime" +
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
