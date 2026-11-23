package org.mywubi

import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.junit.Assert.*
import org.junit.Test
import java.io.File

class UpdateIdleHostTest {
    @Test fun seedCachedUpdate() {
        org.junit.Assume.assumeTrue(InstrumentationRegistry.getArguments().getString("seed_cache") == "true")
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val update = AppUpdate("0.5.1",2_000_000_000,26,"test","${RELEASE_ROOT}v0.5.1/MyWubi-0.5.1-Android.apk","a".repeat(64),listOf(android.os.Build.SUPPORTED_ABIS.first()))
        context.getSharedPreferences("app-updates",0).edit().putString("available",update.json().toString())
            .putLong("next",System.currentTimeMillis()+86400000).commit()
    }

    @Test fun realImeDefersCachedUpdateUntilIdleAcrossRotationAndRecreation() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val device = UiDevice.getInstance(instrumentation)
        val preferences = context.getSharedPreferences("app-updates",0)
        org.junit.Assume.assumeTrue("Seed cache in a separate process before this test",
            InstrumentationRegistry.getArguments().getString("test_idle") == "true" && preferences.contains("available"))
        fun awaitNode(selector: BySelector): UiObject2 {
            val end = android.os.SystemClock.uptimeMillis()+10000
            do {
                instrumentation.uiAutomation.clearCache()
                device.findObject(selector)?.let { return it }
                Thread.sleep(100)
            } while(android.os.SystemClock.uptimeMillis()<end)
            error("Missing $selector")
        }
        fun key(label:String) = awaitNode(By.desc(label).pkg("org.mywubi"))
        val badge = By.desc("设置，有可用更新").pkg("org.mywubi")
        fun openEditor() {
            if (InstrumentationRegistry.getArguments().getString("prediction_host")=="editor") {
                val host="org.mywubi.acceptancehost"
                device.executeShellCommand("am force-stop $host")
                device.executeShellCommand("am start -W -n $host/.EditorActivity")
                focusExternalEditor(device)
                return
            }
            device.executeShellCommand("am force-stop com.android.settings")
            device.executeShellCommand("am force-stop com.google.android.settings.intelligence")
            device.executeShellCommand("am start -a android.settings.SETTINGS")
            device.wait(Until.findObject(By.res("com.android.settings","search_action_bar")),15000).click()
            device.wait(Until.findObject(By.clazz("android.widget.EditText")),15000).apply {text="";click()}
        }
        try {
            device.executeShellCommand("ime enable org.mywubi/.MyWubiService")
            device.executeShellCommand("ime set org.mywubi/.MyWubiService")
            openEditor()
            awaitNode(badge)
            device.takeScreenshot(File(context.filesDir,"au-idle.png"))
            val updates = EngineUpdates.preferences(context)
            val deadline = android.os.SystemClock.elapsedRealtime() + 15000
            while (updates.getString("requested",null) != updates.getString("applied",null) &&
                android.os.SystemClock.elapsedRealtime() < deadline) Thread.sleep(80)
            assertEquals("Runtime reload",updates.getString("requested",null),updates.getString("applied",null))
            instrumentation.uiAutomation.clearCache()
            device.findObject(By.pkg("org.mywubi").desc("英 → 中"))?.click()
            awaitNode(By.pkg("org.mywubi").desc("中 → 英").enabled(true))
            val p = key("p").visibleBounds
            device.click(p.left + p.width()/8,p.centerY())
            awaitNode(By.pkg("org.mywubi").descStartsWith("候选 这"))
            assertFalse(device.hasObject(badge))
            key("展开").click()
            assertFalse(device.hasObject(badge))
            device.takeScreenshot(File(context.filesDir,"au-typing.png"))
            instrumentation.uiAutomation.clearCache()
            device.findObject(By.descStartsWith("候选 这").pkg("org.mywubi")).click()
            awaitNode(By.clazz("android.widget.EditText").text("这"))
            awaitNode(badge)
            device.setOrientationLeft()
            if (InstrumentationRegistry.getArguments().getString("prediction_host") == "editor") {
                awaitNode(By.pkg("org.mywubi.acceptancehost").clazz("android.widget.EditText").text("这"))
                if (!device.hasObject(By.pkg("org.mywubi").desc("退格"))) focusExternalEditor(device)
            }
            awaitNode(badge)
            device.takeScreenshot(File(context.filesDir,"au-landscape.png"))
            device.setOrientationNatural()
            val alternate=InstrumentationRegistry.getArguments().getString("alternate_ime","com.google.android.inputmethod.latin/com.android.inputmethod.latin.LatinIME")
            device.executeShellCommand("ime set $alternate")
            device.executeShellCommand("ime set org.mywubi/.MyWubiService")
            openEditor()
            awaitNode(badge)
            key("设置，有可用更新").click()
            awaitNode(By.pkg("org.mywubi").desc("设置分类"))
            selectSettingsTab(device,"常用")
            assertTrue(findSettingsControl(device,By.text("应用更新")).actionNode().isEnabled)
        } finally {
            device.takeScreenshot(File(context.filesDir,"au-idle-end.png"))
            device.dumpWindowHierarchy(File(context.filesDir,"au-idle-end.xml"))
            device.setOrientationNatural();device.unfreezeRotation()
            preferences.edit().clear().commit()
            device.pressBack();device.pressBack()
        }
    }
}
