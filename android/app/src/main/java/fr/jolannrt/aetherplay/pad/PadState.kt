package fr.jolannrt.aetherplay.pad

/**
 * Controller state in the W3C "standard gamepad" order, the order AetherPlay
 * (RomM's interface) and the Bluetooth report both use:
 * 0 bottom face, 1 right face, 2 left face, 3 top face, 4 LB, 5 RB, 6 LT, 7 RT,
 * 8 select, 9 start, 10 L3, 11 R3, 12-15 D-pad up/down/left/right, 16 home.
 * Face buttons are positional: "bottom" is Xbox A, Sony Cross, Nintendo B.
 */
object Btn {
    const val SOUTH = 0
    const val EAST = 1
    const val WEST = 2
    const val NORTH = 3
    const val LB = 4
    const val RB = 5
    const val LT = 6
    const val RT = 7
    const val SELECT = 8
    const val START = 9
    const val L3 = 10
    const val R3 = 11
    const val UP = 12
    const val DOWN = 13
    const val LEFT = 14
    const val RIGHT = 15
    const val HOME = 16
    const val COUNT = 17
}

/** Left X, left Y, right X, right Y in -1..1 (Y down positive, as W3C). */
data class PadState(val buttons: Int = 0, val axes: List<Float> = listOf(0f, 0f, 0f, 0f)) {
    fun pressed(button: Int) = buttons and (1 shl button) != 0

    /** Bluetooth HID input report (see HidGamepad.DESCRIPTOR): 3 bytes of buttons, 4 signed axes. */
    fun hidReport(): ByteArray {
        val r = ByteArray(7)
        r[0] = (buttons and 0xFF).toByte()
        r[1] = ((buttons shr 8) and 0xFF).toByte()
        r[2] = ((buttons shr 16) and 0x01).toByte()
        for (i in 0 until 4) r[3 + i] = (axes[i].coerceIn(-1f, 1f) * 127f).toInt().toByte()
        return r
    }

    companion object {
        val IDLE = PadState()
    }
}
