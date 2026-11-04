package org.mywubi

import android.view.inputmethod.InputMethodManager
import android.widget.EditText
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
import org.junit.Assert.*
import org.junit.Test

class HideKeyboardHostTest {
    @Test fun toolbarHidesRealImeAndEditorCanOpenItAgain() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val device = UiDevice.getInstance(instrumentation)
        val previous = device.executeShellCommand("settings get secure default_input_method").trim()
        try {
            device.executeShellCommand("ime enable org.mywubi/.MyWubiService")
            device.executeShellCommand("ime set org.mywubi/.MyWubiService")
            ActivityScenario.launch(MainActivity::class.java).use { scenario ->
                lateinit var editor: EditText
                scenario.onActivity { activity ->
                    editor = EditText(activity).apply { setText("保留正文"); setSingleLine() }
                    activity.setContentView(editor)
                }
                fun showKeyboard() {
                    scenario.onActivity { activity ->
                        editor.requestFocus()
                        activity.getSystemService(InputMethodManager::class.java)
                            .showSoftInput(editor, InputMethodManager.SHOW_IMPLICIT)
                    }
                }
                val hide = By.desc("收起键盘").pkg("org.mywubi")
                showKeyboard()
                val button = device.wait(Until.findObject(hide), 10000) ?: error("IME did not open")
                button.click()
                assertTrue("IME should be hidden", device.wait(Until.gone(hide), 5000))
                scenario.onActivity { assertEquals("保留正文", editor.text.toString()) }
                showKeyboard()
                assertTrue("Editor should reopen the keyboard", device.wait(Until.hasObject(hide), 5000))
            }
        } finally {
            if (previous.isNotEmpty() && previous != "null") device.executeShellCommand("ime set $previous")
        }
    }
}
