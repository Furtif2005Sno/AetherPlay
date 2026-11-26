package fr.jolannrt.aetherplay.pad

/**
 * On-screen controller layouts, one per console. Positions are fractions of
 * the screen (x of the width, y and radius of the height). Each control sends
 * the standard button the emulators bind that console's button to (RetroArch
 * RetroPad, Eden, Dolphin…), so the original label sits where it should.
 */
sealed interface Control {
    data class Button(
        val id: Int,
        val label: String,
        val x: Float,
        val y: Float,
        val r: Float = 0.075f,
        val fill: Long = 0xFF2C2C36,
        val text: Long = 0xFFFFFFFF,
        val pill: Boolean = false,
        /** Instead of a button: push this axis to [axisValue] (N64 C buttons). */
        val axis: Int = -1,
        val axisValue: Float = 0f,
    ) : Control

    data class Dpad(val x: Float, val y: Float, val r: Float = 0.17f) : Control

    /** [axis] is 0 for the left stick (axes 0/1), 2 for the right one (2/3). */
    data class Stick(val axis: Int, val x: Float, val y: Float, val r: Float = 0.16f, val tint: Long = 0xFF3A3A46) : Control
}

data class PadLayout(
    val title: String,
    val background: Long,
    val accent: Long,
    val controls: List<Control>,
)

/** Face buttons in standard order: bottom, right, left, top. */
private data class Face(val labels: List<String>, val fills: List<Long>, val texts: List<Long>)

private const val DARK = 0xFF2C2C36
private const val WHITE = 0xFFFFFFFF

private val XBOX = Face(
    listOf("A", "B", "X", "Y"),
    listOf(0xFF2E9D4F, 0xFFD64545, 0xFF2F7FE0, 0xFFE0B21F),
    List(4) { WHITE },
)
private val NINTENDO = Face(listOf("B", "A", "Y", "X"), List(4) { DARK }, List(4) { WHITE })
private val SNES = Face(
    listOf("B", "A", "Y", "X"),
    listOf(0xFFE2B93B, 0xFFCF4A45, 0xFF3F9E4D, 0xFF3D63C9),
    List(4) { WHITE },
)
private val SONY = Face(
    listOf("✕", "○", "□", "△"),
    List(4) { DARK },
    listOf(0xFF8FA8F5, 0xFFF0707A, 0xFFE68FD0, 0xFF52D3A8),
)
private val DREAMCAST = Face(
    listOf("A", "B", "X", "Y"),
    listOf(0xFFD9443F, 0xFF3C6FD1, 0xFFE8B930, 0xFF3CA55C),
    List(4) { WHITE },
)

private fun face(f: Face, x: Float = 0.85f, y: Float = 0.44f, gap: Float = 0.085f, r: Float = 0.072f) = listOf(
    Control.Button(Btn.SOUTH, f.labels[0], x, y + gap * 1.6f, r, f.fills[0], f.texts[0]),
    Control.Button(Btn.EAST, f.labels[1], x + gap, y, r, f.fills[1], f.texts[1]),
    Control.Button(Btn.WEST, f.labels[2], x - gap, y, r, f.fills[2], f.texts[2]),
    Control.Button(Btn.NORTH, f.labels[3], x, y - gap * 1.6f, r, f.fills[3], f.texts[3]),
)

private fun pill(id: Int, label: String, x: Float, y: Float, fill: Long = DARK) =
    Control.Button(id, label, x, y, 0.05f, fill, WHITE, pill = true)

private fun shoulders(lb: String?, rb: String?, lt: String? = null, rt: String? = null) = listOfNotNull(
    lt?.let { pill(Btn.LT, it, 0.07f, 0.09f) },
    lb?.let { pill(Btn.LB, it, if (lt != null) 0.2f else 0.09f, 0.09f) },
    rb?.let { pill(Btn.RB, it, if (rt != null) 0.8f else 0.91f, 0.09f) },
    rt?.let { pill(Btn.RT, it, 0.93f, 0.09f) },
)

private fun center(select: String?, start: String?, home: String? = null) = listOfNotNull(
    select?.let { pill(Btn.SELECT, it, 0.4f, 0.9f) },
    home?.let { Control.Button(Btn.HOME, it, 0.5f, 0.9f, 0.045f) },
    start?.let { pill(Btn.START, it, 0.6f, 0.9f) },
)

/** Two sticks, the left one above the D-pad (Xbox / Switch style). */
private fun sticksHigh(stickTint: Long = 0xFF3A3A46) = listOf(
    Control.Stick(0, 0.15f, 0.42f, tint = stickTint),
    Control.Dpad(0.31f, 0.78f, 0.14f),
    Control.Stick(2, 0.68f, 0.78f, 0.14f, stickTint),
)

/** Two sticks below, D-pad above (PlayStation style). */
private fun sticksLow() = listOf(
    Control.Dpad(0.15f, 0.42f),
    Control.Stick(0, 0.33f, 0.78f, 0.14f),
    Control.Stick(2, 0.67f, 0.78f, 0.14f),
)

private val DPAD = Control.Dpad(0.17f, 0.58f)

