package fr.jolannrt.aetherplay.ui

import android.graphics.Paint
import android.graphics.Typeface
import android.view.HapticFeedbackConstants
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.border
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
import androidx.compose.material.icons.filled.Image
import androidx.compose.material.icons.filled.SportsEsports
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
import fr.jolannrt.aetherplay.pad.ScreenArea
import android.graphics.Bitmap
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import fr.jolannrt.aetherplay.pad.Tweak
import fr.jolannrt.aetherplay.pad.BUTTON_COLORS
import fr.jolannrt.aetherplay.pad.tweaked
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min

/** What is drawn behind the buttons. */
enum class Backdrop { CONTROLLER, COVER }

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
    /** AetherPlay can turn Wi-Fi input into a controller the games see. */
    wifiInput: Boolean = true,
    tweaks: Map<Int, Tweak> = emptyMap(),
    onTweaks: (Map<Int, Tweak>) -> Unit = {},
    backdrop: Backdrop = Backdrop.CONTROLLER,
    onBackdrop: (Backdrop) -> Unit = {},
    frame: Bitmap? = null,
    screenSize: Pair<Int, Int>? = null,
    onTouch: (Float, Float, Boolean) -> Unit = { _, _, _ -> },
    buttonStyle: ButtonStyle = ButtonStyle.ORIGINAL,
    onButtonStyle: (ButtonStyle) -> Unit = {},
    /** A problem to show over the controller (e.g. the server link needs a new code). */
    notice: String? = null,
) {
    val accent = Color(layout.accent)
    var editing by remember { mutableStateOf(false) }
    var selected by remember(layout) { mutableStateOf<Int?>(null) }
    Box(
        Modifier
            .fillMaxSize()
            .background(Color(layout.background)),
    ) {
        when (backdrop) {
            Backdrop.CONTROLLER -> Canvas(Modifier.fillMaxSize()) { drawBody(layout.body) }
            Backdrop.COVER -> game?.cover?.let {
                Image(
                    it.asImageBitmap(), null,
                    Modifier.fillMaxSize(),
                    contentScale = ContentScale.Crop,
                    alpha = 0.3f,
                )
            }
        }
        if (editing) {
            EditPad(layout, tweaks, selected, { selected = it }, onTweaks, buttonStyle)
        } else {
            Pad(layout.tweaked(tweaks), onState, frame, screenSize, onTouch, buttonStyle, tweaks.styles())
        }
        Row(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 8.dp, vertical = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Spacer(Modifier.width(235.dp))
            if (player > 0 || bluetooth) {
                Text(
                    // Bluetooth alone: the emulator numbers players by connection order.
                    if (player > 0) "J$player" else "BT",
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
                IconButton(onClick = onQuitGame) {
                    Icon(Icons.Filled.Close, "Quitter le jeu", tint = Color(0xFFFF6B6B))
                }
            }
            IconButton(onClick = {
                onBackdrop(if (backdrop == Backdrop.CONTROLLER) Backdrop.COVER else Backdrop.CONTROLLER)
            }) {
                Icon(
                    if (backdrop == Backdrop.CONTROLLER) Icons.Filled.Image else Icons.Filled.SportsEsports,
                    if (backdrop == Backdrop.CONTROLLER) "Fond : jaquette" else "Fond : manette",
                    tint = Color.White,
                )
            }
            IconButton(onClick = { editing = !editing; selected = null }) {
                Icon(Icons.Filled.Tune, "Personnaliser", tint = if (editing) accent else Color.White)
            }
            IconButton(onClick = onSettings) { Icon(Icons.Filled.Settings, "Connexion", tint = Color.White) }
            Spacer(Modifier.width(235.dp))
        }
        if (!editing && notice != null) {
            Text(
                notice,
                color = Color.White,
                fontSize = 13.sp,
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .padding(bottom = 6.dp, start = 80.dp, end = 80.dp)
                    .background(Color(0xCCB3261E), RoundedCornerShape(12.dp))
                    .padding(horizontal = 12.dp, vertical = 6.dp),
            )
        } else if (!editing && game != null && !bluetooth && !wifiInput) {
            Text(
                "Les jeux ne reçoivent pas cette manette : connecte le téléphone en Bluetooth, " +
                    "ou sur Windows installe le pilote dans AetherPlay (Réglages > AetherPlay > Manette Android).",
                color = Color.White,
                fontSize = 13.sp,
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .padding(bottom = 6.dp, start = 80.dp, end = 80.dp)
                    .background(Color(0xCCB3261E), RoundedCornerShape(12.dp))
                    .padding(horizontal = 12.dp, vertical = 6.dp),
            )
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
                if (sel == null) {
                    // Accessibility: button colours (Origine = the console's, by default).
                    TextButton(onClick = {
                        val all = ButtonStyle.entries
                        onButtonStyle(all[(all.indexOf(buttonStyle) + 1) % all.size])
                    }) { Text("Style : ${buttonStyle.label}", color = Color.White) }
                } else {
                    // This control only: its own style, then its own colour.
                    val t = tweaks[sel] ?: Tweak()
                    TextButton(onClick = {
                        val order = listOf<ButtonStyle?>(null) + ButtonStyle.entries
                        onTweaks(tweaks + (sel to t.copy(style = order[(order.indexOf(t.style) + 1) % order.size])))
                    }) { Text("Style : ${t.style?.label ?: "comme les autres"}", color = Color.White) }
                    if (layout.controls.getOrNull(sel) !is Control.Dpad) {
                        TextButton(onClick = {
                            val next = BUTTON_COLORS[(BUTTON_COLORS.indexOf(t.color) + 1) % BUTTON_COLORS.size]
                            // A colour shows with the Origine style.
                            onTweaks(tweaks + (sel to t.copy(color = next, style = if (next != null) ButtonStyle.ORIGINAL else t.style)))
                        }) {
                            Text("Couleur", color = Color.White)
                            Box(
                                Modifier
                                    .padding(start = 6.dp)
                                    .size(16.dp)
                                    .background(t.color?.let { Color(it) } ?: Color.Transparent, RoundedCornerShape(50))
                                    .border(1.5.dp, Color.White, RoundedCornerShape(50)),
                            )
                        }
                    }
                }
                TextButton(onClick = { onTweaks(emptyMap()); selected = null }) { Text("Réinitialiser", color = Color.White) }
                TextButton(onClick = { editing = false; selected = null }) { Text("Terminé", color = accent, fontWeight = FontWeight.Bold) }
            }
        }
    }
}

