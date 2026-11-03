package org.mywubi

/** Empty composition alone is insufficient: direct input and held keys are also busy. */
internal class KeyboardIdle(private val quietMillis: Long = 1800) {
    private var lastActivity = 0L
    private var touching = false

    fun activity(now: Long) { lastActivity = now }
    fun touch(pressed: Boolean, now: Long) { touching = pressed; activity(now) }
    fun isIdle(now: Long, spelling: String, hasCandidates: Boolean, ownsPreedit: Boolean, temporary: Boolean): Boolean =
        !touching && now - lastActivity >= quietMillis && spelling.isEmpty() && !hasCandidates && !ownsPreedit && !temporary
}
