package org.mywubi

import androidx.test.uiautomator.*
import androidx.test.platform.app.InstrumentationRegistry

/** Compose can expose a description as a child of the actionable semantics node. */
internal fun UiObject2.actionNode(): UiObject2 {
    var current: UiObject2? = this
    while (current != null) {
        if (current.isClickable) return current
        current = current.parent
    }
    return this
}

/** Categories can be off screen on a phone or at accessibility font sizes. */
internal fun selectSettingsTab(device: UiDevice, label: String) {
    val ins = InstrumentationRegistry.getInstrumentation()
    for (forward in listOf(false, true)) repeat(8) {
        device.waitForIdle(); ins.waitForIdleSync(); ins.uiAutomation.clearCache()
        val tabs = device.wait(Until.findObject(By.desc("设置分类")), 10000) ?: run {
            device.dumpWindowHierarchy(java.io.File(ins.targetContext.filesDir,"settings-tabs-failure.xml"))
            error("Missing settings tabs")
        }
        val bounds = tabs.visibleBounds
        tabs.findObjects(By.text(label)).firstOrNull { bounds.contains(it.visibleBounds) && !it.visibleBounds.isEmpty }?.let {
            it.click(); device.waitForIdle(); return
        }
        // Stay outside Android's edge-back gesture area.
        val inset=bounds.width()/5
        device.swipe(if(forward) bounds.right-inset else bounds.left+inset,bounds.centerY(),
            if(forward) bounds.left+inset else bounds.right-inset,bounds.centerY(),20)
        device.waitForIdle()
    }
    error("Missing settings tab $label")
}
