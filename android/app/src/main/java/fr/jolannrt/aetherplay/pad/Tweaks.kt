package fr.jolannrt.aetherplay.pad

import android.content.Context
import org.json.JSONObject

/**
 * A control moved (fractions of the screen) and resized by the player, with
 * its own look if chosen: [style] (null: the style of the whole controller)
 * and [color] (0xAARRGGBB fill, null: the console's).
 */
data class Tweak(
    val dx: Float = 0f,
    val dy: Float = 0f,
    val scale: Float = 1f,
    val style: fr.jolannrt.aetherplay.ui.ButtonStyle? = null,
    val color: Long? = null,
)

/** Colours offered per button (Personnaliser > Couleur), null = the console's. */
val BUTTON_COLORS: List<Long?> = listOf(
    null, 0xFFF2F2F5, 0xFFFFD43B, 0xFFFF8A00, 0xFFE03131, 0xFFE64980, 0xFF7048E8, 0xFF1C7ED6, 0xFF0CA678, 0xFF2C2C36,
)

/** Readable label colour on [fill]. */
fun textOn(fill: Long): Long {
    val r = (fill shr 16) and 0xFF
    val g = (fill shr 8) and 0xFF
    val b = fill and 0xFF
    return if (r * 299 + g * 587 + b * 114 > 150_000) 0xFF111114 else 0xFFFFFFFF
}

/** The layout with the player's adjustments, keyed by control index. */
fun PadLayout.tweaked(tweaks: Map<Int, Tweak>): PadLayout =
    if (tweaks.isEmpty()) this else copy(controls = controls.mapIndexed { i, c -> tweaks[i]?.let { c.apply(it) } ?: c })

private fun Control.apply(t: Tweak): Control {
    fun Float.x() = (this + t.dx).coerceIn(0.02f, 0.98f)
    fun Float.y() = (this + t.dy).coerceIn(0.04f, 0.96f)
    return when (this) {
        is Control.Button -> copy(x = x.x(), y = y.y(), r = r * t.scale, fill = t.color ?: fill, text = t.color?.let(::textOn) ?: text)
        is Control.Dpad -> copy(x = x.x(), y = y.y(), r = r * t.scale)
        is Control.Stick -> copy(x = x.x(), y = y.y(), r = r * t.scale, tint = t.color ?: tint)
    }
}

/** Saved adjustments, one set per console layout. */
class TweakStore(context: Context) {
    private val prefs = context.getSharedPreferences("layouts", Context.MODE_PRIVATE)

    fun load(layout: PadLayout): Map<Int, Tweak> {
        val json = prefs.getString(layout.title, null) ?: return emptyMap()
        return runCatching {
            val o = JSONObject(json)
            o.keys().asSequence().associate { k ->
                val t = o.getJSONArray(k)
                k.toInt() to Tweak(
                    t.getDouble(0).toFloat(), t.getDouble(1).toFloat(), t.getDouble(2).toFloat(),
                    style = t.optString(3).takeIf { it.isNotEmpty() && it != "null" }
                        ?.let { runCatching { fr.jolannrt.aetherplay.ui.ButtonStyle.valueOf(it) }.getOrNull() },
                    color = if (t.length() > 4 && !t.isNull(4)) t.getLong(4) else null,
                )
            }
        }.getOrDefault(emptyMap())
    }

    fun save(layout: PadLayout, tweaks: Map<Int, Tweak>) {
        if (tweaks.isEmpty()) {
            prefs.edit().remove(layout.title).apply()
            return
        }
        val o = JSONObject()
        for ((i, t) in tweaks) o.put(i.toString(), org.json.JSONArray(
            listOf(t.dx.toDouble(), t.dy.toDouble(), t.scale.toDouble(), t.style?.name ?: JSONObject.NULL, t.color ?: JSONObject.NULL),
        ))
        prefs.edit().putString(layout.title, o.toString()).apply()
    }
}
