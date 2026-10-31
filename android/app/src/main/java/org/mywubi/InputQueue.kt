package org.mywubi

/** Main-thread FIFO. Reentrant input waits; context changes invalidate queued actions. */
class InputQueue(private val capacity: Int = 128) {
    private val actions = ArrayDeque<Pair<Long, () -> Unit>>()
    private var draining = false
    var generation = 0L
        private set

    fun invalidate() { generation++; actions.clear() }

    fun submit(action: () -> Unit): Boolean {
        if (actions.size >= capacity) return false
        actions.addLast(generation to action)
        if (!draining) {
            draining = true
            try {
                while (actions.isNotEmpty()) {
                    val (epoch, next) = actions.removeFirst()
                    if (epoch == generation) next()
                }
            } finally { draining = false }
        }
        return true
    }
}
