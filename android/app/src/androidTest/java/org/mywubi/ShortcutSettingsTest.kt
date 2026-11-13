package org.mywubi

import android.app.Application
import android.os.SystemClock
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File

/** Device/JNI acceptance: compiling this class is not evidence that it ran. */
class ShortcutSettingsTest {
    @Test fun rejectedSaveRestoresDraftAndCorrectedSaveCanonicalizesCode() {
        val ins = InstrumentationRegistry.getInstrumentation()
        val app = ins.targetContext.applicationContext as Application
        val store = EngineStore(app)
        val original = store.configFile.readBytes()
        val draftFile = File(app.filesDir, "settings-draft.json")
        val originalDraft = draftFile.takeIf { it.exists() }?.readBytes()
        var owner = ViewModelStore()
        lateinit var model: SettingsModel
        fun main(action: () -> Unit) = ins.runOnMainSync(action)
        fun ready() {
            val end = SystemClock.uptimeMillis() + 20000
            var busy = true
            while (busy && SystemClock.uptimeMillis() < end) {
                main { busy = model.busy }; SystemClock.sleep(25)
            }
            assertFalse("settings timed out", busy)
        }
        fun create() {
            main { model = ViewModelProvider(owner, ViewModelProvider.AndroidViewModelFactory(app))[SettingsModel::class.java] }
            ready()
        }
        fun rules(second: Int) = JSONArray().put(JSONObject().put("code", "JJAD").put("position", 2)
            .put("kind", "date_time").put("format", "yyyy年MM月dd日").put("enabled", true))
            .put(JSONObject().put("code", "jjad").put("position", second)
                .put("kind", "date_time").put("format", "yyyy-MM-dd").put("enabled", false))
        try {
            create(); main { model.reload(true) }; ready()
            main { model.change("shortcuts", "rules", rules(2)); model.save() }; ready()
            main {
                assertTrue(model.dirty)
                assertTrue(model.message, model.message.contains("position"))
                assertEquals("JJAD", model.config!!.getJSONObject("shortcuts").getJSONArray("rules").getJSONObject(0).getString("code"))
                owner.clear()
            }
            assertArrayEquals(original, store.configFile.readBytes())
            owner = ViewModelStore(); create()
            main {
                assertTrue("invalid draft survives model recreation", model.dirty)
                assertEquals(2, model.config!!.getJSONObject("shortcuts").getJSONArray("rules").getJSONObject(1).getInt("position"))
                model.change("shortcuts", "rules", rules(3)); model.save()
            }; ready()
            main {
                assertFalse(model.message, model.dirty)
                val saved = model.config!!.getJSONObject("shortcuts").getJSONArray("rules")
                assertEquals("jjad", saved.getJSONObject(0).getString("code"))
                assertEquals("jjad", saved.getJSONObject(1).getString("code"))
                assertFalse("disabled intent is preserved", saved.getJSONObject(1).getBoolean("enabled"))
            }
            val read = store.readConfig()
            NativeHandle(read.getLong("version")).use {
                val preview = NativeBridge.objectCall("PreviewShortcuts", "path" to store.configFile.path,
                    "config" to read.getJSONObject("config"), "code" to "JJAD", "time" to shortcutExampleTime())
                val rows = preview.getJSONArray("rows")
                val dates = (0 until rows.length()).map { rows.getJSONObject(it) }.filter { !it.isNull("target") }
                assertEquals(1, dates.size)
                assertEquals("2026年11月12日", dates.single().getJSONObject("candidate").getString("text"))
                assertEquals(2, dates.single().getInt("target"))
            }
        } finally {
            main { owner.clear() }
            store.configFile.writeBytes(original)
            if (originalDraft == null) draftFile.delete() else draftFile.writeBytes(originalDraft)
            EngineUpdates.request(app)
        }
    }
}
