package fr.jolannrt.aetherplay.ui

import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathOperation
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.translate
import fr.jolannrt.aetherplay.pad.Body
import fr.jolannrt.aetherplay.pad.BodyShape

/**
 * Stylised controller body drawn behind the buttons, so each console's
 * controller is recognisable (shape and colours, AetherPlay's own drawing).
 * Coordinates: fractions of the width (x) and height (y).
 */
fun DrawScope.drawBody(body: Body) {
    val w = size.width
    val h = size.height
    fun rect(l: Float, t: Float, r: Float, b: Float, radius: Float) =
        Path().apply { addRoundRect(RoundRect(Rect(l * w, t * h, r * w, b * h), CornerRadius(radius * h))) }
    fun circle(x: Float, y: Float, r: Float) =
        Path().apply { addOval(Rect(Offset(x * w, y * h), r * h)) }
    fun union(vararg parts: Path) = parts.reduce { acc, p -> Path.combine(PathOperation.Union, acc, p) }

    val main = Color(body.main)
    val second = Color(body.second)
    val accent = Color(body.accent)
    val shadow = Color.Black.copy(alpha = 0.35f)
    val rim = Color.White.copy(alpha = 0.10f)

    fun shape(p: Path, color: Color) {
        translate(top = 0.015f * h) { drawPath(p, shadow) }
        drawPath(p, color)
        drawPath(p, rim, style = Stroke(3f))
    }

    when (body.shape) {
        BodyShape.NONE -> {}

        // Super Nintendo, Mega Drive, Saturn: two round grips joined by a band.
        BodyShape.DOGBONE -> {
            val outline = union(circle(0.2f, 0.6f, 0.38f), circle(0.8f, 0.6f, 0.38f), rect(0.2f, 0.3f, 0.8f, 0.94f, 0.2f))
            shape(outline, main)
            drawPath(rect(0.36f, 0.4f, 0.64f, 0.8f, 0.12f), second)
            drawPath(circle(0.84f, 0.56f, 0.3f), accent)
            drawPath(circle(0.17f, 0.58f, 0.24f), second.copy(alpha = 0.6f))
        }

        // NES, Master System, PC Engine: a flat rectangle with an inset panel.
        BodyShape.BRICK -> {
            shape(rect(0.03f, 0.2f, 0.97f, 0.98f, 0.08f), main)
            drawPath(rect(0.06f, 0.3f, 0.94f, 0.93f, 0.05f), second)
            for (i in 0..2) {
                drawRect(accent, Offset(0.42f * w, (0.42f + i * 0.08f) * h), Size(0.16f * w, 0.025f * h))
            }
        }

        // Nintendo 64: a bar with three grips.
        BodyShape.TRIDENT -> {
            val outline = union(
                rect(0.03f, 0.18f, 0.97f, 0.6f, 0.2f),
                rect(0.03f, 0.4f, 0.16f, 0.98f, 0.07f),
                rect(0.18f, 0.4f, 0.33f, 0.98f, 0.07f),
                rect(0.64f, 0.4f, 0.97f, 0.98f, 0.12f),
            )
            shape(outline, main)
            drawPath(rect(0.4f, 0.26f, 0.6f, 0.5f, 0.1f), second)
        }

        // PlayStation, Xbox, GameCube, Dreamcast: wide body and two handles.
        BodyShape.HANDLES -> {
            val outline = union(
                rect(0.05f, 0.18f, 0.95f, 0.66f, 0.24f),
                circle(0.19f, 0.7f, 0.29f),
                circle(0.81f, 0.7f, 0.29f),
                rect(0.28f, 0.45f, 0.72f, 0.92f, 0.18f),
            )
            shape(outline, main)
            drawPath(rect(0.4f, 0.26f, 0.6f, 0.56f, 0.1f), second)
        }

        // Switch: two Joy-Con around the console's screen.
        BodyShape.JOYCONS -> {
            drawPath(rect(0.38f, 0.22f, 0.62f, 0.94f, 0.04f), Color(0xFF15161A))
            drawPath(rect(0.4f, 0.27f, 0.6f, 0.89f, 0.02f), Color(0xFF26282E))
            shape(rect(0.02f, 0.17f, 0.4f, 0.98f, 0.3f), main)
            shape(rect(0.6f, 0.17f, 0.98f, 0.98f, 0.3f), second)
            drawRect(accent, Offset(0.385f * w, 0.25f * h), Size(0.012f * w, 0.66f * h))
            drawRect(accent, Offset(0.603f * w, 0.25f * h), Size(0.012f * w, 0.66f * h))
        }

        // Game Boy, DS, PSP, Wii U GamePad: a handheld with its screen.
        BodyShape.HANDHELD -> {
            shape(rect(0.02f, 0.17f, 0.98f, 0.98f, 0.3f), main)
            drawPath(rect(0.37f, 0.24f, 0.63f, 0.8f, 0.06f), Color(0xFF15161A))
            drawPath(rect(0.39f, 0.29f, 0.61f, 0.75f, 0.03f), second)
        }

        // Arcade stick, Atari: a flat control panel.
        BodyShape.PANEL -> {
            shape(rect(0.02f, 0.2f, 0.98f, 0.98f, 0.04f), main)
            drawRect(accent, Offset(0.02f * w, 0.24f * h), Size(0.96f * w, 0.03f * h))
            drawPath(rect(0.04f, 0.32f, 0.96f, 0.94f, 0.03f), second)
        }
    }
}
