package org.mywubi

import android.content.Context
import java.util.UUID

/** Notifications contain no engine options; config.toml remains the only source. */
object EngineUpdates {
    fun preferences(context: Context) = context.getSharedPreferences("engine-updates", Context.MODE_PRIVATE)
    fun request(context: Context) {
        // The configuration is already durable. Notification failure must not
        // turn a successful save into an apparent failed transaction.
        val preferences = preferences(context)
        if (!preferences.edit().putString("requested", UUID.randomUUID().toString()).commit()) {
            preferences.edit().putString("error", "配置已保存，但重载通知未持久化；请重新进入输入框或重启输入法").apply()
        }
    }
}

/** A late build never replaces a newer request, including after service destruction. */
class LoadGeneration {
    private var current = 0L
    fun next(): Long = ++current
    fun accepts(ticket: Long): Boolean = ticket == current
}
