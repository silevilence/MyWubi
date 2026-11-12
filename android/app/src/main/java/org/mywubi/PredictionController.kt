package org.mywubi

import android.app.ActivityManager
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.BatteryManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import org.json.JSONObject

/** Rust owns one bounded worker. This controller only exchanges small messages
 * and polls on the main looper; no model I/O or inference runs on that looper. */
class PredictionController(private val context:Context,private val changed:()->Unit) {
    private val main=Handler(Looper.getMainLooper())
    private var activeKey:String?=null
    private var currentSession:Long?=null
    private var configured=false
    private var configuredFor=""
    private var pressureChecked=0L
    private var pressure=false
    private var lifecycleAllowed=false
    private var awaiting=false
    private var personal=false
    var personalVersion:String?=null; private set
    private var personalConfig:java.io.File?=null
    private var learningVisible=false
    private val learningPoll=Runnable { if(learningVisible)learning(false,true) }
    fun configurePersonal(file:java.io.File){personalConfig=file}
    private fun personalCommand(op:String,vararg fields:Pair<String,Any?>):JSONObject {
        val request=JSONObject().put("op",op);fields.forEach{(k,v)->request.put(k,v ?: JSONObject.NULL)}
        return JSONObject(NativeBridge.personalRuntime(request.toString().toByteArray(Charsets.UTF_8)).toString(Charsets.UTF_8))
    }
    fun learning(busy:Boolean,visible:Boolean) {
        learningVisible=visible
        main.removeCallbacks(learningPoll)
        val file=personalConfig ?: return
        val battery=context.registerReceiver(null,IntentFilter(Intent.ACTION_BATTERY_CHANGED))
        val charging=battery?.getIntExtra(BatteryManager.EXTRA_PLUGGED,0)?.let{it!=0}==true
        runCatching{personalCommand("Learning","config" to file.absolutePath,"allowed" to (visible && charging && !resourcePressure()),"busy" to busy)}
        if(visible)main.postDelayed(learningPoll,1000)
    }
    private val poll=Runnable { currentSession?.let { update(it,lifecycleAllowed) } }
    init { NativeBridge.runtimeDirectory(context.applicationInfo.nativeLibraryDir) }
    private fun command(op:String,vararg fields:Pair<String,Any>):JSONObject {
        val request=JSONObject().put("op",op);fields.forEach { (k,v)->request.put(k,v) }
        return JSONObject(NativeBridge.inference(request.toString().toByteArray(Charsets.UTF_8)).toString(Charsets.UTF_8))
    }
    private fun resourcePressure():Boolean {
        val now=android.os.SystemClock.elapsedRealtime()
        if(now-pressureChecked<1000) return pressure
        pressureChecked=now
        pressure=readPressure()
        return pressure
    }
    private fun readPressure():Boolean {
        val memory=ActivityManager.MemoryInfo()
        context.getSystemService(ActivityManager::class.java).getMemoryInfo(memory)
        if(memory.lowMemory) return true
        val power=context.getSystemService(PowerManager::class.java)
        if(power.isPowerSaveMode || (Build.VERSION.SDK_INT>=29 && power.currentThermalStatus>=PowerManager.THERMAL_STATUS_SEVERE)) return true
        val battery=context.registerReceiver(null,IntentFilter(Intent.ACTION_BATTERY_CHANGED)) ?: return false
        val level=battery.getIntExtra(BatteryManager.EXTRA_LEVEL,-1)
        val scale=battery.getIntExtra(BatteryManager.EXTRA_SCALE,100)
        return battery.getIntExtra(BatteryManager.EXTRA_PLUGGED,0)==0 && level>=0 && scale>0 && level*100/scale<=15
    }
    fun cancel() {
        runCatching { personalCommand("Invalidate") }
        activeKey=null;main.removeCallbacks(poll)
        if(configured) runCatching { command("Invalidate") }
    }
    fun pause() {
        learning(false,false)
        lifecycleAllowed=false
        cancel()
        if(configured) runCatching { command("Pause","paused" to true) }
    }
    fun update(session:Long,allowed:Boolean) {
        lifecycleAllowed=allowed
        try { updateChecked(session,allowed) }
        catch (_:RuntimeException) {
            cancel()
            // Keep checking the core's deadline even when a device service or
            // native endpoint is unavailable; ordinary input stays usable.
            if(allowed) main.postDelayed(poll,16)
        }
    }
    private fun updateChecked(session:Long,allowed:Boolean) {
        if(currentSession!=null && currentSession!=session) cancel()
        currentSession=session
        if(!allowed) {pause();return}
        val work=NativeBridge.objectCall("PredictionWork","session" to session)
        val config=work.getJSONObject("config")
        personal=work.optJSONObject("personal")?.optBoolean("predict")==true
        if(personal){
            if(configured){command("Close");configured=false;configuredFor=""}
            val request=work.optJSONObject("request")
            if(request==null){activeKey=null;return}
            val key=request.getJSONObject("key")
            if(resourcePressure()) {NativeBridge.call("PredictionResult","session" to session,"key" to key,"response" to null);changed();return}
            if(activeKey!=key.toString()){
                personalCommand("Submit","root" to work.getString("personal_root"),"request" to request,"right" to work.opt("right"))
                activeKey=key.toString()
            }
            val reply=personalCommand("Poll","key" to key)
            val value=reply.optJSONObject("value")
            if(value!=null)personalVersion=reply.opt("model_version") as? String
            if(value!=null){NativeBridge.call("PredictionResult","session" to session,"key" to key,"response" to value.optJSONObject("Ok"));activeKey=null;changed()}
            else{main.removeCallbacks(poll);main.postDelayed(poll,16)}
            return
        }
        if(!config.optBoolean("enabled") || config.isNull("model")) {
            awaiting=false;cancel();if(configured) {command("Close");configured=false;configuredFor=""};return
        }
        val request=work.optJSONObject("request")
        if(resourcePressure()) {
            pause()
            if(request!=null) NativeBridge.call("PredictionResult","session" to session,"key" to request.getJSONObject("key"),"response" to null)
            awaiting=false; lifecycleAllowed=true
            main.postDelayed(poll,1000);changed();return
        }
        val preload=config.optString("loading","on_demand")=="preload_keep"
        if(request==null && !preload) {val notify=awaiting;awaiting=false;cancel();if(notify)changed();return}
        val identity=work.getString("root")+config.toString()
        if(configuredFor!=identity) {
            cancel()
            val result=command("Configure","root" to work.getString("root"),"config" to config)
            if(result.getBoolean("ok")){configured=true;configuredFor=identity}
        }
        check(configured)
        command("Pause","paused" to false)
        if(request==null) {
            val notify=awaiting;awaiting=false
            if(activeKey!=null) cancel()
            main.removeCallbacks(poll);main.postDelayed(poll,1000)
            if(notify)changed()
            return
        }
        awaiting=true
        val key=request.getJSONObject("key")
        fun fallback() { NativeBridge.call("PredictionResult","session" to session,"key" to key,"response" to null);activeKey=null;awaiting=false }
        try {
            if(activeKey!=key.toString()) {
                cancel()
                check(configured);command("Pause","paused" to false)
                check(command("Submit","request" to request).getBoolean("ok"))
                activeKey=key.toString()
            }
            val result=command("Poll","key" to key)
            check(result.getBoolean("ok"))
            val value=result.optJSONObject("value")
            if(value!=null) {
                NativeBridge.call("PredictionResult","session" to session,"key" to key,"response" to value.optJSONObject("Ok"))
                activeKey=null;awaiting=false;changed()
            } else {main.removeCallbacks(poll);main.postDelayed(poll,16)}
        } catch (_:RuntimeException) {fallback();changed()}
    }
    fun close() {pause();if(configured)runCatching { command("Close") };configured=false;currentSession=null}
}
