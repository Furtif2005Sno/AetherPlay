package fr.jolannrt.aetherplay.ui

import android.graphics.Paint
import android.graphics.Typeface
import android.view.HapticFeedbackConstants
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Bluetooth
import androidx.compose.material.icons.filled.BluetoothDisabled
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Tune
import androidx.compose.material.icons.filled.Wifi
import androidx.compose.material.icons.filled.WifiOff
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.input.pointer.PointerId
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import fr.jolannrt.aetherplay.net.Game
import fr.jolannrt.aetherplay.pad.Btn
import fr.jolannrt.aetherplay.pad.Control
import fr.jolannrt.aetherplay.pad.PadLayout
import fr.jolannrt.aetherplay.pad.PadState
import fr.jolannrt.aetherplay.pad.Tweak
import fr.jolannrt.aetherplay.pad.tweaked
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min

@Composable
fun PadScreen(
    layout: PadLayout,
    game: Game?,
    bluetooth: Boolean,
    wifi: Boolean,
    onState: (PadState) -> Unit,
    onQuitGame: () -> Unit,
    onSettings: () -> Unit,
    player: Int = 0,
    tweaks: Map<Int, Tweak> = emptyMap(),
    onTweaks: (Map<Int, Tweak>) -> Unit = {},
) {
    val accent = Color(layout.accent)
    var editing by remember { mutableStateOf(false) }
    var selected by remember(layout) { mutableStateOf<Int?>(null) }
    Box(
        Modifier
            .fillMaxSize()
            .background(Color(layout.background)),
    ) {
        game?.cover?.let {
            Image(
                it.asImageBitmap(), null,
                Modifier.fillMaxSize(),
                contentScale = ContentScale.Crop,
                alpha = 0.18f,
            )
        }
        if (editing) {
            EditPad(layout, tweaks, selected, { selected = it }, onTweaks)
        } else {
            Pad(layout.tweaked(tweaks), onState)
        }
        Row(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 8.dp, vertical = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Spacer(Modifier.width(260.dp))
            if (player > 0) {
                Text(
                    "J$player",
                    color = Color.Black,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.Bold,
                    modifier = Modifier
                        .padding(end = 8.dp)
                        .background(accent, RoundedCornerShape(50))
                        .padding(horizontal = 10.dp, vertical = 2.dp),
                )
            }
            Text(
                game?.name ?: layout.title,
                color = Color.White.copy(alpha = 0.85f),
                fontSize = 14.sp,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                modifier = Modifier.weight(1f),
            )
            Icon(if (bluetooth) Icons.Filled.Bluetooth else Icons.Filled.BluetoothDisabled, "Bluetooth",
                tint = if (bluetooth) accent else Color.White.copy(alpha = 0.35f))
            Icon(if (wifi) Icons.Filled.Wifi else Icons.Filled.WifiOff, "Wi-Fi",
                tint = if (wifi) accent else Color.White.copy(alpha = 0.35f), modifier = Modifier.padding(start = 6.dp))
            if (game != null) {
                TextButton(onClick = onQuitGame) {
                    Icon(Icons.Filled.Close, null, tint = Color.White)
                    Text("Quitter le jeu", color = Color.White, fontSize = 13.sp)
                }
            }
            IconButton(onClick = { editing = !editing; selected = null }) {
                Icon(Icons.Filled.Tune, "Personnaliser", tint = if (editing) accent else Color.White)
            }
            IconButton(onClick = onSettings) { Icon(Icons.Filled.Settings, "Connexion", tint = Color.White) }
            Spacer(Modifier.width(260.dp))
        }
        if (editing) {
            Row(
                Modifier
                    .align(Alignment.BottomCenter)
                    .padding(bottom = 6.dp)
                    .background(Color.Black.copy(alpha = 0.6f), RoundedCornerShape(50))
                    .padding(horizontal = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    if (selected == null) "Glisse une commande pour la déplacer" else "Taille",
                    color = Color.White, fontSize = 13.sp, modifier = Modifier.padding(horizontal = 8.dp),
                )
                val sel = selected
                if (sel != null) {
                    val resize = { factor: Float ->
                        val t = tweaks[sel] ?: Tweak()
                        onTweaks(tweaks + (sel to t.copy(scale = (t.scale * factor).coerceIn(0.5f, 2f))))
                    }
                    TextButton(onClick = { resize(1f / 1.12f) }) { Text("−", color = Color.White, fontSize = 20.sp) }
                    TextButton(onClick = { resize(1.12f) }) { Text("+", color = Color.White, fontSize = 20.sp) }
                }
                TextButton(onClick = { onTweaks(emptyMap()); selected = null }) { Text("Réinitialiser", color = Color.White) }
                TextButton(onClick = { editing = false; selected = null }) { Text("Terminé", color = accent, fontWeight = FontWeight.Bold) }
            }
        }
    }
}

