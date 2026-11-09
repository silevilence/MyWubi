package org.mywubi

import android.content.Context
import java.io.File

/** Initial resources are prepared off the UI thread and never overwrite user files. */
object Resources {
    @Synchronized fun prepare(context: Context): File {
        val root = File(context.filesDir, "engine").apply { mkdirs() }
        val tables = File(root, "tables").apply { mkdirs() }
        val expressions = File(root, "expressions").apply { mkdirs() }
        for (name in listOf("emoji.mwe", "kaomoji.mwe")) {
            val target = File(expressions, name)
            if (!target.exists()) {
                // Keep existing files on upgrades, including user-maintained resources.
                runCatching {
                    val text = context.assets.open("expressions/$name").bufferedReader(Charsets.UTF_8).use { it.readText() }
                    atomicCreate(target, text)
                }.onFailure { android.util.Log.w("MyWubi", "内置表情词库准备失败：$name", it) }
            }
        }
        for (name in listOf("wubi86.dict", "pinyin.dict", "english.dict")) {
            val target = File(tables, name)
            if (!target.exists()) {
                val text = context.assets.open("tables/$name").bufferedReader().use { it.readText() }.replace("\r\n", "\n")
                // The shared table already declares its auxiliary dictionaries.
                atomicCreate(target, text)
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
