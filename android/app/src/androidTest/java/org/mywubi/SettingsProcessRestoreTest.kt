package org.mywubi

import android.os.Process
import android.os.SystemClock
import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/** Run the two steps separately, with am force-stop between them. Back up any previous draft. */
class SettingsProcessRestoreTest {
    private val ins = InstrumentationRegistry.getInstrumentation()
    private val app = ins.targetContext
    private val dir get() = File(app.cacheDir,"pgz110-process-restore")
    private val draft get() = File(app.filesDir,"settings-draft.json")
    private fun withModel(action:(SettingsModel)->Unit) {
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            val until=SystemClock.elapsedRealtime()+15000
            var ready=false
            while(!ready && SystemClock.elapsedRealtime()<until) {
                scenario.onActivity { val m=ViewModelProvider(it)[SettingsModel::class.java];ready=m.config!=null && !m.busy }
                if(!ready) SystemClock.sleep(80)
            }
            assertTrue(ready)
            scenario.onActivity {action(ViewModelProvider(it)[SettingsModel::class.java])}
        }
    }
    @Test fun saveSkinDraftInFirstProcess() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("process_restore")=="true")
        check(!dir.exists() && dir.mkdir())
        val config=EngineStore(app).configFile
        val was=draft.isFile
        if(was)draft.copyTo(File(dir,"original.json"))
        File(dir,"config.before").writeBytes(config.readBytes())
        try {
            if(was)check(draft.delete())
            withModel { m ->
                val android=JSONObject(m.config!!.getJSONObject("skins").getJSONObject("android").toString())
                val newMode=if(android.optString("mode")=="dark") "light" else "dark"
                android.put("mode",newMode)
                File(dir,"state.json").writeText(JSONObject().put("pid",Process.myPid()).put("mode",newMode).toString())
                m.change("skins","android",android)
                assertTrue(m.dirty)
            }
            assertTrue(draft.isFile)
            assertArrayEquals(File(dir,"config.before").readBytes(),config.readBytes())
        } catch(e:Throwable) {restore();throw e}
    }
    private fun restore() {
        val original=File(dir,"original.json")
        if(original.exists())original.copyTo(draft,overwrite=true) else draft.delete()
        dir.deleteRecursively()
    }
    @Test fun recoverPersistedDraftInNewProcess() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("process_restore")=="true")
        check(File(dir,"state.json").isFile)
        try {
            val state=JSONObject(File(dir,"state.json").readText())
            assertNotEquals(state.getInt("pid"),Process.myPid())
            withModel { m ->
                assertTrue("New settings process lost its draft",m.dirty)
                assertEquals(state.getString("mode"),m.config!!.getJSONObject("skins").getJSONObject("android").getString("mode"))
            }
            assertArrayEquals(File(dir,"config.before").readBytes(),EngineStore(app).configFile.readBytes())
        } finally {restore()}
    }
}
