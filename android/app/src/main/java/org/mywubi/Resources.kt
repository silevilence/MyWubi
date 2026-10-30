package org.mywubi

import android.content.Context
import java.io.File

/** Initial resources are prepared off the UI thread and never overwrite user files. */
object Resources {
    fun prepare(context: Context): File {
        val root = File(context.filesDir, "engine").apply { mkdirs() }
        val tables = File(root, "tables").apply { mkdirs() }
        for (name in listOf("wubi86.dict", "pinyin.dict", "english.dict")) {
            val target = File(tables, name)
            if (!target.exists()) {
                val text = context.assets.open("tables/$name").bufferedReader().use { it.readText() }.replace("\r\n", "\n")
                val content = if (name == "wubi86.dict") text.replaceFirst("---\n", "---\ntemporary:\n  pinyin_table: pinyin.dict\n  english_table: english.dict\n  reverse_lookup: true\n") else text
                atomicCreate(target, content)
            }
        }
        return File(root, "config.toml").also {
            if (!it.exists()) atomicCreate(it, """
                [basic]
                auto_commit_unique = false
                [dictionary]
                system_table = "tables/wubi86.dict"
                user_table = "user.dict"
            """.trimIndent())
        }
    }

    private fun atomicCreate(target: File, text: String) {
        val temp = File.createTempFile("prepare-", ".tmp", target.parentFile)
        try {
            temp.outputStream().use { it.write(text.toByteArray(Charsets.UTF_8)); it.fd.sync() }
            check(temp.renameTo(target)) { "无法准备输入资源" }
        } finally { temp.delete() }
    }
}
