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
    @androidx.test.filters.SdkSuppress(minSdkVersion = 30)
    @Test fun landscapeKeepsBottomEditorVisibleWhileTyping() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val device = UiDevice.getInstance(instrumentation)
        val previous = device.executeShellCommand("settings get secure default_input_method").trim()
        try {
            device.executeShellCommand("ime enable org.mywubi/.MyWubiService")
            device.executeShellCommand("ime set org.mywubi/.MyWubiService")
            ActivityScenario.launch(MainActivity::class.java).use { scenario ->
                scenario.onActivity { it.requestedOrientation = android.content.pm.ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE }
                device.waitForIdle()
                lateinit var editor: EditText
                scenario.onActivity { activity ->
                    assertEquals(android.content.res.Configuration.ORIENTATION_LANDSCAPE, activity.resources.configuration.orientation)
                    activity.window.setSoftInputMode(android.view.WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE)
                    val host = android.widget.FrameLayout(activity)
                    editor = EditText(activity).apply { setSingleLine(); setText("方案"); setSelection(length()) }
                    host.addView(editor, android.widget.FrameLayout.LayoutParams(-1, -2, android.view.Gravity.BOTTOM))
                    activity.setContentView(host)
                    editor.requestFocus()
                    editor.post { activity.getSystemService(InputMethodManager::class.java).showSoftInput(editor, 0) }
                }
                fun key(label: String) = device.wait(Until.findObject(By.desc(label).pkg("org.mywubi").enabled(true)), 10000)
                    ?: error("Missing key: $label")
                fun assertEditorVisible() {
                    device.waitForIdle()
                    scenario.onActivity { activity ->
                        val bounds = activity.windowManager.currentWindowMetrics.bounds
                        val insets = editor.rootWindowInsets
                        assertTrue(insets.isVisible(android.view.WindowInsets.Type.ime()))
                        val imeHeight = insets.getInsets(android.view.WindowInsets.Type.ime()).bottom
                        val position = IntArray(2).also(editor::getLocationOnScreen)
                        val visible = android.graphics.Rect()
                        assertTrue(editor.hasFocus() && editor.getLocalVisibleRect(visible))
                        assertEquals(editor.height, visible.height())
                        assertTrue(position[1] + editor.height <= bounds.bottom - imeHeight)
                        assertTrue("IME must leave a quarter of the landscape window for the host",
                            imeHeight <= bounds.height() * 0.75f + 1)
                    }
                }
                key("g")
                device.findObject(By.desc("英 → 中").pkg("org.mywubi"))?.click()
                assertEditorVisible()
                key("g").click()
                device.waitForIdle()
                instrumentation.uiAutomation.clearCache()
                device.takeScreenshot(java.io.File(instrumentation.targetContext.filesDir, "landscape-editor-composing.png"))
                val candidate = device.wait(Until.findObject(By.descStartsWith("候选 ").pkg("org.mywubi").clickable(true)), 5000)
                    ?: error("No candidate for g")
                val expected = "方案" + candidate.contentDescription.removePrefix("候选 ").substringBefore("，")
                assertEditorVisible()
                candidate.click()
                assertTrue(device.wait(Until.hasObject(By.text(expected).clazz(EditText::class.java)), 5000))
                assertEditorVisible()
                scenario.onActivity { assertEquals(expected, editor.text.toString()); assertEquals(expected.length, editor.selectionStart) }
            }
        } finally {
            if (previous.isNotEmpty() && previous != "null") device.executeShellCommand("ime set $previous")
        }
    }

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
