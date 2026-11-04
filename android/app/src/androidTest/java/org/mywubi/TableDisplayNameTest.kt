package org.mywubi

import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Test
import java.io.File

class TableDisplayNameTest {
    @Test fun catalogRereadsHeaderAndKeepsOriginalFilenameForBlankNames() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val root = File(Resources.prepare(context).parentFile, "packages/display-name-${java.util.UUID.randomUUID()}")
        check(root.mkdirs())
        val file = File(root, "0.dict")
        val catalog = TableCatalog(context)
        try {
            file.writeText("---\ndisplay_name: '我的五笔 #1'\n---\na\t工\n")
            catalog.record(EngineStore.ImportSet(root,
                listOf(EngineStore.ImportFile("我的五笔 #1", file, "original.dict"))), 0)
            assertEquals("我的五笔 #1", catalog.label(file.path))
            file.writeText("---\ndisplay_name: 更新名称\n---\na\t工\n")
            assertEquals("更新名称", catalog.label(file.path))
            file.writeText("---\ndisplay_name: '   '\n---\na\t工\n")
            assertEquals("original.dict", catalog.label(file.path))
            file.writeText("a\t工\n")
            assertEquals("original.dict", catalog.label(file.path))
        } finally { root.deleteRecursively() }
    }
}
