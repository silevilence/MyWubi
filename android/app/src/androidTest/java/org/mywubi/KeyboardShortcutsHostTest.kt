package org.mywubi

import android.content.ClipData
import android.content.ClipboardManager
import android.os.SystemClock
import android.text.InputType
import android.view.InputDevice
import android.view.MotionEvent
import android.view.inputmethod.InputMethodManager
import android.widget.EditText
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.junit.Assert.*
import org.junit.Test

class KeyboardShortcutsHostTest {
    private val ins = InstrumentationRegistry.getInstrumentation()
    private val device = UiDevice.getInstance(ins)

    private fun gone(selector: BySelector): Boolean {
        val deadline = SystemClock.uptimeMillis() + 5000
        do {
            ins.uiAutomation.clearCache()
            if (!device.hasObject(selector)) return true
            SystemClock.sleep(100)
        } while (SystemClock.uptimeMillis() < deadline)
        return false
    }

    @Test fun realEditorPastesDismissesAndClearsWholeFieldFromMiddle() {
        val previous = device.executeShellCommand("settings get secure default_input_method").trim()
        val preferences = ins.targetContext.getSharedPreferences("android-ui", 0)
        val original = preferences.getBoolean("clipboard_suggestion", true)
        preferences.edit().putBoolean("clipboard_suggestion", true).commit()
        try {
            device.executeShellCommand("ime enable org.mywubi/.MyWubiService")
            device.executeShellCommand("ime set org.mywubi/.MyWubiService")
            ActivityScenario.launch(MainActivity::class.java).use { scenario ->
                lateinit var editor: EditText
                val text = "beancount 今日确认\n😀 abc"
                val paste = By.desc("粘贴剪贴板内容")
                val hide = By.desc("收起键盘")
                scenario.onActivity { activity ->
                    activity.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("", text))
                    editor = EditText(activity).apply { inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_MULTI_LINE }
                    activity.setContentView(editor)
                }
                fun show() {
                    scenario.onActivity { activity ->
                        editor.requestFocus()
                        activity.getSystemService(InputMethodManager::class.java).showSoftInput(editor, InputMethodManager.SHOW_IMPLICIT)
                    }
                    assertTrue(device.wait(Until.hasObject(hide), 10000))
                    device.waitForIdle()
                    SystemClock.sleep(300) // Wait for the IME window animation before hit-testing keys.
                    ins.uiAutomation.clearCache()
                }
                fun hide() { device.findObject(hide).click(); assertTrue(gone(hide)) }
                show()
                device.takeScreenshot(java.io.File(ins.targetContext.filesDir, "keyboard-clipboard.png"))
                device.wait(Until.findObject(paste.enabled(true)), 10000).click()
                device.waitForIdle()
                scenario.onActivity { assertEquals(text, editor.text.toString()) }
                assertTrue(gone(paste))

                hide(); show()
                assertTrue(device.wait(Until.hasObject(paste), 5000))
                device.wait(Until.findObject(By.desc("q").enabled(true)), 10000).click()
                if (!gone(paste)) {
                    device.takeScreenshot(java.io.File(ins.targetContext.filesDir, "clipboard-dismiss-failure.png"))
                    device.dumpWindowHierarchy(java.io.File(ins.targetContext.filesDir, "clipboard-dismiss-failure.xml"))
                    fail("输入后应隐藏剪贴板建议")
                }
                device.findObject(By.desc("退格")).click()
                device.waitForIdle()
                assertTrue("退格清码后不重新弹出建议", gone(paste))

                hide(); show()
                device.wait(Until.findObject(By.desc("关闭剪贴板建议")), 5000).click()
                assertTrue(gone(paste))

                // The caret is in the middle: clearing must remove both sides, including Unicode.
                val body = "前文👩‍💻\n后文".repeat(80)
                scenario.onActivity { editor.setText(body); editor.setSelection(body.length / 2) }
                device.waitForIdle()
                val key = device.wait(Until.findObject(By.desc("退格")), 5000).visibleCenter
                val rise = (70 * ins.targetContext.resources.displayMetrics.density).toInt()
                val start = SystemClock.uptimeMillis()
                fun touch(action: Int, y: Int) {
                    val event = MotionEvent.obtain(start, SystemClock.uptimeMillis(), action, key.x.toFloat(), y.toFloat(), 0)
                    event.source = InputDevice.SOURCE_TOUCHSCREEN
                    try { assertTrue(ins.uiAutomation.injectInputEvent(event, true)) } finally { event.recycle() }
                }
                touch(MotionEvent.ACTION_DOWN, key.y)
                SystemClock.sleep(500)
                touch(MotionEvent.ACTION_MOVE, key.y - rise)
                SystemClock.sleep(100)
                device.takeScreenshot(java.io.File(ins.targetContext.filesDir, "keyboard-clear-gesture.png"))
                touch(MotionEvent.ACTION_UP, key.y - rise)
                device.waitForIdle()
                scenario.onActivity { assertEquals("", editor.text.toString()) }
                device.findObject(By.desc("a")).click()
                device.waitForIdle()
                scenario.onActivity { assertTrue("清空后还能继续输入", editor.text.isNotEmpty()) }

                hide()
                scenario.onActivity { editor.inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD }
                show()
                assertTrue("密码框没有剪贴板预览", gone(paste))
            }
        } finally {
            preferences.edit().putBoolean("clipboard_suggestion", original).commit()
            if (previous.isNotEmpty() && previous != "null") device.executeShellCommand("ime set $previous")
        }
    }
}
