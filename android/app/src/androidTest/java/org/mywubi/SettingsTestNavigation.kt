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
        val labelBounds = tabs.findObject(By.text(label))?.visibleBounds
        if (labelBounds != null && labelBounds.width() >= 24 && labelBounds.height() >= 24) {
            repeat(3) {
                ins.uiAutomation.clearCache()
                val option = device.findObject(By.desc("设置分类"))?.findObject(By.text(label))
                if (option != null) {
                    val button = option.actionNode()
                    if (button.isSelected) return
                    val centre = option.visibleCenter
                    device.click(centre.x,centre.y)
                    val end = android.os.SystemClock.elapsedRealtime() + 1500
                    while (android.os.SystemClock.elapsedRealtime() < end) {
                        ins.uiAutomation.clearCache()
                        val selected = device.findObject(By.desc("设置分类"))?.findObject(By.text(label))?.actionNode()
                        if (selected?.isSelected == true ||
                            (label == "特殊输入" && (device.hasObject(By.desc("特殊输入项目列表")) ||
                                device.hasObject(By.desc("特殊输入项目详情")))) ||
                            (label == "常用" && device.hasObject(By.text("输入习惯"))) ||
                            (label == "预测" && device.hasObject(By.text("字词预测")))) return
                        android.os.SystemClock.sleep(60)
                    }
                }
            }
        }
        // Stay outside Android's edge-back gesture area.
        val inset=bounds.width()/5
        device.swipe(if(forward) bounds.right-inset else bounds.left+inset,bounds.centerY(),
            if(forward) bounds.left+inset else bounds.right-inset,bounds.centerY(),20)
        device.waitForIdle()
    }
    error("Missing settings tab $label")
}

/** Open a rememberSaveable disclosure, including after the activity has been reopened. */
internal fun expandSettingsControl(device: UiDevice, title: String, modelRoot: Boolean = false) {
    val names = if (modelRoot) java.util.regex.Pattern.compile("(?:收起)?" + java.util.regex.Pattern.quote(title))
        else java.util.regex.Pattern.compile(java.util.regex.Pattern.quote(title))
    val label = findSettingsControl(device, By.text(names))
    val button = label.actionNode()
    if (label.text.startsWith("收起") || button.hasObject(By.text("收起"))) return
    clickSetting(device, title)
}

/** Activate the accessibility action even if a sticky bar clips the control's centre. */
internal fun clickSetting(device: UiDevice, label: String, description: Boolean = false) {
    findSettingsControl(device, if (description) By.desc(label) else By.text(label))
    val automation = InstrumentationRegistry.getInstrumentation().uiAutomation
    val deadline = android.os.SystemClock.uptimeMillis() + 10000
    do {
        automation.clearCache()
        val nodes = java.util.ArrayDeque<android.view.accessibility.AccessibilityNodeInfo>()
        automation.windows.mapNotNull { it.root }.forEach(nodes::add)
        while (nodes.isNotEmpty()) {
            val node = nodes.removeFirst()
            val value = if (description) node.contentDescription else node.text
            if (node.packageName == "org.mywubi" && value?.toString() == label) {
                var target: android.view.accessibility.AccessibilityNodeInfo? = node
                while (target != null && !target.isClickable) target = target.parent
                if (target != null && target.isEnabled &&
                    target.performAction(android.view.accessibility.AccessibilityNodeInfo.ACTION_CLICK)) {
                    device.waitForIdle()
                    return
                }
            }
            for (i in 0 until node.childCount) node.getChild(i)?.let(nodes::add)
        }
        android.os.SystemClock.sleep(100)
    } while (android.os.SystemClock.uptimeMillis() < deadline)
    error("Settings action unavailable: $label")
}

/** Scroll the settings list itself. Screen swipes can hit the tab strip or a nested editor. */
internal fun findSettingsControl(device: UiDevice, selector: BySelector, timeoutMs: Long = 30000): UiObject2 {
    val automation = InstrumentationRegistry.getInstrumentation().uiAutomation
    val deadline = android.os.SystemClock.uptimeMillis() + timeoutMs
    do {
        for (forward in listOf(false, true)) {
            do {
                automation.clearCache()
                device.findObject(selector)?.let { return it }
                val root = automation.rootInActiveWindow ?: break
                val nodes = java.util.ArrayDeque(listOf(root))
                var scrolled = false
                while (nodes.isNotEmpty()) {
                    val node = nodes.removeFirst()
                    if (node.packageName == "org.mywubi" && node.contentDescription == "设置内容" && node.isScrollable) {
                        scrolled = node.performAction(if (forward) android.view.accessibility.AccessibilityNodeInfo.ACTION_SCROLL_FORWARD
                            else android.view.accessibility.AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD)
                        break
                    }
                    for (i in 0 until node.childCount) node.getChild(i)?.let(nodes::add)
                }
                if (!scrolled) break
                android.os.SystemClock.sleep(200)
            } while (android.os.SystemClock.uptimeMillis() < deadline)
        }
        android.os.SystemClock.sleep(150)
    } while (android.os.SystemClock.uptimeMillis() < deadline)
    error("Missing settings control $selector")
}