/** Customisation: drag a control to move it, the bar below resizes the selected one. */
@Composable
private fun EditPad(
    base: PadLayout,
    tweaks: Map<Int, Tweak>,
    selected: Int?,
    onSelect: (Int?) -> Unit,
    onTweaks: (Map<Int, Tweak>) -> Unit,
) {
    val layout = base.tweaked(tweaks)
    val current by rememberUpdatedState(tweaks)
    val shown by rememberUpdatedState(layout)
    val change by rememberUpdatedState(onTweaks)
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val w = constraints.maxWidth.toFloat()
        val h = constraints.maxHeight.toFloat()
        Canvas(
            Modifier
                .fillMaxSize()
                .pointerInput(w, h) {
                    awaitPointerEventScope {
                        while (true) {
                            val down = awaitPointerEvent().changes.firstOrNull { it.pressed && !it.previousPressed } ?: continue
                            val index = controlAt(shown, down.position, w, h)
                            onSelect(index)
                            if (index == null) continue
                            var last = down.position
                            while (true) {
                                val move = awaitPointerEvent().changes.firstOrNull { it.id == down.id } ?: break
                                if (!move.pressed) break
                                val d = move.position - last
                                last = move.position
                                move.consume()
                                val t = current[index] ?: Tweak()
                                change(current + (index to t.copy(dx = t.dx + d.x / w, dy = t.dy + d.y / h)))
                            }
                        }
                    }
                },
        ) {
            layout.controls.forEachIndexed { i, ctl ->
                when (ctl) {
                    is Control.Button -> drawButton(ctl, 0, false, layout.accent)
                    is Control.Dpad -> drawDpad(ctl, 0, layout.accent)
                    is Control.Stick -> drawStick(ctl, Offset.Zero, layout.accent)
                }
                val (x, y, r) = when (ctl) {
                    is Control.Button -> Triple(ctl.x, ctl.y, if (ctl.pill) ctl.r * 2.3f else ctl.r)
                    is Control.Dpad -> Triple(ctl.x, ctl.y, ctl.r)
                    is Control.Stick -> Triple(ctl.x, ctl.y, ctl.r)
                }
                drawCircle(
                    Color.White.copy(alpha = if (i == selected) 0.9f else 0.25f),
                    r * size.height * 1.15f, Offset(x * size.width, y * size.height),
                    style = Stroke(if (i == selected) 4f else 2f),
                )
            }
        }
    }
}

private fun controlAt(layout: PadLayout, p: Offset, w: Float, h: Float): Int? =
    layout.controls.withIndex().minByOrNull { (_, c) ->
        val (x, y) = when (c) {
            is Control.Button -> c.x to c.y
            is Control.Dpad -> c.x to c.y
            is Control.Stick -> c.x to c.y
        }
        hypot(p.x - x * w, p.y - y * h)
    }?.takeIf { (_, c) ->
        val (x, y, r) = when (c) {
            is Control.Button -> Triple(c.x, c.y, if (c.pill) c.r * 2.4f else c.r)
            is Control.Dpad -> Triple(c.x, c.y, c.r)
            is Control.Stick -> Triple(c.x, c.y, c.r)
        }
        hypot(p.x - x * w, p.y - y * h) <= r * h * 1.4f
    }?.index