private fun xboxLike(title: String, bg: Long = 0xFF15181D, accent: Long = 0xFF2E9D4F) = PadLayout(
    title, bg, accent,
    sticksHigh() + face(XBOX) + shoulders("LB", "RB", "LT", "RT") + center("◧", "≡", "⌂"),
)

private fun sony(title: String, home: Boolean, bg: Long = 0xFF1B1F2B) = PadLayout(
    title, bg, 0xFF8FA8F5,
    sticksLow() + face(SONY) + shoulders("L1", "R1", "L2", "R2") + center("SELECT", "START", if (home) "PS" else null),
)

private fun twoButtons(title: String, bg: Long, accent: Long, b: String, a: String, fill: Long, l: String? = null, r: String? = null) =
    PadLayout(
        title, bg, accent,
        listOf(
            DPAD,
            Control.Button(Btn.SOUTH, b, 0.76f, 0.64f, 0.085f, fill),
            Control.Button(Btn.EAST, a, 0.9f, 0.52f, 0.085f, fill),
        ) + shoulders(l, r) + center("SELECT", "START"),
    )

/** Controller for the AetherPlay menus (RomM's interface): A confirms, B goes back. */
val MENU = PadLayout(
    "AetherPlay", 0xFF0D0B14, 0xFF8B5CF6,
    listOf(DPAD, Control.Stick(2, 0.68f, 0.78f, 0.12f)) + face(XBOX) + shoulders("LB", "RB") + center("◧", "≡"),
)

