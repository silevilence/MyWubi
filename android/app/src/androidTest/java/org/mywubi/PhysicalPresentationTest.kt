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
        val ime = android.content.ComponentName(app, MyWubiService::class.java).flattenToShortString()
        val wasEnabled = ime in dev.executeShellCommand("ime list -s").lineSequence().map { it.trim() }
        val previous=dev.executeShellCommand("settings get secure default_input_method").trim()
        val args=Configurator.getInstance()
        val previousIdle=args.waitForIdleTimeout
        fun key(name:String):androidx.test.uiautomator.UiObject2 {
            val until=SystemClock.elapsedRealtime()+15000
            do {
                ins.uiAutomation.clearCache()
                dev.findObject(By.pkg(app.packageName).desc(name).enabled(true))?.let {return it}
                SystemClock.sleep(100)
            } while (SystemClock.elapsedRealtime()<until)
            val visible=dev.findObjects(By.pkg(app.packageName).clickable(true)).take(25).map {it.contentDescription}
            error("Missing keyboard key $name. Exposed keys: $visible")
        }
        fun field()=By.pkg(host).clazz("android.widget.EditText")
        val candidate=By.pkg(app.packageName).desc(Pattern.compile("候选 这(?:，.*)?"))
        fun crop(bounds:Rect, automation: android.app.UiAutomation = ins.uiAutomation):Bitmap {
            val screen=requireNotNull(automation.takeScreenshot())
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
        report.put("motion_pattern", InstrumentationRegistry.getArguments().getString("motion_pattern", "stationary_swipe"))
        report.put("debuggable", app.applicationInfo.flags and android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE != 0)
        val out=File(app.filesDir,"physical-presentation.json")
        val latencies=mutableListOf<Long>()
        val trials = JSONArray()
        val memory = JSONArray()
        report.put("instrumented_pss_kib", memory)
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
            dev.executeShellCommand("ime enable $ime")
            dev.executeShellCommand("ime set $ime")
            dev.executeShellCommand("am force-stop $host")
            dev.executeShellCommand("am start -W -n $host/.EditorActivity")
            focusExternalEditor(dev)
            SystemClock.sleep(800)
            ins.uiAutomation.clearCache()
            val shift=dev.findObject(By.pkg(app.packageName).desc("大写 · 锁定"))
            shift?.click()
            key("p")
            SystemClock.sleep(500)
            ins.uiAutomation.clearCache()
            if(dev.hasObject(By.pkg(app.packageName).desc("英 → 中")))key("英 → 中").click()
            val keyBounds = key("p").visibleBounds
            // The right side of the outermost key overlaps the system back gesture.
            val point = android.graphics.Point(keyBounds.left + keyBounds.width()/8, keyBounds.centerY())
            tapAt(point.x,point.y)
            val word=requireNotNull(findFresh(dev,candidate,5000)) {
                ins.uiAutomation.clearCache()
                "Expected candidate 这 for p; host=${dev.findObject(field())?.text}; candidates=${dev.findObjects(By.pkg(app.packageName).descStartsWith("候选 ")).map { it.contentDescription }}"
            }
            val bounds=word.visibleBounds
            report.put("key_bounds_px", JSONArray(listOf(keyBounds.left, keyBounds.top, keyBounds.right, keyBounds.bottom)))
            report.put("candidate_bounds_px", JSONArray(listOf(bounds.left, bounds.top, bounds.right, bounds.bottom)))
            report.put("display_px", JSONArray(listOf(dev.displayWidth, dev.displayHeight)))
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
                // Resolve key coordinates with UI Automator, then release its accessibility service
                // during normal-input sampling. The host's own TextWatcher confirms every edit.
                val ordinary = measureLayer && android.os.Build.VERSION.SDK_INT >= 31 &&
                    InstrumentationRegistry.getArguments().getString("presentation_accessibility") != "inspected"
                val capture = if (ordinary) ins.getUiAutomation(android.app.UiAutomation.FLAG_DONT_USE_ACCESSIBILITY or
                    android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES) else ins.uiAutomation
                val timeline = if (measureLayer) CandidateFrameTimeline(capture) else null
                if (ordinary) SystemClock.sleep(250)
                report.put("inspection_attached", !ordinary)
                report.put("accessibility_enabled", app.getSystemService(android.view.accessibility.AccessibilityManager::class.java).isEnabled)
                fun journalText(expected: String) {
                    val end = SystemClock.uptimeMillis() + 3000
                    do {
                        val row = timeline!!.shell("tail -n 1 /sdcard/Android/data/$host/files/edits.jsonl").trim()
                        if (row.startsWith("{") && JSONObject(row).getString("text") == expected) return
                        SystemClock.sleep(20)
                    } while (SystemClock.uptimeMillis() < end)
                    error("Independent editor did not record '$expected'")
                }
                repeat(195) { i ->
                    val draw = timeline?.tap(point.x, point.y)
                    val start = draw?.down ?: System.nanoTime()
                    if (draw == null) assertTrue(tapAt(point.x,point.y))
                    // Screenshot readback can stall the GPU. Let the normal frame present first.
                    if (draw != null) SystemClock.sleep(250)
                    var matched=false
                    var elapsed=0L
                    while(!matched && elapsed<2000000) {
                        val frame=crop(bounds, capture)
                        try {matched=frame.sameAs(reference)} finally {frame.recycle()}
                        elapsed=(System.nanoTime()-start)/1000
                    }
                    assertTrue("Candidate pixels at trial $i",matched)
                    draw?.let {
                        trials.put(requireNotNull(timeline).submittedFrame(it).put("down_ns",it.down).put("release_ns",it.up)
                            .put("draw_ns",it.observedNs).put("matched_capture_us",elapsed))
                    }
                    if(i%65>=5)latencies.add(elapsed)
                    if (ordinary) {
                        timeline!!.shell("input tap ${bounds.centerX()} ${bounds.centerY()}")
                        journalText("这")
                        timeline.shell("input keyevent 67")
                        journalText("")
                        SystemClock.sleep(120)
                    } else {
                        tapAt(bounds.centerX(),bounds.centerY())
                        assertTrue(dev.wait(Until.hasObject(field().text("这")),3000))
                        dev.pressKeyCode(android.view.KeyEvent.KEYCODE_DEL)
                        assertTrue(awaitHostText(dev,field(),"",3000))
                        key("p");SystemClock.sleep(120)
                    }
                    if (i % 65 == 4 || i % 65 == 64) memory.put(
                        JSONObject().put("after_trial", i + 1).put("pss_kib", android.os.Debug.getPss()))
                }
                fun roundsOf(samples: List<Long>) = samples.chunked(60).map { batch ->
                    val sorted=batch.sorted()
                    JSONObject().put("p95_us",sorted[56]).put("p99_us",sorted[59])
                }
                if (ordinary) ins.uiAutomation // Reattach UI Automator for the navigation assertions.
                val captures = roundsOf(latencies)
                report.put("rounds",JSONArray(captures))
                val screenRounds = if (measureLayer) {
                    assertEquals(195, trials.length())
                    val held = mutableListOf<Long>()
                    val released = mutableListOf<Long>()
                    val physical = (0 until trials.length()).filter { it % 65 >= 5 }.map { index ->
                        val frame = trials.getJSONObject(index)
                        val gpu = frame.getLong("GpuCompleted")
                        val fences = frame.getJSONArray("layer_fences")
                        val buffer = (0 until fences.length()).map(fences::getJSONArray).single { it.getLong(0) == gpu }
                        val release = frame.getLong("release_ns")
                        held.add((release - frame.getLong("down_ns")) / 1000)
                        released.add((buffer.getLong(1) - release + 999) / 1000)
                        (buffer.getLong(1) - frame.getLong("down_ns")) / 1000
                    }
                    report.put("hold_rounds", JSONArray(roundsOf(held)))
                    report.put("release_to_display_rounds", JSONArray(roundsOf(released)))
                    roundsOf(physical).also { report.put("physical_rounds", JSONArray(it)) }
                } else captures
                args.waitForIdleTimeout = previousIdle
                // Cross the compact window boundary using both navigation paths.
                tapAt(point.x,point.y)
                key("展开").click()
                val grid = key("全部候选")
                val names = linkedSetOf<String>()
                val choices = By.pkg(app.packageName).descStartsWith("候选 ")
                for (page in 0..4) {
                    ins.uiAutomation.clearCache()
                    names.addAll(dev.findObjects(choices).sortedWith(compareBy({ it.visibleBounds.top }, { it.visibleBounds.left }))
                        .map { it.contentDescription })
                    if (names.size >= 15) break
                    val bounds = grid.visibleBounds
                    dev.swipe(bounds.centerX(), bounds.bottom - 10, bounds.centerX(), bounds.centerY(), 24)
                    dev.waitForIdle()
                }
                assertTrue("Need at least 15 distinct candidates, found $names", names.size >= 15)
                val later = names.elementAt(14)
                val text = later.removePrefix("候选 ").substringBefore("，")
                key(later).click()
                assertTrue(awaitHostText(dev, field(), text, 3000))
                repeat(text.codePointCount(0, text.length)) { dev.pressKeyCode(android.view.KeyEvent.KEYCODE_DEL) }
                assertTrue(awaitHostText(dev, field(), "", 3000))
                tapAt(point.x, point.y)
                val first = requireNotNull(findFresh(dev, candidate, 3000)).visibleBounds
                val end = key("展开").visibleBounds.left - 3
                val selector = By.pkg(app.packageName).desc(later)
                for (swipe in 0..12) {
                    ins.uiAutomation.clearCache()
                    val visible = dev.findObject(selector)?.visibleBounds
                    if (visible != null && visible.centerX() in (first.left + 8)..(end - 8)) break
                    dev.swipe(end - 12, first.centerY(), first.left + 12, first.centerY(), 48)
                    dev.waitForIdle()
                }
                var selected = key(later).visibleBounds
                var stable = 0
                val deadline = SystemClock.uptimeMillis() + 3000
                while (stable < 3 && SystemClock.uptimeMillis() < deadline) {
                    SystemClock.sleep(80)
                    val next = key(later).visibleBounds
                    stable = if (next == selected) stable + 1 else 0
                    selected = next
                }
                assertTrue("Candidate did not settle", stable == 3)
                tapAt(selected.centerX(), selected.centerY())
                val committed = awaitHostText(dev, field(), text, 3000)
                assertTrue("Scroll selection $later at $selected, editor=${dev.findObject(field())?.text}, visible=${dev.findObjects(choices).map { it.contentDescription }}", committed)
                report.put("expanded_and_scrolled_candidate", text)
                assertTrue("Haptics disabled must leave vibrator history unchanged", report.getBoolean("vibration_off_history_unchanged"))
                assertTrue("Haptics enabled must create an OS record", report.getBoolean("vibration_on_new_os_record"))
                assertTrue("Visible candidate p95/p99 exceed 200/300 ms",
                    screenRounds.size==3 && screenRounds.all { it.getLong("p95_us")<=200000 && it.getLong("p99_us")<=300000 })
                if (measureLayer) {
                    val rounds = report.getJSONArray("release_to_display_rounds")
                    val pass = (0 until rounds.length()).all { rounds.getJSONObject(it).getLong("p95_us") <= 50_000 }
                    report.put("ak_release_p95_pass", pass)
                    if (InstrumentationRegistry.getArguments().getString("require_ak_50") == "true") {
                        assertTrue("AK-17 release-to-display P95 exceeds 50 ms: $rounds", pass)
                    }
                }
            } finally {reference.recycle()}
        } finally {
            report.put("boot_minus_monotonic_end_ns", clockOffset())
            report.put("trials", trials)
            if(originalReceipt!=null)receipt.writeBytes(originalReceipt)
            report.put("tap_to_matching_capture_us",JSONArray(latencies))
            out.writeText(report.toString())
            if (InstrumentationRegistry.getArguments().getString("presentation_export") == "true") {
                File(app.getExternalFilesDir(null), out.name).writeText(report.toString())
            }
            args.waitForIdleTimeout=previousIdle
            val edit=ui.edit()
            if(had)edit.putBoolean("vibration",old) else edit.remove("vibration")
            if(hadClip)edit.putBoolean("clipboard_suggestion",clip) else edit.remove("clipboard_suggestion")
            edit.commit()
            dev.executeShellCommand("am force-stop $host")
            if(previous.isNotEmpty())dev.executeShellCommand("ime set $previous")
            if (!wasEnabled) dev.executeShellCommand("ime disable $ime")
        }
    }
}
