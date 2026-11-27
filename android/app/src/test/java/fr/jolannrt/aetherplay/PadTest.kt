package fr.jolannrt.aetherplay

import fr.jolannrt.aetherplay.pad.Btn
import fr.jolannrt.aetherplay.pad.Control
import fr.jolannrt.aetherplay.pad.PadState
import fr.jolannrt.aetherplay.pad.layoutFor
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PadTest {
    @Test
    fun releaseVersionsCompareNumerically() {
        assertTrue(AppUpdate.newer("0.3.10", "0.3.9"))
        assertTrue(AppUpdate.newer("0.4.0", "0.3.2"))
        assertTrue(!AppUpdate.newer("0.3.2", "0.3.2"))
        assertTrue(!AppUpdate.newer("0.3.1", "0.3.2"))
    }

    @Test
    fun hidReportPacksButtonsThenAxes() {
        val state = PadState((1 shl Btn.SOUTH) or (1 shl Btn.START) or (1 shl Btn.HOME), listOf(1f, -1f, 0f, 0.5f))
        val r = state.hidReport()
        assertEquals(7, r.size)
        assertEquals(0x01, r[0].toInt() and 0xFF) // bit 0
        assertEquals(0x02, r[1].toInt() and 0xFF) // start = bit 9
        assertEquals(0x01, r[2].toInt()) //          home = bit 16
        assertEquals(127, r[3].toInt())
        assertEquals(-127, r[4].toInt())
        assertEquals(0, r[5].toInt())
        assertEquals(63, r[6].toInt())
    }

    @Test
    fun everyLayoutStaysOnScreenWithValidButtons() {
        val platforms = listOf(
            null, "nes", "snes", "n64", "gb", "gbc", "gba", "nds", "3ds", "switch", "switch-2", "wiiu", "ngc", "wii",
            "genesis-slash-megadrive", "sms", "gamegear", "turbografx16--1", "saturn", "dc", "neogeoaes", "arcade",
            "atari2600", "psx", "ps2", "ps3", "ps4--1", "psvita", "psp", "xbox", "xbox360", "unknown",
        )
        for (p in platforms) {
            val layout = layoutFor(p)
            assertTrue("$p has controls", layout.controls.isNotEmpty())
            for (c in layout.controls) {
                val (x, y) = when (c) {
                    is Control.Button -> {
                        assertTrue("$p ${c.label}", c.axis >= 0 || c.id in 0 until Btn.COUNT)
                        c.x to c.y
                    }
                    is Control.Dpad -> c.x to c.y
                    is Control.Stick -> c.x to c.y
                }
                assertTrue("$p x=$x", x in 0f..1f)
                assertTrue("$p y=$y", y in 0f..1f)
            }
        }
    }
}