fun layoutFor(platform: String?): PadLayout = when (platform) {
    null, "" -> MENU
    "nes", "famicom" -> twoButtons("NES", 0xFF26262B, 0xFFD13B3B, "B", "A", 0xFFC83737)
    "snes", "sfam" -> PadLayout(
        "Super Nintendo", 0xFF2A2735, 0xFF7B68D9,
        listOf(DPAD) + face(SNES) + shoulders("L", "R") + center("SELECT", "START"),
    )
    "n64" -> PadLayout(
        "Nintendo 64", 0xFF1E2028, 0xFF2F6FDB,
        listOf(
            Control.Dpad(0.1f, 0.34f, 0.11f),
            Control.Stick(0, 0.25f, 0.66f),
            Control.Button(Btn.SOUTH, "A", 0.8f, 0.72f, 0.09f, 0xFF2F6FDB),
            Control.Button(Btn.WEST, "B", 0.69f, 0.58f, 0.075f, 0xFF2E9D4F),
            Control.Button(-1, "▲", 0.87f, 0.26f, 0.055f, 0xFFE2B93B, 0xFF1E2028, axis = 3, axisValue = -1f),
            Control.Button(-1, "▼", 0.87f, 0.48f, 0.055f, 0xFFE2B93B, 0xFF1E2028, axis = 3, axisValue = 1f),
            Control.Button(-1, "◀", 0.8f, 0.37f, 0.055f, 0xFFE2B93B, 0xFF1E2028, axis = 2, axisValue = -1f),
            Control.Button(-1, "▶", 0.94f, 0.37f, 0.055f, 0xFFE2B93B, 0xFF1E2028, axis = 2, axisValue = 1f),
            pill(Btn.LT, "Z", 0.36f, 0.3f),
        ) + shoulders("L", "R") + center(null, "START"),
    )
    "gb" -> twoButtons("Game Boy", 0xFF2B2E26, 0xFF9BBC0F, "B", "A", 0xFF8E2A5C)
    "gbc" -> twoButtons("Game Boy Color", 0xFF2A2338, 0xFF8E5CD9, "B", "A", 0xFF8E2A5C)
    "gba" -> twoButtons("Game Boy Advance", 0xFF27234A, 0xFF5B54C9, "B", "A", 0xFF3A3480, "L", "R")
    "nds" -> PadLayout(
        "Nintendo DS", 0xFF22242A, 0xFFB8BCC6,
        listOf(DPAD) + face(NINTENDO) + shoulders("L", "R") + center("SELECT", "START"),
    )
    "3ds", "n3ds" -> PadLayout(
        "Nintendo 3DS", 0xFF2A1F22, 0xFFD6404B,
        listOf(Control.Stick(0, 0.15f, 0.4f, 0.14f), Control.Dpad(0.17f, 0.78f, 0.13f)) +
            face(NINTENDO) + shoulders("L", "R", "ZL", "ZR") + center("SELECT", "START", "⌂"),
    )
    "switch" -> PadLayout(
        "Nintendo Switch", 0xFF1A1A20, 0xFFFF4554,
        sticksHigh() + face(NINTENDO) + shoulders("L", "R", "ZL", "ZR") + center("−", "+", "⌂"),
    )
    "switch-2" -> PadLayout(
        "Nintendo Switch 2", 0xFF16181F, 0xFF4FA3FF,
        sticksHigh() + face(NINTENDO) + shoulders("L", "R", "ZL", "ZR") + center("−", "+", "⌂"),
    )
    "wiiu" -> PadLayout(
        "Wii U", 0xFF1B2228, 0xFF1FA7D9,
        sticksHigh() + face(NINTENDO) + shoulders("L", "R", "ZL", "ZR") + center("−", "+", "⌂"),
    )
    "ngc", "wii" -> PadLayout(
        if (platform == "ngc") "GameCube" else "Wii", 0xFF2E2655, 0xFF7E6BE0,
        listOf(
            Control.Stick(0, 0.15f, 0.42f),
            Control.Dpad(0.3f, 0.8f, 0.12f),
            Control.Stick(2, 0.68f, 0.8f, 0.12f, 0xFFB89A1E),
            Control.Button(Btn.SOUTH, "A", 0.83f, 0.52f, 0.1f, 0xFF2E9D4F),
            Control.Button(Btn.WEST, "B", 0.71f, 0.62f, 0.065f, 0xFFD64545),
            Control.Button(Btn.EAST, "X", 0.95f, 0.4f, 0.065f, 0xFF8A8A96),
            Control.Button(Btn.NORTH, "Y", 0.8f, 0.3f, 0.065f, 0xFF8A8A96),
        ) + shoulders(null, "Z", "L", "R") + center(null, "START"),
    )
    "genesis-slash-megadrive" -> PadLayout(
        "Mega Drive", 0xFF151515, 0xFFD32F2F,
        listOf(
            DPAD,
            Control.Button(Btn.WEST, "A", 0.7f, 0.66f, 0.07f),
            Control.Button(Btn.SOUTH, "B", 0.81f, 0.6f, 0.07f),
            Control.Button(Btn.EAST, "C", 0.92f, 0.54f, 0.07f),
        ) + center(null, "START"),
    )
    "sms", "gamegear" -> twoButtons(if (platform == "sms") "Master System" else "Game Gear", 0xFF181818, 0xFFD32F2F, "1", "2", DARK)
    "turbografx16--1" -> twoButtons("PC Engine", 0xFF1C1C22, 0xFFE07B1F, "II", "I", DARK)
    "saturn" -> PadLayout(
        "Saturn", 0xFF24242C, 0xFF9AA0B5,
        listOf(
            DPAD,
            Control.Button(Btn.SOUTH, "A", 0.7f, 0.66f, 0.065f),
            Control.Button(Btn.EAST, "B", 0.81f, 0.6f, 0.065f),
            Control.Button(Btn.RB, "C", 0.92f, 0.54f, 0.065f),
            Control.Button(Btn.WEST, "X", 0.7f, 0.46f, 0.055f),
            Control.Button(Btn.NORTH, "Y", 0.81f, 0.4f, 0.055f),
            Control.Button(Btn.LB, "Z", 0.92f, 0.34f, 0.055f),
        ) + shoulders(null, null, "L", "R") + center(null, "START"),
    )
    "dc" -> PadLayout(
        "Dreamcast", 0xFF1F2830, 0xFFF57C00,
        sticksHigh().take(2) + face(DREAMCAST) + shoulders(null, null, "L", "R") + center(null, "START"),
    )
    "neogeoaes", "arcade" -> PadLayout(
        if (platform == "arcade") "Arcade" else "Neo Geo", 0xFF141414, 0xFFE53935,
        listOf(
            Control.Stick(0, 0.2f, 0.58f, 0.18f),
            Control.Button(Btn.SOUTH, "A", 0.66f, 0.64f, 0.075f, 0xFFD64545),
            Control.Button(Btn.EAST, "B", 0.78f, 0.56f, 0.075f, 0xFFE0B21F),
            Control.Button(Btn.WEST, "C", 0.88f, 0.44f, 0.075f, 0xFF2E9D4F),
            Control.Button(Btn.NORTH, "D", 0.95f, 0.28f, 0.075f, 0xFF2F7FE0),
        ) + center("COIN", "START"),
    )
    "atari2600" -> PadLayout(
        "Atari 2600", 0xFF2E2118, 0xFFE0662F,
        listOf(Control.Stick(0, 0.2f, 0.58f, 0.18f), Control.Button(Btn.SOUTH, "FIRE", 0.82f, 0.58f, 0.11f, 0xFFC62828)) +
            center("SELECT", "RESET"),
    )
    "psx" -> sony("PlayStation", home = false)
    "ps2" -> sony("PlayStation 2", home = false)
    "ps3" -> sony("PlayStation 3", home = true)
    "ps4--1" -> sony("PlayStation 4", home = true, bg = 0xFF141B2B)
    "psvita" -> PadLayout(
        "PS Vita", 0xFF181C26, 0xFF8FA8F5,
        sticksLow() + face(SONY) + shoulders("L", "R") + center("SELECT", "START", "PS"),
    )
    "psp" -> PadLayout(
        "PSP", 0xFF1A1A1F, 0xFF8FA8F5,
        listOf(Control.Dpad(0.15f, 0.42f), Control.Stick(0, 0.16f, 0.82f, 0.11f)) + face(SONY) +
            shoulders("L", "R") + center("SELECT", "START", "⌂"),
    )
    "xbox" -> xboxLike("Xbox", 0xFF121A12)
    "xbox360" -> xboxLike("Xbox 360", 0xFF15191D)
    else -> xboxLike("Manette")
}
