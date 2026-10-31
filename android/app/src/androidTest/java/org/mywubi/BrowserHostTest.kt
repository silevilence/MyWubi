package org.mywubi

import android.os.SystemClock
import android.view.KeyEvent
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.UiObject2
import org.junit.Assert.*
import org.junit.Test

/** Real Chrome InputConnection. Serve android/tests on localhost:8765 and adb reverse it. */
class BrowserHostTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val device = UiDevice.getInstance(instrumentation)

    private fun <T> awaitValue(description: String, read: () -> T?): T {
        val deadline = SystemClock.uptimeMillis() + 15000
        do {
            instrumentation.uiAutomation.clearCache()
            read()?.let { return it }
            SystemClock.sleep(100)
        } while (SystemClock.uptimeMillis() < deadline)
        throw AssertionError(description)
    }

    private fun key(label: String) {
        awaitValue("IME key $label") {
            device.findObject(By.pkg("org.mywubi").desc(label).enabled(true))
        }.click()
        device.waitForIdle()
    }

    private fun field(): UiObject2 = awaitValue("Chrome form field") {
        device.findObject(By.clazz("android.webkit.WebView"))?.findObject(By.clazz("android.widget.EditText"))
    }

    private fun open(index: Int) {
        val run=SystemClock.uptimeMillis()
        device.executeShellCommand("ime enable org.mywubi/.MyWubiService")
        device.executeShellCommand("ime set org.mywubi/.MyWubiService")
        device.executeShellCommand("am start -n com.android.chrome/com.google.android.apps.chrome.Main")
        awaitValue("Chrome address bar") {device.findObject(By.res("com.android.chrome","url_bar"))}.click()
        awaitValue("Editable Chrome address") {device.findObject(By.res("com.android.chrome","url_bar"))}.text =
            "http://127.0.0.1:8765/host.html?field=$index&run=$run"
        device.pressEnter()
        awaitValue("Loaded browser fixture") {device.findObject(By.text("场景 $index · $run"))}
        field().click()
        device.waitForIdle()
    }

    private fun expect(value: String) {
        awaitValue("Expected host text '$value'") { field().text.takeIf { it == value } }
    }

    private fun counter(value: Int) {
        awaitValue("Submission count $value") {device.findObject(By.text("提交次数：$value"))}
    }

    @Test fun enterCommitsBeforeSendAndEmptyTemporaryDoesNotSubmit() {
        open(1)
        key("拼音"); key("空格"); key("发送"); counter(0)
        key("n"); key("i"); key("发送"); expect("ni"); counter(0)
        key("发送"); counter(1)
        open(2)
        key("g"); key("搜索"); expect("g"); counter(0)
        key("搜索"); counter(1)
        open(0)
        key("g"); key("换行"); expect("g")
        key("换行"); expect("g\n")
    }

    @Test fun unicodeSelectionAndCompositionCursorInChrome() {
        open(0)
        for (cluster in listOf("汉", "😀", "e\u0301", "👩‍💻", "🇨🇳", "👍🏽")) {
            field().text = "前$cluster"
            device.pressKeyCode(KeyEvent.KEYCODE_MOVE_END)
            key("退格"); expect("前")
        }
        field().text = "甲😀e\u0301乙"
        device.pressKeyCode(KeyEvent.KEYCODE_A, KeyEvent.META_CTRL_ON)
        key("退格"); expect("")
        key("g"); key("g"); expect("gg")
        device.pressDPadLeft()
        key("退格"); expect("g")
        key("a"); expect("ag")
        key("退格"); expect("g")
    }

    @Test fun restrictedFieldsAndNextActionInChrome() {
        open(3)
        for(c in "ggll") key(c.toString())
        assertTrue(device.hasObject(By.text("安全输入")))
        assertFalse(device.hasObject(By.descStartsWith("候选 ")))
        assertFalse(device.hasObject(By.text("ggll").pkg("org.mywubi")))
        open(4)
        key("1"); key("2")
        // Chrome 113 appends the accessible name to number-input values.
        awaitValue("Numeric host value 12") {field().text.takeIf {it == "12" || it == "12, 数字"}}
        assertFalse(device.hasObject(By.descStartsWith("候选 ")))
        open(5)
        key("+"); key("1"); key("2"); expect("+12")
        open(6)
        key("g");key("g");expect("gg")
        key(".");expect("gg.")
        open(7)
        key("a");key("@");expect("a@")
        open(8)
        key("下一项")
        awaitValue("Next field focused") {
            val fields=device.findObject(By.clazz("android.webkit.WebView"))?.findObjects(By.clazz("android.widget.EditText"))
            fields?.getOrNull(1)?.takeIf {it.isFocused}
        }
    }

    @Test fun hideRotateAndSwitchEditorsDoNotReuseOldCandidates() {
        open(0)
        key("g"); expect("g")
        device.pressBack()
        field().click()
        key("g"); expect("gg")
        try {
            device.setOrientationLeft()
            device.waitForIdle()
            SystemClock.sleep(1200)
            field().click()
            device.pressKeyCode(KeyEvent.KEYCODE_MOVE_END)
            device.waitForIdle()
            instrumentation.uiAutomation.clearCache()
            if (device.hasObject(By.text("Add to dictionary"))) device.pressBack()
            try { key("换行") } catch (failure: Throwable) {
                device.takeScreenshot(java.io.File(instrumentation.targetContext.filesDir, "rotation.png"))
                device.dumpWindowHierarchy(java.io.File(instrumentation.targetContext.filesDir, "rotation.xml"))
                throw failure
            }
            device.pressBack()
            // Rotation may replace the connection. Existing text must not duplicate.
            assertTrue(field().text == "gg" || field().text == "gg\n")
        } finally { device.setOrientationNatural(); device.unfreezeRotation() }
        open(8)
        key("g")
        device.pressBack()
        device.waitForIdle()
        val fields = awaitValue("Two form fields") {
            device.findObject(By.clazz("android.webkit.WebView"))?.findObjects(By.clazz("android.widget.EditText"))?.takeIf { it.size == 2 }
        }
        fields[1].click()
        awaitValue("Second editor focused") {
            device.findObject(By.clazz("android.webkit.WebView"))?.findObjects(By.clazz("android.widget.EditText"))?.getOrNull(1)?.takeIf { it.isFocused }
        }
        key("a")
        awaitValue("New field receives only its own input") {
            device.findObject(By.clazz("android.webkit.WebView"))?.findObjects(By.clazz("android.widget.EditText"))?.getOrNull(1)?.takeIf { it.text == "a" }
        }
        device.pressHome()
        device.executeShellCommand("am start -a android.settings.SETTINGS")
        open(0)
        key("g"); expect("g")
    }
}
