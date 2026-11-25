package org.mywubi

import android.os.SystemClock
import android.view.KeyEvent
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.BySelector
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.UiObject2
import org.junit.Assert.*
import org.junit.Test

private fun <T> awaitValue(description: String, read: () -> T?): T {
    val deadline = SystemClock.uptimeMillis() + 15000
    do {
        InstrumentationRegistry.getInstrumentation().uiAutomation.clearCache()
        read()?.let { return it }
        SystemClock.sleep(100)
    } while (SystemClock.uptimeMillis() < deadline)
    throw AssertionError(description)
}

internal fun focusBrowserAddressBar(device: UiDevice, browser: String): BySelector {
    device.executeShellCommand("am start -W -a android.intent.action.VIEW -d about:blank -p $browser")
    val address = By.res(browser, "url_bar")
    awaitValue("Browser address bar") {
        device.findObject(address) ?: device.findObject(By.res(browser, "web_title"))
            ?: device.findObject(By.res(browser, "search"))
            ?: device.findObject(By.pkg(browser).clazz("android.widget.EditText"))
            ?: device.findObject(By.pkg(browser).desc(java.util.regex.Pattern.compile("地址栏|网址栏|搜索或输入网址")))
    }.click()
    val editor = By.pkg(browser).clazz("android.widget.EditText").focused(true)
    awaitValue("Focused address bar") { device.findObject(editor) }.text = ""
    return editor
}

/** Real Chrome InputConnection. Serve android/tests on localhost:8765 and adb reverse it. */
class BrowserHostTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val device = UiDevice.getInstance(instrumentation)

    private val browser get() = InstrumentationRegistry.getArguments().getString("browserPackage","com.android.chrome")

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
        // Each fixture starts independently; an old browser task (including a
        // failed rotation) must not contaminate later editor contract tests.

        val launch = if (browser == "org.mywubi.acceptancehost") {
            device.executeShellCommand("am start -W -n $browser/.WebEditorActivity --ei field $index --el run $run")
        } else {
            device.executeShellCommand("am start -W -a android.intent.action.VIEW -d http://127.0.0.1:8765/host.html?case=$index-$run -p $browser")
        }
        assertFalse(launch, launch.contains("Error:"))
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

    @Test fun addressBarCommitsChineseAndEnglishWithoutLosingFocus() {
        device.executeShellCommand("ime enable org.mywubi/.MyWubiService")
        device.executeShellCommand("ime set org.mywubi/.MyWubiService")
        focusBrowserAddressBar(device, browser)
        val initialMode = awaitValue("Enabled language switch") {
            device.findObject(By.pkg("org.mywubi").desc(java.util.regex.Pattern.compile("中 → 英|英 → 中")).enabled(true))?.contentDescription
        }
        try {
            if (initialMode == "英 → 中") key("英 → 中")
            key("p")
            awaitValue("Chinese address-bar candidate") {
                device.findObject(By.pkg("org.mywubi").desc(java.util.regex.Pattern.compile("候选 这(?:，.*)?")))
            }.click()
            awaitValue("Committed Chinese") { device.findObject(By.pkg(browser).clazz("android.widget.EditText").focused(true).text("这")) }
            key("中 → 英"); key("p"); key(".")
            if (!device.hasObject(By.pkg("org.mywubi").desc("/"))) key("符号")
            key("/")
            awaitValue("Mixed address-bar text") { device.findObject(By.pkg(browser).clazz("android.widget.EditText").focused(true).text("这p./")) }
        } finally {
            if (device.hasObject(By.pkg("org.mywubi").desc("ABC"))) key("ABC")
            val restore = if (initialMode == "中 → 英") "英 → 中" else "中 → 英"
            if (device.hasObject(By.pkg("org.mywubi").desc(restore).enabled(true))) key(restore)
        }
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
        key("换行")
        // Chrome's accessible EditText value may trim a trailing newline; verify
        // the page's actual DOM value instead of weakening the newline assertion.
        awaitValue("DOM value contains exactly one newline") {
            device.findObject(By.text("正文值：\"g\\n\""))
        }
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
        // Chrome applies selection asynchronously. Establish the host selection
        // before exercising the IME's delete-selection contract.
        awaitValue("Chrome selected the complete UTF-16 range") {device.findObject(By.text("选区：0,6"))}
        device.waitForIdle()
        key("退格"); expect("")
        key("g"); key("g"); expect("gg")
        device.pressDPadLeft()
        key("退格"); expect("g")
        key("a"); expect("ag")
        key("退格"); expect("g")
    }

    @Test fun restrictedFieldsAndNextActionInChrome() {
        val failures = mutableListOf<String>()
        fun checkField(index: Int, action: () -> Unit) {
            try {
                open(index)
                action()
            } catch (e: AssertionError) {
                failures.add("field $index: ${e.message}")
            } catch (e: RuntimeException) {
                failures.add("field $index: ${e.message}")
            }
        }
        checkField(3) {
            for (c in "ggll") key(c.toString())
            assertTrue(device.hasObject(By.desc("安全输入")))
            assertFalse(device.hasObject(By.descStartsWith("候选 ")))
            assertFalse(device.hasObject(By.text("ggll").pkg("org.mywubi")))
        }
        checkField(4) {
            key("1"); key("2")
            awaitValue("Numeric host value 12") { field().text.takeIf { it == "12" || it == "12, 数字" } }
            assertFalse(device.hasObject(By.descStartsWith("候选 ")))
        }
        checkField(5) { key("+"); key("1"); key("2"); expect("+12") }
        checkField(6) { key("/"); key("."); expect("/.") }
        checkField(7) { key("a"); key("@"); expect("a@") }
        checkField(8) {
            key("下一项")
            awaitValue<UiObject2>("Next field focused") {
                val fields=device.findObject(By.clazz("android.webkit.WebView"))?.findObjects(By.clazz("android.widget.EditText"))
                fields?.getOrNull(1)?.takeIf { it.isFocused }
            }
        }
        assertTrue(failures.joinToString(), failures.isEmpty())
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
            awaitValue("Rotated document retained both characters") {
                device.findObject(By.text("正文值：\"gg\"")) ?: device.findObject(By.text("正文值：\"gg\\n\""))
            }
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
