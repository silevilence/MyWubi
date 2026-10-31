package org.mywubi

import android.app.Application
import android.os.SystemClock
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Test

class LibraryModelTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private fun main(action: () -> Unit) = instrumentation.runOnMainSync(action)
    private fun ready(model: LibraryModel) {
        val deadline = SystemClock.uptimeMillis() + 15000
        var busy = true
        while (busy && SystemClock.uptimeMillis() < deadline) {
            main { busy = model.busy }; SystemClock.sleep(25)
        }
        assertFalse("operation timed out", busy)
    }

    @Test fun invalidSaveRetainsDraftAndConflictCannotOverwriteDisk() {
        val app = instrumentation.targetContext.applicationContext as Application
        var owner = ViewModelStore()
        lateinit var model: LibraryModel
        fun create() {
            main { model = ViewModelProvider(owner, ViewModelProvider.AndroidViewModelFactory(app))[LibraryModel::class.java] }
            ready(model); main { model.open(0) }; ready(model)
        }
        create()
        val file = model.paths[0].second
        val original = file.readBytes()
        try {
            main { model.change("max_code_len", "invalid", "header"); model.save() }
            ready(model)
            main { assertTrue(model.dirty); assertTrue(model.message.contains("整数")) }
            assertArrayEquals(original, file.readBytes())
            main { owner.clear() }; owner = ViewModelStore(); create()
            main {
                assertTrue(model.dirty)
                assertEquals("invalid", model.form.getJSONObject("header").getString("max_code_len"))
                model.open(0, true)
            }
            ready(model)
            main { model.change("code", "ab"); model.change("word", "草稿验收😀"); model.save() }
            ready(model)
            main { assertFalse(model.message, model.dirty) }
            assertTrue(file.readText().contains("草稿验收😀"))
            main { model.change("word", "冲突草稿") }
            file.appendText("\n# external revision\n")
            val external = file.readBytes()
            main { model.save() }; ready(model)
            main { assertTrue(model.dirty); assertTrue(model.message.contains("版本冲突")) }
            assertArrayEquals(external, file.readBytes())
        } finally {
            file.writeBytes(original)
            main { model.open(0, true) }; ready(model)
            main { owner.clear() }
            EngineUpdates.request(app)
        }
    }
}
