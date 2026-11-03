package org.mywubi

/** Touch mode cycle shared by Shift and the Android pinyin latch. */
enum class KeyMode {
    Off, Once, Locked;

    fun next() = when (this) { Off -> Once; Once -> Locked; Locked -> Off }
    fun consumed() = if (this == Once) Off else this
}