private fun Map<Int, Tweak>.styles(): Map<Int, ButtonStyle> =
    entries.mapNotNull { (i, t) -> t.style?.let { i to it } }.toMap()

/** Customisation: drag a control to move it, the bar below resizes the selected one. */
@Composable
private fun EditPad(
    base: PadLayout,
    tweaks: Map<Int, Tweak>,
    selected: Int?,
    onSelect: (Int?) -> Unit,
    onTweaks: (Map<Int, Tweak>) -> Unit,
    style: ButtonStyle = ButtonStyle.ORIGINAL,
) {
    val layout = base.tweaked(tweaks)
    val styles = tweaks.styles()
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
                val st = styles[i] ?: style
                when (ctl) {
                    is Control.Button -> drawButton(ctl, 0, false, layout.accent, st)
                    is Control.Dpad -> drawDpad(ctl, 0, layout.accent, st)
                    is Control.Stick -> drawStick(ctl, Offset.Zero, layout.accent, st)
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
private fun Pad(
    layout: PadLayout,
    onState: (PadState) -> Unit,
    frame: Bitmap? = null,
    screenSize: Pair<Int, Int>? = null,
    onTouch: (Float, Float, Boolean) -> Unit = { _, _, _ -> },
    style: ButtonStyle = ButtonStyle.ORIGINAL,
    /** Per-control styles chosen in Personnaliser (index -> style). */
    styles: Map<Int, ButtonStyle> = emptyMap(),
) {
    val view = LocalView.current
    val liveSize by rememberUpdatedState(screenSize)
    val touch by rememberUpdatedState(onTouch)
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
                    var stylus: PointerId? = null
                    awaitPointerEventScope {
                        while (true) {
                            val event = awaitPointerEvent()
                            event.changes.forEach { it.consume() }
                            // Second screen: the finger that lands on it is the stylus.
                            val image = layout.screen?.let { area -> liveSize?.let { screenRect(area, it, w, h) } }
                            for (c in event.changes) {
                                if (image != null && c.pressed && !c.previousPressed && stylus == null && image.contains(c.position)) {
                                    stylus = c.id
                                }
                                if (c.id == stylus && image != null) {
                                    val x = ((c.position.x - image.left) / image.width).coerceIn(0f, 1f)
                                    val y = ((c.position.y - image.top) / image.height).coerceIn(0f, 1f)
                                    touch(x, y, c.pressed)
                                    if (!c.pressed) stylus = null
                                }
                            }
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
                                if (!c.pressed || c.id == stylus) continue
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
            val area = layout.screen
            if (area != null && screenSize != null) {
                val r = screenRect(area, screenSize, size.width, size.height)
                drawRoundRect(Color.Black, r.topLeft - Offset(8f, 8f), Size(r.width + 16f, r.height + 16f), CornerRadius(14f))
                if (frame != null) {
                    drawImage(
                        frame.asImageBitmap(),
                        dstOffset = IntOffset(r.left.toInt(), r.top.toInt()),
                        dstSize = IntSize(r.width.toInt(), r.height.toInt()),
                        filterQuality = FilterQuality.None,
                    )
                }
            }
            layout.controls.forEachIndexed { i, ctl ->
                val st = styles[i] ?: style
                when (ctl) {
                    is Control.Button -> drawButton(ctl, pressed, ctl in axisButtons, layout.accent, st)
                    is Control.Dpad -> drawDpad(ctl, pressed, layout.accent, st)
                    is Control.Stick -> drawStick(ctl, sticks[ctl.axis] ?: Offset.Zero, layout.accent, st)
                }
            }
        }
    }
}

/** The second screen's image: [size] fitted in [area], centered. */
private fun screenRect(area: ScreenArea, size: Pair<Int, Int>, w: Float, h: Float): Rect {
    val aw = (area.x1 - area.x0) * w
    val ah = (area.y1 - area.y0) * h
    val scale = min(aw / size.first, ah / size.second)
    val iw = size.first * scale
    val ih = size.second * scale
    val left = area.x0 * w + (aw - iw) / 2
    val top = area.y0 * h + (ah - ih) / 2
    return Rect(left, top, left + iw, top + ih)
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

/** How buttons are drawn (Personnaliser > Style): the console's colours by default. */
enum class ButtonStyle(val label: String) {
    ORIGINAL("Origine"),
    CONTRAST("Contraste élevé"),
    OUTLINE("Contour"),
}

private val CONTRAST_FILL = Color(0xFFF2F2F5)
private val CONTRAST_TEXT = 0xFF111114

private fun DrawScope.drawButton(b: Control.Button, pressed: Int, axisDown: Boolean, accent: Long, style: ButtonStyle = ButtonStyle.ORIGINAL) {
    val h = size.height
    val c = Offset(b.x * size.width, b.y * h)
    val r = b.r * h
    val down = if (b.axis >= 0) axisDown else pressed and (1 shl b.id) != 0
    val base = when (style) {
        ButtonStyle.ORIGINAL -> Color(b.fill)
        ButtonStyle.CONTRAST -> CONTRAST_FILL
        ButtonStyle.OUTLINE -> Color.Black.copy(alpha = 0.25f)
    }
    val fill = if (down) (if (style == ButtonStyle.CONTRAST) Color(accent) else lerpWhite(base)) else base
    val text = when (style) {
        ButtonStyle.ORIGINAL -> b.text
        ButtonStyle.CONTRAST -> if (down) 0xFFFFFFFF else CONTRAST_TEXT
        ButtonStyle.OUTLINE -> 0xFFFFFFFF
    }
    val outline = style == ButtonStyle.OUTLINE
    if (b.pill) {
        val half = Size(r * 2.2f, r * 1.1f)
        drawRoundRect(fill, c - Offset(half.width, half.height), half * 2f, CornerRadius(half.height))
        if (outline) drawRoundRect(Color.White, c - Offset(half.width, half.height), half * 2f, CornerRadius(half.height), style = Stroke(4f))
        if (down) drawRoundRect(Color(accent), c - Offset(half.width, half.height), half * 2f, CornerRadius(half.height), style = Stroke(4f))
        label(b.label, c, r * 0.8f, text)
    } else {
        drawCircle(Color.Black.copy(alpha = 0.35f), r, c + Offset(0f, r * 0.08f))
        drawCircle(fill, r, c)
        if (outline) drawCircle(Color.White, r, c, style = Stroke(5f))
        if (down) drawCircle(Color(accent), r, c, style = Stroke(5f))
        label(b.label, c, r * 0.9f, text)
    }
}

private fun DrawScope.drawDpad(d: Control.Dpad, pressed: Int, accent: Long, style: ButtonStyle = ButtonStyle.ORIGINAL) {
    val h = size.height
    val c = Offset(d.x * size.width, d.y * h)
    val r = d.r * h
    val arm = r * 0.36f
    val corner = CornerRadius(arm * 0.4f)
    val base = when (style) {
        ButtonStyle.ORIGINAL -> Color(0xFF16161B)
        ButtonStyle.CONTRAST -> CONTRAST_FILL
        ButtonStyle.OUTLINE -> Color.Black.copy(alpha = 0.25f)
    }
    drawCircle(Color.Black.copy(alpha = 0.18f), r * 1.12f, c)
    drawCircle(Color.White.copy(alpha = 0.08f), r * 1.12f, c, style = Stroke(2f))
    val vertical = Offset(c.x - arm, c.y - r) to Size(arm * 2, r * 2)
    val horizontal = Offset(c.x - r, c.y - arm) to Size(r * 2, arm * 2)
    for ((o, s) in listOf(vertical, horizontal)) drawRoundRect(base, o, s, corner)
    // Always outlined, so the cross reads on dark backgrounds too.
    val edge = if (style == ButtonStyle.ORIGINAL) Color.White.copy(alpha = 0.35f) else Color.White
    if (style != ButtonStyle.CONTRAST) for ((o, s) in listOf(vertical, horizontal)) drawRoundRect(edge, o, s, corner, style = Stroke(if (style == ButtonStyle.OUTLINE) 4f else 2.5f))
    // Arrows: which way each arm goes.
    val arrow = if (style == ButtonStyle.CONTRAST) Color(CONTRAST_TEXT) else Color.White.copy(alpha = 0.7f)
    val a = arm * 0.45f
    val tip = r * 0.82f
    for ((dx, dy) in listOf(0f to -1f, 0f to 1f, -1f to 0f, 1f to 0f)) {
        val p = c + Offset(dx * tip, dy * tip)
        val back = c + Offset(dx * (tip - a), dy * (tip - a))
        val side = Offset(dy * a * 0.8f, dx * a * 0.8f)
        drawPath(androidx.compose.ui.graphics.Path().apply {
            moveTo(p.x, p.y); lineTo(back.x + side.x, back.y + side.y); lineTo(back.x - side.x, back.y - side.y); close()
        }, arrow)
    }
    val hl = Color(accent).copy(alpha = 0.85f)
    if (pressed and (1 shl Btn.UP) != 0) drawRoundRect(hl, Offset(c.x - arm, c.y - r), Size(arm * 2, r - arm), corner)
    if (pressed and (1 shl Btn.DOWN) != 0) drawRoundRect(hl, Offset(c.x - arm, c.y + arm), Size(arm * 2, r - arm), corner)
    if (pressed and (1 shl Btn.LEFT) != 0) drawRoundRect(hl, Offset(c.x - r, c.y - arm), Size(r - arm, arm * 2), corner)
    if (pressed and (1 shl Btn.RIGHT) != 0) drawRoundRect(hl, Offset(c.x + arm, c.y - arm), Size(r - arm, arm * 2), corner)
}

private fun DrawScope.drawStick(s: Control.Stick, value: Offset, accent: Long, style: ButtonStyle = ButtonStyle.ORIGINAL) {
    val h = size.height
    val c = Offset(s.x * size.width, s.y * h)
    val r = s.r * h
    drawCircle(Color.Black.copy(alpha = 0.3f), r, c)
    drawCircle(Color.White.copy(alpha = if (style == ButtonStyle.ORIGINAL) 0.12f else 0.8f), r, c, style = Stroke(if (style == ButtonStyle.ORIGINAL) 3f else 4f))
    val knob = c + value * (r * 0.6f)
    val active = value != Offset.Zero
    val tint = when (style) {
        ButtonStyle.ORIGINAL -> Color(s.tint)
        ButtonStyle.CONTRAST -> CONTRAST_FILL
        ButtonStyle.OUTLINE -> Color.Black.copy(alpha = 0.25f)
    }
    drawCircle(if (active) lerpWhite(tint) else tint, r * 0.48f, knob)
    if (style == ButtonStyle.OUTLINE) drawCircle(Color.White, r * 0.48f, knob, style = Stroke(4f))
    if (active) drawCircle(Color(accent), r * 0.48f, knob, style = Stroke(4f))
}
private fun lerpWhite(c: Color) = Color(
    min(1f, c.red + (1 - c.red) * 0.35f),
    min(1f, c.green + (1 - c.green) * 0.35f),
    min(1f, c.blue + (1 - c.blue) * 0.35f),
    max(c.alpha, 1f),
)
