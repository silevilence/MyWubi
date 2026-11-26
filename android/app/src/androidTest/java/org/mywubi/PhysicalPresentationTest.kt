package org.mywubi

import android.graphics.Bitmap
import android.graphics.Rect
import android.os.SystemClock
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.util.regex.Pattern

class PhysicalPresentationTest {
    @Test fun tapToCapturedCandidateAndVibrationSwitch() {
        org.junit.Assume.assumeTrue(InstrumentationRegistry.getArguments().getString("physical_acceptance")=="true")
        val ins=InstrumentationRegistry.getInstrumentation()
        val app=ins.targetContext
        val dev=UiDevice.getInstance(ins)
        val host="org.mywubi.acceptancehost"
        val ui=app.getSharedPreferences("android-ui",0)
        val old=ui.getBoolean("vibration",false)
        val had=ui.contains("vibration")
        val clip=ui.getBoolean("clipboard_suggestion",true)
        val hadClip=ui.contains("clipboard_suggestion")
        val previous=dev.executeShellCommand("settings get secure default_input_method").trim()
        val args=Configurator.getInstance()
        val previousIdle=args.waitForIdleTimeout
        fun key(name:String):androidx.test.uiautomator.UiObject2 {
            val until=SystemClock.elapsedRealtime()+15000
            do {
                ins.uiAutomation.clearCache()
                dev.findObject(By.pkg("org.mywubi").desc(name).enabled(true))?.let {return it}
                SystemClock.sleep(100)
            } while (SystemClock.elapsedRealtime()<until)
            val visible=dev.findObjects(By.pkg("org.mywubi").clickable(true)).take(25).map {it.contentDescription}
            error("Missing keyboard key $name. Exposed keys: $visible")
        }
        fun field()=By.pkg(host).clazz("android.widget.EditText")
        val candidate=By.pkg("org.mywubi").desc(Pattern.compile("候选 这(?:，.*)?"))
        fun crop(bounds:Rect):Bitmap {
            val screen=requireNotNull(ins.uiAutomation.takeScreenshot())
            try {return Bitmap.createBitmap(screen,bounds.left,bounds.top,bounds.width(),bounds.height())}
            finally { screen.recycle() }
        }
        fun tapAt(x:Int,y:Int):Boolean {
            val start=SystemClock.uptimeMillis()
            fun send(action:Int):Boolean {
                val event=android.view.MotionEvent.obtain(start,SystemClock.uptimeMillis(),action,x.toFloat(),y.toFloat(),0)
                event.source=android.view.InputDevice.SOURCE_TOUCHSCREEN
                return try {ins.uiAutomation.injectInputEvent(event,false)} finally {event.recycle()}
            }
            if(!send(android.view.MotionEvent.ACTION_DOWN)) return false
            SystemClock.sleep(40)
            return send(android.view.MotionEvent.ACTION_UP)
        }
        val report=JSONObject()
        val out=File(app.filesDir,"physical-presentation.json")
        val latencies=mutableListOf<Long>()
        val trials = JSONArray()
        fun clockOffset() = SystemClock.elapsedRealtimeNanos() - System.nanoTime()
        report.put("boot_minus_monotonic_start_ns", clockOffset())
        fun haptics():String=dev.executeShellCommand("dumpsys vibrator_manager").lineSequence().filter { line ->
            val completed = "finished" in line && ("Usage=TOUCH" in line || "usage: TOUCH" in line)
            val appRecord = "opPkg=${app.packageName}," in line || "| ${app.packageName} (uid=" in line
            val systemRecord = "opPkg=android," in line && "uid=1000," in line
            completed && (appRecord || systemRecord)
        }.joinToString("\n")
        val receipt=File(EngineStore(app).configFile.parentFile,"keyboard-selection.json")
        val originalReceipt=receipt.takeIf {it.exists()}?.readBytes()
        try {
            if(originalReceipt!=null) check(receipt.delete())
            ui.edit().putBoolean("clipboard_suggestion",false).putBoolean("vibration",false).commit()
            dev.executeShellCommand("ime enable org.mywubi/.MyWubiService")
            dev.executeShellCommand("ime set org.mywubi/.MyWubiService")
            dev.executeShellCommand("am force-stop $host")
            dev.executeShellCommand("am start -W -n $host/.EditorActivity")
            focusExternalEditor(dev)
            SystemClock.sleep(800)
            ins.uiAutomation.clearCache()
            val shift=dev.findObject(By.pkg("org.mywubi").desc("大写 · 锁定"))
            shift?.click()
            key("p")
            SystemClock.sleep(500)
            ins.uiAutomation.clearCache()
            if(dev.hasObject(By.pkg("org.mywubi").desc("英 → 中")))key("英 → 中").click()
            val keyBounds = key("p").visibleBounds
            // The right side of the outermost key overlaps the system back gesture.
            val point = android.graphics.Point(keyBounds.left + keyBounds.width()/8, keyBounds.centerY())
            tapAt(point.x,point.y)
            val word=requireNotNull(findFresh(dev,candidate,5000)) {
                ins.uiAutomation.clearCache()
                "Expected candidate 这 for p; host=${dev.findObject(field())?.text}; candidates=${dev.findObjects(By.pkg("org.mywubi").descStartsWith("候选 ")).map { it.contentDescription }}"
            }
            val bounds=word.visibleBounds
            bounds.inset(6,6)
            SystemClock.sleep(250)
            val reference=crop(bounds)
            try {
                tapAt(bounds.centerX(),bounds.centerY())
                assertTrue(dev.wait(Until.hasObject(field().text("这")),3000))
                dev.pressKeyCode(android.view.KeyEvent.KEYCODE_DEL)
                assertTrue(awaitHostText(dev,field(),"",3000))
                key("p")
                SystemClock.sleep(180)
                val empty=crop(bounds)
                try {
                    assertFalse("Baseline and candidate pixels must differ",empty.sameAs(reference))
                }
                finally {empty.recycle()}
                val captureOverhead = JSONArray()
                repeat(8) {
                    val start = SystemClock.elapsedRealtimeNanos()
                    crop(bounds).recycle()
                    captureOverhead.put((SystemClock.elapsedRealtimeNanos()-start)/1000)
                }
                report.put("screen_capture_us",captureOverhead)
                val prior=haptics()
                fun commitForFeedback() {
                    tapAt(point.x,point.y)
                    requireNotNull(findFresh(dev,candidate,5000))
                    tapAt(bounds.centerX(),bounds.centerY())
                    assertTrue(dev.wait(Until.hasObject(field().text("这")),3000))
                    dev.pressKeyCode(android.view.KeyEvent.KEYCODE_DEL)
                    assertTrue(awaitHostText(dev,field(),"",3000))
                    key("p")
                    SystemClock.sleep(200)
                }
                commitForFeedback()
                val off=haptics()
                report.put("vibration_off_history_unchanged",prior==off)
                ui.edit().putBoolean("vibration",true).commit()
                commitForFeedback()
                val enabledHistory = haptics()
                val newFeedback = enabledHistory.lineSequence().filter { it.isNotEmpty() && it !in off.lines() }.toList()
                report.put("vibration_on_new_os_record",newFeedback.isNotEmpty())
                report.put("new_feedback_records",JSONArray(newFeedback))
                ui.edit().putBoolean("vibration",false).commit()
                args.waitForIdleTimeout=0
                val measureLayer = InstrumentationRegistry.getArguments().getString("presentation_layer") != "false"
                val timeline = if (measureLayer) CandidateFrameTimeline(dev) else null
                repeat(195) { i ->
                    val draw = timeline?.tap(point.x, point.y)
                    val start = draw?.down ?: System.nanoTime()
                    if (draw == null) assertTrue(tapAt(point.x,point.y))
                    // Screenshot readback can stall the GPU. Let the normal frame present first.
                    if (draw != null) SystemClock.sleep(250)
                    var matched=false
                    var elapsed=0L
                    while(!matched && elapsed<2000000) {
                        val frame=crop(bounds)
                        try {matched=frame.sameAs(reference)} finally {frame.recycle()}
                        elapsed=(System.nanoTime()-start)/1000
                    }
                    assertTrue("Candidate pixels at trial $i",matched)
                    draw?.let {
                        trials.put(requireNotNull(timeline).submittedFrame(it).put("down_ns",it.down).put("release_ns",it.up)
                            .put("draw_ns",it.observedNs).put("matched_capture_us",elapsed))
                    }
                    if(i%65>=5)latencies.add(elapsed)
                    tapAt(bounds.centerX(),bounds.centerY())
                    assertTrue(dev.wait(Until.hasObject(field().text("这")),3000))
                    dev.pressKeyCode(android.view.KeyEvent.KEYCODE_DEL)
                    assertTrue(awaitHostText(dev,field(),"",3000))
                    key("p");SystemClock.sleep(120)
                }
                fun roundsOf(samples: List<Long>) = samples.chunked(60).map { batch ->
                    val sorted=batch.sorted()
                    JSONObject().put("p95_us",sorted[56]).put("p99_us",sorted[59])
                }
                val captures = roundsOf(latencies)
                report.put("rounds",JSONArray(captures))
                val screenRounds = if (measureLayer) {
                    assertEquals(195, trials.length())
                    val physical = (0 until trials.length()).filter { it % 65 >= 5 }.map { index ->
                        val frame = trials.getJSONObject(index)
                        val gpu = frame.getLong("GpuCompleted")
                        val fences = frame.getJSONArray("layer_fences")
                        val buffer = (0 until fences.length()).map(fences::getJSONArray).single { it.getLong(0) == gpu }
                        (buffer.getLong(1) - frame.getLong("down_ns")) / 1000
                    }
                    roundsOf(physical).also { report.put("physical_rounds", JSONArray(it)) }
                } else captures
                assertTrue("Haptics disabled must leave vibrator history unchanged", report.getBoolean("vibration_off_history_unchanged"))
                assertTrue("Haptics enabled must create an OS record", report.getBoolean("vibration_on_new_os_record"))
                assertTrue("Visible candidate p95/p99 exceed 200/300 ms",
                    screenRounds.size==3 && screenRounds.all { it.getLong("p95_us")<=200000 && it.getLong("p99_us")<=300000 })
            } finally {reference.recycle()}
        } finally {
            report.put("boot_minus_monotonic_end_ns", clockOffset())
            report.put("trials", trials)
            if(originalReceipt!=null)receipt.writeBytes(originalReceipt)
            report.put("tap_to_matching_capture_us",JSONArray(latencies))
            out.writeText(report.toString())
            args.waitForIdleTimeout=previousIdle
            val edit=ui.edit()
            if(had)edit.putBoolean("vibration",old) else edit.remove("vibration")
            if(hadClip)edit.putBoolean("clipboard_suggestion",clip) else edit.remove("clipboard_suggestion")
            edit.commit()
            dev.executeShellCommand("am force-stop $host")
            if(previous.isNotEmpty())dev.executeShellCommand("ime set $previous")
        }
    }
}
