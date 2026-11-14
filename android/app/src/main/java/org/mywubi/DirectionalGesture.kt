package org.mywubi

import kotlin.math.*

/** Release-only ordinary-key state machine. Delete repetition uses its own recognizer. */
internal class DirectionalGesture(private val bound: Set<Int>, private val canHold: Boolean) {
    private enum class Phase { Tracking, Sliding, Holding, Cancelled, Finished }
    private var phase=Phase.Tracking
    var direction: Int? = null
        private set
    val sliding: Boolean get() = phase==Phase.Sliding
    val holding: Boolean get() = phase==Phase.Holding
    val cancelled: Boolean get() = phase==Phase.Cancelled
    fun hold() { if(phase==Phase.Tracking && canHold) phase=Phase.Holding }
    fun cancel() { phase=Phase.Cancelled; direction=null }
    fun move(dx: Float, dy: Float) {
        if(phase==Phase.Cancelled || phase==Phase.Finished) return
        val distance=hypot(dx,dy)
        if(!distance.isFinite() || distance>160f) { cancel();return }
        if(phase==Phase.Holding) return
        if(phase==Phase.Tracking && distance<24f) return
        if(distance<12f) { cancel();return }
        phase=Phase.Sliding
        val angle=((atan2(dx,-dy)*180f/PI.toFloat())+360f)%360f
        val previous=direction
        if(previous!=null) {
            val delta=abs(((angle-previous*45f+540f)%360f)-180f)
            if(delta<27.5f) return
        }
        direction=(floor((angle+22.5f)/45f).toInt())%8
    }
    /** -1 is click, -2 is hold; unbound/cancelled directions return no action. */
    fun release(): Int? {
        val result=when(phase) {
            Phase.Tracking -> -1
            Phase.Holding -> -2
            Phase.Sliding -> direction?.takeIf { it in bound }
            else -> null
        }
        phase=Phase.Finished
        return result
    }
}

internal data class TouchChoice(val name: String, val action: () -> Unit)
internal val directionNames=listOf("上","右上","右","右下","下","左下","左","左上")
internal val directionIds=listOf("up","up_right","right","down_right","down","down_left","left","up_left")
