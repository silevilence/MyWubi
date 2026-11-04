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

    @Test fun auxiliaryChoicesSaveRelativeReferencesAndPreserveHiddenDesktopKey() {
        val app = instrumentation.targetContext.applicationContext as Application
        val owner = ViewModelStore()
        lateinit var model: LibraryModel
        main { model = ViewModelProvider(owner, ViewModelProvider.AndroidViewModelFactory(app))[LibraryModel::class.java] }
        ready(model); main { model.open(0, true) }; ready(model)
        val file = model.paths[0].second
        val original = file.readBytes()
        try {
            val key = model.form.getJSONObject("header").getJSONObject("temporary").getString("key")
            val choices = model.referenceChoices()
            assertTrue(choices.any { it.first == "不引用" && it.second.isEmpty() })
            assertTrue(choices.none { it.second.isNotEmpty() && java.io.File(file.parentFile, it.second).canonicalFile == file.canonicalFile })
            main {
                model.change("display_name", "自定义五笔", "header")
                model.change("pinyin_table", choices.first { it.first == "内置拼音" }.second, "temporary")
                model.change("english_table", choices.first { it.first == "内置英文" }.second, "temporary")
                model.save()
            }
            ready(model)
            main {
                assertFalse(model.message, model.dirty)
                assertEquals("自定义五笔", model.form.getJSONObject("header").getString("display_name"))
                assertEquals("当前码表：自定义五笔", model.paths[model.selected].first)
                val temporary = model.form.getJSONObject("header").getJSONObject("temporary")
                assertEquals(key, temporary.getString("key"))
                assertEquals("内置拼音", model.referenceLabel(temporary.getString("pinyin_table")))
                assertEquals("内置英文", model.referenceLabel(temporary.getString("english_table")))
            }
        } finally {
            file.writeBytes(original)
            main { model.open(0, true) }; ready(model)
            main { owner.clear() }; EngineUpdates.request(app)
        }
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