/** Draws the controls and turns every finger on the screen into a controller state. */
@Composable
private fun Pad(layout: PadLayout, onState: (PadState) -> Unit) {
    val view = LocalView.current
    var pressed by remember(layout) { mutableStateOf(0) }
    val sticks = remember(layout) { mutableStateMapOf<Int, Offset>() }
    var axisButtons by remember(layout) { mutableStateOf(emptySet<Control.Button>()) }

    BoxWithConstraints(Modifier.fillMaxSize()) {
        val w = constraints.maxWidth.toFloat()
        val h = constraints.maxHeight.toFloat()

        Canvas(
            Modifier
                .fillMaxSize()
                .pointerInput(layout, w, h) {
                    val owners = HashMap<PointerId, Control.Stick>()
                    awaitPointerEventScope {
                        while (true) {
                            val event = awaitPointerEvent()
                            event.changes.forEach { it.consume() }
                            for (c in event.changes) {
                                if (c.pressed && !c.previousPressed) {
                                    stickAt(layout, c.position, w, h)?.let { owners[c.id] = it }
                                } else if (!c.pressed) {
                                    owners.remove(c.id)
                                }
                            }
                            var buttons = 0
                            val axes = FloatArray(4)
                            val stickPos = HashMap<Int, Offset>()
                            val axisHits = HashSet<Control.Button>()
                            for (c in event.changes) {
                                if (!c.pressed) continue
                                val stick = owners[c.id]
                                if (stick != null) {
                                    val center = Offset(stick.x * w, stick.y * h)
                                    val radius = stick.r * h
                                    var d = (c.position - center) / radius
                                    val len = d.getDistance()
                                    if (len > 1f) d /= len
                                    axes[stick.axis] = d.x
                                    axes[stick.axis + 1] = d.y
                                    stickPos[stick.axis] = d
                                    continue
                                }
                                for (ctl in layout.controls) {
                                    when (ctl) {
                                        is Control.Button -> if (hitButton(ctl, c.position, w, h)) {
                                            if (ctl.axis >= 0) {
                                                axes[ctl.axis] = ctl.axisValue
                                                axisHits += ctl
                                            } else buttons = buttons or (1 shl ctl.id)
                                        }
                                        is Control.Dpad -> buttons = buttons or dpadAt(ctl, c.position, w, h)
                                        is Control.Stick -> {}
                                    }
                                }
                            }
                            if (buttons and pressed.inv() != 0 || (axisHits - axisButtons).isNotEmpty()) view.performHapticFeedback(HapticFeedbackConstants.VIRTUAL_KEY)
                            pressed = buttons
                            axisButtons = axisHits
                            sticks.clear()
                            sticks.putAll(stickPos)
                            onState(PadState(buttons, axes.toList()))
                        }
                    }
                },
        ) {
            for (ctl in layout.controls) {
                when (ctl) {
                    is Control.Button -> drawButton(ctl, pressed, ctl in axisButtons, layout.accent)
                    is Control.Dpad -> drawDpad(ctl, pressed, layout.accent)
                    is Control.Stick -> drawStick(ctl, sticks[ctl.axis] ?: Offset.Zero, layout.accent)
                }
            }
        }
    }
}

private fun stickAt(layout: PadLayout, p: Offset, w: Float, h: Float): Control.Stick? =
    layout.controls.filterIsInstance<Control.Stick>().firstOrNull {
        hypot(p.x - it.x * w, p.y - it.y * h) <= it.r * h * 1.35f
    }

private fun hitButton(b: Control.Button, p: Offset, w: Float, h: Float): Boolean {
    val cx = b.x * w
    val cy = b.y * h
    val r = b.r * h
    return if (b.pill) {
        kotlin.math.abs(p.x - cx) <= r * 2.4f && kotlin.math.abs(p.y - cy) <= r * 1.5f
    } else {
        hypot(p.x - cx, p.y - cy) <= r * 1.3f
    }
}

/** D-pad directions under a finger, diagonals included. */
private fun dpadAt(d: Control.Dpad, p: Offset, w: Float, h: Float): Int {
    val r = d.r * h
    val dx = p.x - d.x * w
    val dy = p.y - d.y * h
    if (hypot(dx, dy) > r * 1.3f) return 0
    val dead = r * 0.28f
    var bits = 0
    if (dy < -dead) bits = bits or (1 shl Btn.UP)
    if (dy > dead) bits = bits or (1 shl Btn.DOWN)
    if (dx < -dead) bits = bits or (1 shl Btn.LEFT)
    if (dx > dead) bits = bits or (1 shl Btn.RIGHT)
    return bits
}

