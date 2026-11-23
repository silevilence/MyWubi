package org.mywubi

import android.os.SystemClock
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until

/** The IME and the editor update different accessibility windows. Requery both roots. */
internal fun findFresh(device: UiDevice, selector: androidx.test.uiautomator.BySelector, timeout: Long): androidx.test.uiautomator.UiObject2? {
    val automation = InstrumentationRegistry.getInstrumentation().uiAutomation
    val deadline = SystemClock.elapsedRealtime() + timeout
    do {
        automation.clearCache()
        device.findObject(selector)?.let { return it }
        SystemClock.sleep(60)
    } while (SystemClock.elapsedRealtime() < deadline)
    return null
}

internal fun awaitHostText(device: UiDevice, selector: androidx.test.uiautomator.BySelector, expected: String, timeout: Long): Boolean {
    val deadline = SystemClock.elapsedRealtime() + timeout
    do {
        InstrumentationRegistry.getInstrumentation().uiAutomation.clearCache()
        val field = device.findObject(selector)
        if (field != null && field.text.orEmpty() == expected) return true
        SystemClock.sleep(50)
    } while (SystemClock.elapsedRealtime() < deadline)
    return false
}

/** Text fields in the acceptance host fill the window. Their old centre can be
 * covered by the IME between finding the field and tapping it during resize. */
internal fun focusExternalEditor(device: UiDevice) {
    val instrumentation = InstrumentationRegistry.getInstrumentation()
    val selector = By.pkg("org.mywubi.acceptancehost").clazz("android.widget.EditText")
    repeat(3) {
        instrumentation.uiAutomation.clearCache()
        val bounds = findFresh(device, selector, 5000)?.visibleBounds ?: return@repeat
        device.click(bounds.left + bounds.width()/3, bounds.top + minOf(bounds.height()/2, 24))
        val until = SystemClock.elapsedRealtime() + 4000
        while (SystemClock.elapsedRealtime() < until) {
            instrumentation.uiAutomation.clearCache()
            if (device.hasObject(By.copy(selector).focused(true)) &&
                device.hasObject(By.pkg("org.mywubi").desc("退格").enabled(true))) return
            SystemClock.sleep(80)
        }
    }
    val state = device.executeShellCommand("dumpsys input_method").lineSequence()
        .filter { it.contains("mCurId=") || it.contains("mInputShown=") }.joinToString()
    error("Focused editor did not acquire the MyWubi keyboard: $state")
}
