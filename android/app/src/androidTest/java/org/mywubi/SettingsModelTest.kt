package org.mywubi

import android.app.Application
import android.net.Uri
import android.os.SystemClock
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Test
import java.io.File

class SettingsModelTest {
    @Test fun predictionSettingsPersistWithoutInstallingOrEnablingAModelImplicitly() {
        val instrumentation=InstrumentationRegistry.getInstrumentation()
        val app=instrumentation.targetContext.applicationContext as Application
        val store=EngineStore(app)
        Resources.prepare(app)
        val original=store.configFile.readBytes()
        var owner=ViewModelStore()
        lateinit var model:SettingsModel
        fun main(action:()->Unit)=instrumentation.runOnMainSync(action)
        fun ready() {
            val end=SystemClock.uptimeMillis()+20000
            var busy=true
            while(busy && SystemClock.uptimeMillis()<end) { main {busy=model.busy};SystemClock.sleep(25) }
            assertFalse("settings timed out",busy)
        }
        fun create() { main {model=ViewModelProvider(owner,ViewModelProvider.AndroidViewModelFactory(app))[SettingsModel::class.java]};ready() }
        try {
            create();main {model.reload(true)};ready()
            main {
                model.change("prediction","enabled",false)
                model.change("prediction","model",org.json.JSONObject.NULL)
                model.change("prediction","loading","preload_keep")
                model.change("prediction","wait_ms",850)
                model.save()
            };ready()
            main {assertFalse(model.message,model.dirty);owner.clear()};owner=ViewModelStore();create()
            main {
                val p=model.config!!.getJSONObject("prediction")
                assertFalse(p.getBoolean("enabled"));assertTrue(p.isNull("model"))
                assertEquals(850,p.getInt("wait_ms"));assertEquals("preload_keep",p.getString("loading"))
                model.change("prediction","model",org.json.JSONObject().put("id","absent-mi27").put("version","v1"))
                model.change("prediction","enabled",true);model.save()
            };ready()
            main {assertFalse(model.message,model.dirty);owner.clear()};owner=ViewModelStore();create()
            main {
                val p=model.config!!.getJSONObject("prediction")
                assertTrue(p.getBoolean("enabled"));assertEquals("absent-mi27",p.getJSONObject("model").getString("id"))
                model.change("prediction","enabled",false);model.save()
            };ready()
            main {assertFalse(model.message,model.dirty)}
        } finally {
            store.configFile.writeBytes(original)
            main {model.reload(true)};ready();main {owner.clear()}
            EngineUpdates.request(app)
        }
    }

    @Test fun cancellingInvalidImportPreservesEditableDraftAcrossReopen() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val app = instrumentation.targetContext.applicationContext as Application
        val store = EngineStore(app)
        val original = store.configFile.readBytes()
        val invalid = File.createTempFile("invalid-table", ".dict", app.cacheDir)
        invalid.writeText("not a valid table row")
        var owner = ViewModelStore()
        lateinit var model: SettingsModel
        fun main(action: () -> Unit) = instrumentation.runOnMainSync(action)
        fun ready() {
            val deadline = SystemClock.uptimeMillis() + 15000
            var busy = true
            while (busy && SystemClock.uptimeMillis() < deadline) {
                main {busy=model.busy}; SystemClock.sleep(25)
            }
            assertFalse("operation timed out",busy)
        }
        fun create() {
            main {model=ViewModelProvider(owner,ViewModelProvider.AndroidViewModelFactory(app))[SettingsModel::class.java]}
            ready()
        }
        create()
        try {
            main {model.reload(true)}; ready()
            var changed = false
            main {
                changed = !model.config!!.getJSONObject("basic").getBoolean("auto_commit_unique")
                model.change("basic","auto_commit_unique",changed)
                model.stage(listOf(Uri.fromFile(invalid)))
            }
            ready()
            main {model.importPackage(0,null,null)}; ready()
            main {assertNotNull(model.pack); assertTrue(model.dirty); model.cancelImport()}
            assertArrayEquals(original,store.configFile.readBytes())
            main {owner.clear()}; owner=ViewModelStore(); create()
            main {
                assertNull(model.pack)
                assertTrue(model.dirty)
                assertEquals(changed,model.config!!.getJSONObject("basic").getBoolean("auto_commit_unique"))
                model.save()
            }
            ready()
            main {assertFalse(model.message,model.dirty)}
            val saved = store.readConfig()
            NativeHandle(saved.getLong("version")).close()
            assertEquals(changed,saved.getJSONObject("config").getJSONObject("basic").getBoolean("auto_commit_unique"))
        } finally {
            store.configFile.writeBytes(original)
            main {model.reload(true)}; ready()
            main {owner.clear()}
            invalid.delete()
            EngineUpdates.request(app)
        }
    }
}