private val labelPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
    textAlign = Paint.Align.CENTER
    typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
}

private fun DrawScope.label(text: String, center: Offset, size: Float, color: Long) {
    labelPaint.textSize = size
    labelPaint.color = color.toInt()
    drawIntoCanvas {
        it.nativeCanvas.drawText(text, center.x, center.y - (labelPaint.ascent() + labelPaint.descent()) / 2, labelPaint)
    }
}

private fun DrawScope.drawButton(b: Control.Button, pressed: Int, axisDown: Boolean, accent: Long) {
    val h = size.height
    val c = Offset(b.x * size.width, b.y * h)
    val r = b.r * h
    val down = if (b.axis >= 0) axisDown else pressed and (1 shl b.id) != 0
    val fill = Color(b.fill).let { if (down) lerpWhite(it) else it }
    if (b.pill) {
        val half = Size(r * 2.2f, r * 1.1f)
        drawRoundRect(fill, c - Offset(half.width, half.height), half * 2f, CornerRadius(half.height))
        if (down) drawRoundRect(Color(accent), c - Offset(half.width, half.height), half * 2f, CornerRadius(half.height), style = Stroke(3f))
        label(b.label, c, r * 0.8f, b.text)
    } else {
        drawCircle(Color.Black.copy(alpha = 0.35f), r, c + Offset(0f, r * 0.08f))
        drawCircle(fill, r, c)
        if (down) drawCircle(Color(accent), r, c, style = Stroke(4f))
        label(b.label, c, r * 0.9f, b.text)
    }
}

private fun DrawScope.drawDpad(d: Control.Dpad, pressed: Int, accent: Long) {
    val h = size.height
    val c = Offset(d.x * size.width, d.y * h)
    val r = d.r * h
    val arm = r * 0.36f
    val base = Color(0xFF2C2C36)
    drawCircle(Color.Black.copy(alpha = 0.25f), r * 1.12f, c)
    drawRoundRect(base, Offset(c.x - arm, c.y - r), Size(arm * 2, r * 2), CornerRadius(arm * 0.4f))
    drawRoundRect(base, Offset(c.x - r, c.y - arm), Size(r * 2, arm * 2), CornerRadius(arm * 0.4f))
    val hl = Color(accent).copy(alpha = 0.85f)
    if (pressed and (1 shl Btn.UP) != 0) drawRoundRect(hl, Offset(c.x - arm, c.y - r), Size(arm * 2, r - arm), CornerRadius(arm * 0.4f))
    if (pressed and (1 shl Btn.DOWN) != 0) drawRoundRect(hl, Offset(c.x - arm, c.y + arm), Size(arm * 2, r - arm), CornerRadius(arm * 0.4f))
    if (pressed and (1 shl Btn.LEFT) != 0) drawRoundRect(hl, Offset(c.x - r, c.y - arm), Size(r - arm, arm * 2), CornerRadius(arm * 0.4f))
    if (pressed and (1 shl Btn.RIGHT) != 0) drawRoundRect(hl, Offset(c.x + arm, c.y - arm), Size(r - arm, arm * 2), CornerRadius(arm * 0.4f))
}

private fun DrawScope.drawStick(s: Control.Stick, value: Offset, accent: Long) {
    val h = size.height
    val c = Offset(s.x * size.width, s.y * h)
    val r = s.r * h
    drawCircle(Color.Black.copy(alpha = 0.3f), r, c)
    drawCircle(Color.White.copy(alpha = 0.12f), r, c, style = Stroke(3f))
    val knob = c + value * (r * 0.6f)
    val active = value != Offset.Zero
    drawCircle(Color(s.tint).let { if (active) lerpWhite(it) else it }, r * 0.48f, knob)
    if (active) drawCircle(Color(accent), r * 0.48f, knob, style = Stroke(4f))
}

private fun lerpWhite(c: Color) = Color(
    min(1f, c.red + (1 - c.red) * 0.35f),
    min(1f, c.green + (1 - c.green) * 0.35f),
    min(1f, c.blue + (1 - c.blue) * 0.35f),
    max(c.alpha, 1f),
)
