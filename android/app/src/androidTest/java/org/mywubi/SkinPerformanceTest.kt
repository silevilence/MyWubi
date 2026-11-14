package org.mywubi

import android.os.Debug
import android.os.SystemClock
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.lang.ref.WeakReference

/** Loader/bitmap resource measurement only; deliberately makes no frame-latency claim. */
class SkinPerformanceTest {
    @Test fun thousandImageReloadsReleaseRetiredBitmaps() {
        val instrumentation=InstrumentationRegistry.getInstrumentation()
        val context=instrumentation.targetContext
        val root=File(context.cacheDir,"skin-performance").apply {mkdirs()}
        val path=File(root,"config.toml").apply {writeText("[dictionary]\nsystem_table=\"test.dict\"\nuser_table=\"user.dict\"\n")}
        File(root,"test.dict").writeText("a\t工\t100\n")
        val config=NativeBridge.objectCall("ReadConfig","path" to path.path).let {read->
            NativeHandle(read.getLong("version")).close();read.getJSONObject("config")
        }
        val source=File(root,"editor-bamboo.mywubi-skin")
        instrumentation.context.assets.open("skins/editor-bamboo.mywubi-skin").use {input->source.outputStream().use {input.copyTo(it)}}
        NativeBridge.objectCall("ManageSkin","request" to JSONObject().put("action","Import").put("path",path.path).put("source",source.path).put("platform","android"))
        config.getJSONObject("skins").getJSONObject("android").put("selection",JSONObject().put("id","mywubi.bamboo").put("version","1").put("component","jade"))
        val times=mutableListOf<Long>()
        val retired=mutableListOf<WeakReference<android.graphics.Bitmap>>()
        val curve=JSONArray()
        var active:JSONObject?=null
        fun sample(step:Int) {
            System.gc();System.runFinalization();SystemClock.sleep(100)
            val memory=Debug.MemoryInfo();Debug.getMemoryInfo(memory)
            curve.put(JSONObject().put("switch",step).put("pss_kib",memory.totalPss)
                .put("private_dirty_kib",memory.totalPrivateDirty).put("native_allocated_bytes",Debug.getNativeHeapAllocatedSize())
                .put("retired_bitmaps_alive",retired.count {it.get()!=null}))
            File(context.filesDir,"skin-performance-progress.json").writeText(curve.toString())
        }
        for(n in 0 until 1020) {
            active?.let {old->
                val bitmap=(old.getJSONObject("light").get("decodedImages") as SkinImages).roles.getValue("background").bitmap
                retired.add(WeakReference(bitmap))
            }
            config.getJSONObject("skins").getJSONObject("android").put("mode",if(n%2==0) "light" else "dark")
            val start=SystemClock.elapsedRealtimeNanos()
            active=loadSkinAppearance(path,config)
            if(n>=20) times.add(SystemClock.elapsedRealtimeNanos()-start)
            val light=active.getJSONObject("light").get("decodedImages") as SkinImages
            val dark=active.getJSONObject("dark").get("decodedImages") as SkinImages
            assertSame(light.roles.getValue("background").bitmap,dark.roles.getValue("background").bitmap)
            if(n>=19 && (n+1)%25==0 || n==19) sample(n+1-20)
        }
        assertNotNull(active)
        sample(1000)
        val ordered=times.sorted()
        val result=JSONObject().put("scope","Android ART loader, JNI and bitmap decode; no touch-to-visible measurement")
            .put("device",android.os.Build.MODEL).put("abi",android.os.Build.SUPPORTED_ABIS.first())
            .put("warmup",20).put("switches",times.size).put("p50_ns",ordered[499])
            .put("p95_ns",ordered[949]).put("p99_ns",ordered[989]).put("curve",curve)
        File(context.filesDir,"skin-performance.json").writeText(result.toString(2))
        assertEquals(1000,times.size)
    }
}
