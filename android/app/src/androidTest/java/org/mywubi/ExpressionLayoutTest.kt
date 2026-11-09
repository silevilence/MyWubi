package org.mywubi

import android.content.pm.ActivityInfo
import android.os.SystemClock
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

/** Compose visual/interaction evidence, independent of the real-host test. */
class ExpressionLayoutTest {
    private val instrumentation=InstrumentationRegistry.getInstrumentation()
    private val device=UiDevice.getInstance(instrumentation)
    private val evidence=File(instrumentation.targetContext.getExternalFilesDir(null),"expression-layout").apply { mkdirs() }
    private fun settled() {
        device.waitForIdle();instrumentation.waitForIdleSync()
        val automation=instrumentation.uiAutomation
        if(android.os.Build.VERSION.SDK_INT>=34) automation.clearCache() else automation.serviceInfo=automation.serviceInfo
    }
    private fun node(selector:BySelector):UiObject2 {
        settled()
        return device.wait(Until.findObject(By.copy(selector).pkg("org.mywubi")),5000) ?: error("Missing $selector")
    }
    private fun shot(name:String) {
        settled();device.takeScreenshot(File(evidence,"$name.png"));device.dumpWindowHierarchy(File(evidence,"$name.xml"))
    }
    private fun reveal(selector:BySelector,pane:BySelector):UiObject2 {
        repeat(16) {
            settled()
            device.findObject(By.copy(selector).pkg("org.mywubi"))?.let { if(it.visibleBounds.height()>32) return it }
            val scroll=node(pane)
            scroll.setGestureMargin(20);scroll.scroll(Direction.DOWN,0.65f)
        }
        return node(selector)
    }
    @Test fun portraitNormalFont()=checkLayout(false,1f)
    @Test fun portraitLargeFont()=checkLayout(false,2f)
    @Test fun landscapeNormalFont()=checkLayout(true,1f)
    @Test fun landscapeLargeFont()=checkLayout(true,2f)

    private fun checkLayout(landscape:Boolean,fontScale:Float) {
        val events=CopyOnWriteArrayList<Any>()
        val name="${if(landscape) "landscape" else "portrait"}-$fontScale"
        val samples=listOf(
            CandidateItem(1,"🚳","禁止自行车 · 名称匹配","emoji"),
            CandidateItem(2,"👨‍👩‍👧‍👦","家庭","emoji"),
            CandidateItem(3,"👩🏽‍⚕️","肤色与性别","emoji"),
            CandidateItem(4,"🇨🇳","旗帜","emoji"),
            CandidateItem(5,"1️⃣","键帽","emoji"),
            CandidateItem(6,"♥︎","文本变体","emoji"),
            CandidateItem(7,"♥️","表情变体","emoji"),
            CandidateItem(8,String(Character.toChars(0x1FAEA)),"新版本字符","emoji"),
            CandidateItem(9,"(๑•̀ㅂ•́)و✧　".repeat(14)+"(╯°□°）╯︵ ┻━┻","开心长颜文字","kaomoji"),
            CandidateItem(10,"Ω∑─╳\u2009(ಠ_ಠ)\u3000","特殊符号与空格","kaomoji")
        )
        var model by mutableStateOf(KeyboardModel(enabled=true,modesAvailable=true,query="{\"revision\":91}",
            expression=ExpressionQueryUi("emoji","Results","禁止车",null),candidates=samples,total=samples.size))
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { it.requestedOrientation=if(landscape) ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE else ActivityInfo.SCREEN_ORIENTATION_PORTRAIT }
            val deadline=SystemClock.uptimeMillis()+10000
            while((device.displayWidth>device.displayHeight)!=landscape && SystemClock.uptimeMillis()<deadline) SystemClock.sleep(100)
            assertEquals(landscape,device.displayWidth>device.displayHeight)
            scenario.onActivity { activity -> activity.setContent {
                val density=LocalDensity.current
                CompositionLocalProvider(LocalDensity provides Density(density.density,fontScale)) {
                    Box(Modifier.fillMaxSize(),contentAlignment=Alignment.BottomCenter) {
                        Keyboard(model,{events.add(it)},{},switchIme={})
                    }
                }
            } }
            shot("$name-compact")
            val cancel=reveal(By.text("取消"),By.desc("表情查询与操作")).visibleBounds
            val letter=node(By.desc("q")).visibleBounds
            val enter=node(By.desc("回车")).visibleBounds
            assertTrue("Query controls must be visible",cancel.height()>0 && cancel.top>=0)
            val first=reveal(By.descStartsWith("候选 🚳"),By.desc("表情查询与操作")).visibleBounds
            assertTrue("Candidate strip must not overlap keys",if(landscape) first.right<=letter.left else first.bottom<=letter.top)
            val minTouch=(48*instrumentation.targetContext.resources.displayMetrics.density).toInt()
            assertTrue("Bottom keys must remain fully visible",enter.height()>=minTouch)
            reveal(By.desc("展开"),By.desc("表情查询与操作")).click();settled();shot("$name-expanded")
            assertTrue(node(By.desc("退格")).visibleBounds.height()>=minTouch)
            val target=reveal(By.descStartsWith("候选 ${samples[8].text}"),By.desc("全部候选"))
            shot("$name-long")
            target.click();settled()
            val selected=(events.last() as JSONObject).getJSONArray("SelectCandidate")
            assertEquals(9L,selected.getLong(1));assertEquals(91,selected.getJSONObject(0).getInt("revision"))
            scenario.onActivity { model=model.copy(expression=ExpressionQueryUi("emoji","Editing","禁止".repeat(100),null),
                spelling="a",candidates=listOf(CandidateItem(20,"车","")),total=1) }
            shot("$name-editing")
            if(landscape) { val pane=node(By.desc("表情查询与操作"));pane.setGestureMargin(20);pane.scroll(Direction.UP,1f) }
            reveal(By.text("查找表情"),By.desc("表情查询与操作")).click();settled();assertEquals("FindExpressions",events.last())
            reveal(By.text("取消"),By.desc("表情查询与操作")).click();settled();assertEquals("EndTemporary",events.last())
            scenario.onActivity { activity -> activity.setContent { ExpressionPreview() } }
            node(By.text("表情显示预览")).click();settled()
            node(By.text("家庭")).click();shot("$name-preview-family")
            assertNotNull(node(By.descStartsWith("表情预览：家庭")))
        }
    }
}
