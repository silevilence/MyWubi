package org.mywubi

import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Density
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.util.concurrent.CopyOnWriteArrayList

class ExpandedCandidatesTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val device = UiDevice.getInstance(instrumentation)
    private fun settled() { device.waitForIdle(); instrumentation.waitForIdleSync(); instrumentation.uiAutomation.clearCache() }
    private fun node(description: String): UiObject2 =
        device.wait(Until.findObject(By.desc(description).pkg("org.mywubi")), 5000) ?: error("Missing $description")

    @Test fun expandedCandidatesAppearOnceAndReturnToTyping() = checkExpanded(1f)
    @Test fun expandedCandidatesRemainUsableAtLargeFont() = checkExpanded(2f)

    private fun checkExpanded(fontScale: Float) {
        val events = CopyOnWriteArrayList<Any>()
        val candidates = listOf(CandidateItem(1, "衩皮儿", ""), CandidateItem(2, "括", "td"),
            CandidateItem(3, "给", ""), CandidateItem(4, "销", ""), CandidateItem(5, "在", ""))
        var model by mutableStateOf(KeyboardModel(enabled=true, modesAvailable=true, pinyinAvailable=true,
            spelling="r", candidates=candidates, total=5))
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            if (InstrumentationRegistry.getArguments().getString("layout_orientation") == "landscape") {
                scenario.onActivity { it.requestedOrientation = android.content.pm.ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE }
                val deadline = android.os.SystemClock.uptimeMillis() + 10000
                while (device.displayWidth <= device.displayHeight && android.os.SystemClock.uptimeMillis() < deadline) {
                    android.os.SystemClock.sleep(100)
                }
                assertTrue(device.displayWidth > device.displayHeight)
            }
            scenario.onActivity { activity -> activity.setContent {
                val density = LocalDensity.current
                CompositionLocalProvider(LocalDensity provides Density(density.density, fontScale)) {
                    Box(Modifier.fillMaxSize(), contentAlignment=Alignment.BottomCenter) {
                        Keyboard(model, { events.add(it) }, {}, switchIme={})
                    }
                }
            } }
            node("展开").click(); settled()
            assertEquals("The compact strip must disappear when the grid opens", 1,
                device.findObjects(By.desc("候选 衩皮儿")).size)
            assertFalse(device.hasObject(By.desc("q")))
            assertFalse("Single-window candidates do not need disabled paging buttons", device.hasObject(By.text("下一组")))
            assertNotNull(node("收起"))
            assertNotNull(node("收起键盘"))
            assertFalse("Expanded candidates must hide toolbar actions", device.hasObject(By.desc("拼音")))
            val minimum = (48 * instrumentation.targetContext.resources.displayMetrics.density).toInt()
            assertTrue("Expanded grid must leave the bottom keys fully visible",
                node("退格").visibleBounds.height() >= minimum)
            device.takeScreenshot(File(instrumentation.targetContext.filesDir, "keyboard-expanded-$fontScale.png"))
            node("候选 括，编码提示 td").click(); settled()
            assertEquals(2L, (events.last() as JSONObject).getJSONArray("SelectCandidate").getLong(1))
            assertNotNull(node("q"))
            node("展开").click(); settled()
            node("收起").click(); settled()
            assertNotNull(node("q"))
            // Prediction candidates have no spelling, but still expand and select normally.
            scenario.onActivity { model = model.copy(spelling="", prediction=true) }
            settled()
            node("展开").click(); settled()
            assertEquals(1, device.findObjects(By.desc("候选 衩皮儿")).size)
            node("候选 衩皮儿").click(); settled()
            assertNotNull(node("q"))
        }
    }

    @Test fun expandedGridScrollsAndPagesWithCurrentCandidateIdentity() {
        val offsets = CopyOnWriteArrayList<Int>()
        val events = CopyOnWriteArrayList<Any>()
        val candidates = (1L..256L).map { CandidateItem(it, "候选词条$it", "ab") }
        var model by mutableStateOf(KeyboardModel(enabled=true, spelling="a", candidates=candidates,
            query="{\"revision\":1}", total=300))
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { it.setContent { Keyboard(model, { events.add(it) }, { offsets.add(it) }, switchIme={}) } }
            node("展开").click(); settled()
            val grid = node("全部候选")
            grid.setGestureMargin(20)
            grid.scroll(Direction.DOWN, 1f); settled()
            assertFalse(device.hasObject(By.desc("候选 候选词条1，编码提示 ab")))
            node("下一组").click(); settled()
            assertEquals(256, offsets.last())
            scenario.onActivity { model = model.copy(offset=256, query="{\"revision\":2}",
                candidates=listOf(CandidateItem(257, "新窗口", "cd"))) }
            settled()
            node("候选 新窗口，编码提示 cd").click(); settled()
            val selection = (events.last() as JSONObject).getJSONArray("SelectCandidate")
            assertEquals(257, selection.getLong(1))
            assertEquals(2, selection.getJSONObject(0).getInt("revision"))
        }
    }
}
