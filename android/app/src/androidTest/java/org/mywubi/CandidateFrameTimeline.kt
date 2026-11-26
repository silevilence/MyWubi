package org.mywubi

import android.inputmethodservice.InputMethodService
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import android.view.ViewTreeObserver
import android.view.Window
import android.view.WindowManager
import android.view.inspector.WindowInspector
import androidx.compose.ui.node.RootForTest
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.UiDevice
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/** Correlate the first draw containing the candidate with SurfaceFlinger's FrameTimeline token. */
@OptIn(androidx.compose.ui.InternalComposeUiApi::class)
internal class CandidateFrameTimeline(private val device: UiDevice) {
    data class Draw(val down: Long, val up: Long, val observedNs: Long)
    private val ins = InstrumentationRegistry.getInstrumentation()
    private lateinit var root: View
    private lateinit var window: Window
    private lateinit var compose: RootForTest
    private val layer = Regex("""(?:^|\{)(InputMethod#\d+)(?: |$)""", RegexOption.MULTILINE)
        .findAll(device.executeShellCommand("dumpsys SurfaceFlinger --list")).map { it.groupValues[1] }.distinct().single()

    init {
        ins.runOnMainSync {
            root = WindowInspector.getGlobalWindowViews().single {
                (it.layoutParams as? WindowManager.LayoutParams)?.type == WindowManager.LayoutParams.TYPE_INPUT_METHOD
            }
            val views = descendants(root).toList()
            window = requireNotNull(views.mapNotNull { it.context as? InputMethodService }.first().window.window)
            compose = views.filterIsInstance<RootForTest>().single()
        }
    }

    private fun descendants(view: View): Sequence<View> = sequence {
        yield(view)
        if (view is ViewGroup) for (i in 0 until view.childCount) yieldAll(descendants(view.getChildAt(i)))
    }

    private fun hasCandidate(node: SemanticsNode): Boolean {
        val descriptions = node.config.getOrNull(SemanticsProperties.ContentDescription)
        return descriptions?.any { it == "候选 这" || it.startsWith("候选 这，") } == true ||
            node.children.any(::hasCandidate)
    }

    fun tap(x: Int, y: Int): Draw {
        val ready = CountDownLatch(1)
        var actualDown = 0L
        var actualUp = 0L
        var found: Draw? = null
        lateinit var original: Window.Callback
        val onDraw = ViewTreeObserver.OnDrawListener {
            val observed = System.nanoTime()
            if (actualUp > 0 && found == null && hasCandidate(compose.semanticsOwner.rootSemanticsNode)) {
                found = Draw(actualDown, actualUp, observed)
                ready.countDown()
            }
        }
        ins.runOnMainSync {
            check(!hasCandidate(compose.semanticsOwner.rootSemanticsNode))
            original = requireNotNull(window.callback)
            window.callback = object : Window.Callback by original {
                override fun dispatchTouchEvent(event: MotionEvent): Boolean {
                    when (event.actionMasked) {
                        MotionEvent.ACTION_DOWN -> actualDown = event.eventTime * 1_000_000L
                        MotionEvent.ACTION_UP -> actualUp = event.eventTime * 1_000_000L
                    }
                    return original.dispatchTouchEvent(event)
                }
            }
            root.viewTreeObserver.addOnDrawListener(onDraw)
        }
        try {
            device.executeShellCommand("cmd input touchscreen swipe $x $y $x $y 40")
            check(ready.await(3, TimeUnit.SECONDS)) { "No frame drew the candidate after touch" }
            return checkNotNull(found)
        } finally {
            ins.runOnMainSync {
                root.viewTreeObserver.removeOnDrawListener(onDraw)
                window.callback = original
            }
        }
    }
    // HWUI can choose a different timeline than Choreographer's preferred one.
    // The UI thread has only one active traversal at the instant we observe the glyph.
    fun submittedFrame(draw: Draw): org.json.JSONObject {
        val lines = device.executeShellCommand("dumpsys gfxinfo ${ins.targetContext.packageName} framestats").lines()
        val fields = lines.first { it.startsWith("Flags,FrameTimelineVsyncId,") }.trimEnd(',').split(',')
        val start = fields.indexOf("PerformTraversalsStart")
        val end = fields.indexOf("SyncQueued")
        val token = fields.indexOf("FrameTimelineVsyncId")
        check(start >= 0 && end >= 0 && token >= 0)
        val matches = lines.asSequence().filter { it.firstOrNull()?.isDigit() == true && ',' in it }
            .map { it.trimEnd(',').split(',') }
            .filter { it.size == fields.size && draw.observedNs in it[start].toLong()..it[end].toLong() }
            .distinctBy { it[token] }.toList()
        val frame = matches.singleOrNull() ?: error("Expected one submitted frame for $draw; found ${matches.size}")
        val fences = org.json.JSONArray()
        device.executeShellCommand("dumpsys SurfaceFlinger --latency $layer").lineSequence().forEach { line ->
            val fields = line.trim().split('\t')
            if (fields.size == 3) {
                val shown = fields[1].toLong()
                val ready = fields[2].toLong()
                if (ready in (draw.down - 50_000_000)..(draw.observedNs + 400_000_000) && shown in ready until Long.MAX_VALUE)
                    fences.put(org.json.JSONArray(listOf(ready, shown)))
            }
        }
        return org.json.JSONObject().apply {
            put("vsync_id", frame[token].toLong())
            put("layer_fences", fences)
            for (name in listOf("IntendedVsync", "HandleInputStart", "DrawStart", "SyncQueued", "SwapBuffers", "SwapBuffersCompleted", "GpuCompleted", "FrameCompleted")) {
                put(name, frame[fields.indexOf(name)].toLong())
            }
        }
    }

}
