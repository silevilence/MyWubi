package org.mywubi

import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File

class PersonalDataTest {
    @Test fun captureTrainExchangeAndPrivacyUseTheNativeProductPath() {
        val app=InstrumentationRegistry.getInstrumentation().targetContext
        val root=File(app.cacheDir,"personal-${System.nanoTime()}").also{it.mkdirs()}
        val config=File(root,"config.toml").also{it.writeText("[personal]\ncollect=true\nlearn=true\npredict=false\n")}
        fun capture(op:String,vararg fields:Pair<String,Any?>){
            val request=JSONObject().put("op",op);fields.forEach{(k,v)->request.put(k,v ?: JSONObject.NULL)}
            val reply=JSONObject(NativeBridge.personalCapture(request.toString().toByteArray()).toString(Charsets.UTF_8))
            assertTrue(reply.toString(),reply.getBoolean("ok"))
        }
        fun manage(op:String,vararg fields:Pair<String,Any?>):JSONObject{
            val request=JSONObject().put("op",op).put("config",config.absolutePath);fields.forEach{(k,v)->request.put(k,v ?: JSONObject.NULL)}
            val reply=JSONObject(NativeBridge.personalManage(request.toString().toByteArray()).toString(Charsets.UTF_8))
            assertTrue(reply.toString(),reply.getBoolean("ok"));return reply.getJSONObject("value")
        }
        val settings=JSONObject().put("collect",true).put("learn",true).put("predict",false)
        val context=JSONObject().put("revision",1).put("left","明天去").put("right","").put("selected","")
            .put("selection",JSONObject.NULL).put("truncated",false).put("discontinuity",false)
        fun commit(){capture("Operation","operation" to JSONObject().put("CommitAndReplace",JSONObject().put("text","图书馆").put("replace_len",0)),"outcome" to "Confirmed","after" to null,"method" to "fixture")}
        try {
            capture("Configure","config" to config.absolutePath,"settings" to settings,"allowed" to true,"session" to "fixture")
            capture("Context","context" to context,"external" to false)
            repeat(40){commit()}
            val end=android.os.SystemClock.uptimeMillis()+10000
            var records=0
            do {records=manage("Status").getJSONObject("data").getInt("records");if(records>=42)break;android.os.SystemClock.sleep(50)}while(android.os.SystemClock.uptimeMillis()<end)
            assertEquals(42,records)
            capture("Configure","config" to config.absolutePath,"settings" to settings,"allowed" to false,"session" to "password")
            commit();android.os.SystemClock.sleep(100)
            assertEquals(records,manage("Status").getJSONObject("data").getInt("records"))
            val model=manage("Train").getString("version")
            assertEquals(model,manage("Train").getString("version"))
            for(password in listOf(null,"本地交换密码")){
                val file=File(root,if(password==null)"plain.mwp" else "encrypted.mwp")
                manage("Export","path" to file.absolutePath,"password" to password)
                val preview=manage("Preview","path" to file.absolutePath,"password" to password,"mode" to "Merge")
                assertEquals(0,preview.getInt("added"));assertEquals(records,preview.getInt("duplicates"))
                manage("Import","path" to file.absolutePath,"password" to password,"mode" to "Merge","revision" to preview.getString("revision"))
            }
            File(root,"plain.mwp").copyTo(File(app.filesDir,"mi-personal-android.mwp"),overwrite=true)
            assertEquals(40,manage("Status").getJSONObject("model").getInt("samples"))
            config.writeText("[personal]\ncollect=false\nlearn=true\npredict=true\n")
            capture("Configure","config" to config.absolutePath,"settings" to JSONObject().put("collect",false),"allowed" to true,"session" to "off")
            manage("Clear","models" to false);assertFalse(manage("Status").isNull("model"))
            manage("Clear","models" to true);assertTrue(manage("Status").isNull("model"))
        } finally {capture("Close");root.deleteRecursively()}
    }
    @Test fun windowsPackagesRoundTripThroughAndroidWithoutDuplicatingEvents() {
        val app=InstrumentationRegistry.getInstrumentation().targetContext
        val external=app.getExternalFilesDir(null)!!
        val plain=File(external,"windows-plain.mwp")
        val encrypted=File(external,"windows-encrypted.mwp")
        org.junit.Assume.assumeTrue("provision locally generated Windows packages",plain.isFile && encrypted.isFile)
        val root=File(app.cacheDir,"cross-platform-${System.nanoTime()}").also{it.mkdirs()}
        val config=File(root,"config.toml").apply{writeText("[personal]\ncollect=false\nlearn=true\npredict=true\n")}
        fun call(op:String,vararg fields:Pair<String,Any?>):JSONObject {
            val request=JSONObject().put("op",op).put("config",config.absolutePath)
            fields.forEach{(k,v)->request.put(k,v ?: JSONObject.NULL)}
            val reply=JSONObject(NativeBridge.personalManage(request.toString().toByteArray()).toString(Charsets.UTF_8))
            assertTrue(reply.toString(),reply.getBoolean("ok"));return reply.getJSONObject("value")
        }
        try {
            var count=0
            for((index,file) in listOf(plain,encrypted).withIndex()) {
                val password=if(index==0)null else "synthetic-test-password"
                val preview=call("Preview","path" to file.absolutePath,"password" to password,"mode" to "Merge")
                if(index==0){count=preview.getInt("added");assertTrue(count>=100)}
                else {assertEquals(0,preview.getInt("added"));assertEquals(count,preview.getInt("duplicates"))}
                call("Import","path" to file.absolutePath,"password" to password,"mode" to "Merge","revision" to preview.getString("revision"))
            }
            call("Train")
            assertEquals(count,call("Status").getJSONObject("data").getInt("records"))
            call("Export","path" to File(external,"android-roundtrip-plain.mwp").absolutePath,"password" to null)
            call("Export","path" to File(external,"android-roundtrip-encrypted.mwp").absolutePath,"password" to "synthetic-test-password")
        } finally {root.deleteRecursively()}
    }
}
