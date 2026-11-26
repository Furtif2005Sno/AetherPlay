package fr.jolannrt.aetherplay.pad

import android.content.Context
import org.json.JSONObject

/** A control moved (fractions of the screen) and resized by the player. */
data class Tweak(val dx: Float = 0f, val dy: Float = 0f, val scale: Float = 1f)

/** The layout with the player's adjustments, keyed by control index. */
fun PadLayout.tweaked(tweaks: Map<Int, Tweak>): PadLayout =
    if (tweaks.isEmpty()) this else copy(controls = controls.mapIndexed { i, c -> tweaks[i]?.let { c.apply(it) } ?: c })

private fun Control.apply(t: Tweak): Control {
    fun Float.x() = (this + t.dx).coerceIn(0.02f, 0.98f)
    fun Float.y() = (this + t.dy).coerceIn(0.04f, 0.96f)
    return when (this) {
        is Control.Button -> copy(x = x.x(), y = y.y(), r = r * t.scale)
        is Control.Dpad -> copy(x = x.x(), y = y.y(), r = r * t.scale)
        is Control.Stick -> copy(x = x.x(), y = y.y(), r = r * t.scale)
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
                k.toInt() to Tweak(t.getDouble(0).toFloat(), t.getDouble(1).toFloat(), t.getDouble(2).toFloat())
            }
        }.getOrDefault(emptyMap())
    }

    fun save(layout: PadLayout, tweaks: Map<Int, Tweak>) {
        if (tweaks.isEmpty()) {
            prefs.edit().remove(layout.title).apply()
            return
        }
        val o = JSONObject()
        for ((i, t) in tweaks) o.put(i.toString(), org.json.JSONArray(listOf(t.dx.toDouble(), t.dy.toDouble(), t.scale.toDouble())))
        prefs.edit().putString(layout.title, o.toString()).apply()
    }
}
