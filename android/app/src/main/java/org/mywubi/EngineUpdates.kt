package org.mywubi

import android.content.Context
import java.util.UUID

/** Notifications contain no engine options; config.toml remains the only source. */
object EngineUpdates {
    fun preferences(context: Context) = context.getSharedPreferences("engine-updates", Context.MODE_PRIVATE)
    fun request(context: Context) {
        check(preferences(context).edit().putString("requested", UUID.randomUUID().toString()).commit())
    }
}

/** A late build never replaces a newer request, including after service destruction. */
class LoadGeneration {
    private var current = 0L
    fun next(): Long = ++current
    fun accepts(ticket: Long): Boolean = ticket == current
}
