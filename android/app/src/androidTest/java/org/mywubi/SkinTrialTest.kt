package org.mywubi

import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.junit.Assert.*
import org.junit.Test
import java.io.File

class SkinTrialTest {
    @Test fun previewAndTrialKeepLayoutLocalToTheMarkedEditor() {
        val ins = InstrumentationRegistry.getInstrumentation()
        val device = UiDevice.getInstance(ins)
        val context = ins.targetContext
        val path = Resources.prepare(context)
        NativeBridge.objectCall("ManageKeyboard", "request" to keyboardRequest(path, "InstallTemplates"))
        val configBefore = path.readBytes()
        fun choices() = listOf("Main", "Pinyin", "English").map { mode ->
            NativeBridge.objectCall("ManageKeyboard", "request" to keyboardRequest(path, "List").put("mode", mode)).optJSONObject("choice")?.toString()
        }
        val before = choices()
        val previousIme = device.executeShellCommand("settings get secure default_input_method").trim()
        val ime = "${context.packageName}/org.mywubi.MyWubiService"
        val wasEnabled = device.executeShellCommand("ime list -s").lineSequence().any { it.substringBefore('/') == context.packageName }
        fun node(selector: BySelector) = requireNotNull(findFresh(device, selector, 10000)) { "Missing $selector" }
        fun key(label: String) = node(By.desc(label).pkg(context.packageName).enabled(true)).click()
        fun text(label: String) = node(By.text(label)).actionNode().click()
        val editor = By.pkg(context.packageName).clazz("android.widget.EditText")
        try {
            device.executeShellCommand("ime enable $ime")
            device.executeShellCommand("ime set $ime")
            ActivityScenario.launch(MainActivity::class.java).use {
                clickSetting(device, "皮肤")
                clickSetting(device, "预览「内置键盘作者模板 · 4」", true)
                fun choose(label: String) {
                    node(By.textStartsWith("键盘 · ")).actionNode().click()
                    text(label)
                }
                for ((label, firstKey) in listOf("full26 · 形码" to "q", "pinyin9 · 拼音" to "ABC",
                        "shape9 · 形码" to "ABC", "shape14 · 形码" to "GH")) {
                    choose(label)
                    for (direction in listOf("竖屏", "横屏")) {
                        text(direction)
                        node(By.desc(firstKey).pkg(context.packageName))
                        node(By.descStartsWith("候选 工"))
                    }
                }
                text("试打")
                key("GH"); key("GH"); key("L"); key("L")
                node(By.descStartsWith("候选 一")).click()
                assertTrue(awaitHostText(device, editor, "一", 5000))
                key("退格")
                assertTrue(node(editor).isFocused)
                fun field(label: String) {
                    node(By.textStartsWith("文本框类型 · ")).actionNode().click()
                    text(label)
                    assertTrue(node(editor).isFocused)
                }
                field("多行文本")
                key("换行")
                assertTrue(awaitHostText(device, editor, "\n", 5000))
                for (label in listOf("数字", "电话")) {
                    field(label); key("2")
                    assertTrue(awaitHostText(device, editor, "2", 5000))
                }
                field("邮箱"); key("a")
                assertTrue(awaitHostText(device, editor, "a", 5000))
                field("网址"); node(By.desc("GH"))
                field("密码"); node(By.desc("安全输入")); assertFalse(device.hasObject(By.desc("GH")))
                field("普通文本"); node(By.desc("GH"))
                device.executeShellCommand("am start -n org.mywubi.acceptancehost/.EditorActivity")
                focusExternalEditor(device)
                node(By.desc("q").pkg(context.packageName))
                assertFalse(device.hasObject(By.desc("GH")))
                device.executeShellCommand("am force-stop org.mywubi.acceptancehost")
                node(By.text("皮肤试打"))
                text("返回预览")
                node(By.textStartsWith("预览 · "))
                assertEquals(before, choices())
                assertArrayEquals(configBefore, path.readBytes())
            }
        } catch (failure: Throwable) {
            device.dumpWindowHierarchy(File(context.filesDir, "skin-trial-failure.xml"))
            throw failure
        } finally {
            if (previousIme.contains('/')) device.executeShellCommand("ime set $previousIme")
            if (!wasEnabled) device.executeShellCommand("ime disable $ime")
        }
    }
}
