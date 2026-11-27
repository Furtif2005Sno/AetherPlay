package fr.jolannrt.aetherplay.net

import android.annotation.SuppressLint
import android.bluetooth.BluetoothManager
import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.TimeUnit

/**
 * The RomM server as relay with AetherPlay when the phone only has a
 * Bluetooth link to the computer (no shared network). Linked once with a
 * code generated in AetherPlay (RomM client token: read-only, the user's
 * password never reaches the phone), it reads every few seconds:
 * * RomM's "now playing" (activity), published by AetherPlay: the game,
 *   its console and cover;
 * * `ui_settings.aetherplay.players` of the user: this phone's player
 *   number, keyed by its Bluetooth name.
 */
class ServerRelay(private val context: Context) {
    private val prefs = context.getSharedPreferences("server", Context.MODE_PRIVATE)
    private val http = OkHttpClient.Builder().connectTimeout(8, TimeUnit.SECONDS).readTimeout(10, TimeUnit.SECONDS).build()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var job: Job? = null

    private val _linked = MutableStateFlow(token != null)
    val linked: StateFlow<Boolean> = _linked
    private val _game = MutableStateFlow<Game?>(null)
    val game: StateFlow<Game?> = _game
    private val _player = MutableStateFlow(0)
    val player: StateFlow<Int> = _player

    val server: String? get() = prefs.getString("server", null)
    private val token: String? get() = prefs.getString("token", null)

    /** Exchanges the code shown in AetherPlay for this phone's access. */
    suspend fun link(serverUrl: String, code: String): Result<Unit> = withContext(Dispatchers.IO) {
        runCatching {
            val base = serverUrl.trim().trimEnd('/').let { if (it.startsWith("http")) it else "https://$it" }
            val body = JSONObject().put("code", code.trim()).toString().toRequestBody("application/json".toMediaType())
            val res = http.newCall(Request.Builder().url("$base/api/client-tokens/exchange").post(body).build()).execute()
            res.use {
                check(it.isSuccessful) { "code refusé (${it.code})" }
                val raw = JSONObject(it.body!!.string()).getString("raw_token")
                prefs.edit().putString("server", base).putString("token", raw).apply()
            }
            _linked.value = true
        }
    }

    fun unlink() {
        prefs.edit().remove("token").apply()
        _linked.value = false
        _game.value = null
        _player.value = 0
    }

    /** Polls while the phone has no Wi-Fi link to AetherPlay. */
    fun setActive(active: Boolean) {
        if (!active) {
            job?.cancel()
            job = null
            _game.value = null
            _player.value = 0
            return
        }
        if (job != null) return
        job = scope.launch {
            var coverFor: String? = null
            var cover: Bitmap? = null
            while (isActive) {
                runCatching { poll(coverFor, cover) }.getOrNull()?.let { (key, bmp) ->
                    coverFor = key
                    cover = bmp
                }
                delay(4000)
            }
        }
    }

    private fun get(path: String): String? {
        val base = server ?: return null
        val t = token ?: return null
        val req = Request.Builder().url("$base$path").header("Authorization", "Bearer $t").build()
        return http.newCall(req).execute().use { if (it.isSuccessful) it.body?.string() else null }
    }

    /** One round: game and player. Returns the cover key and bitmap to keep. */
    private fun poll(coverFor: String?, cover: Bitmap?): Pair<String?, Bitmap?> {
        val me = JSONObject(get("/api/users/me") ?: return coverFor to cover)
        val userId = me.optLong("id")
        // Player number by this phone's Bluetooth name.
        val ui = me.opt("ui_settings").let { if (it is String) runCatching { JSONObject(it) }.getOrNull() else it as? JSONObject }
        val players = ui?.optJSONObject("aetherplay")?.optJSONObject("players")
        _player.value = bluetoothName()?.let { players?.optInt(it, 0) } ?: 0

        // The user's most recent "now playing".
        val activity = JSONArray(get("/api/activity") ?: "[]")
        val mine = (0 until activity.length()).map { activity.getJSONObject(it) }
            .filter { it.optLong("user_id") == userId }
            .maxByOrNull { it.optString("started_at") }
        if (mine == null) {
            _game.value = null
            return coverFor to cover
        }
        val coverPath = mine.optString("rom_cover_path")
        var bmp = cover
        if (coverPath != coverFor) {
            bmp = coverPath.takeIf { it.isNotEmpty() }?.let { path ->
                val base = server ?: return@let null
                val url = if (path.startsWith("/")) "$base$path" else "$base/assets/romm/resources/$path"
                runCatching {
                    http.newCall(Request.Builder().url(url).header("Authorization", "Bearer ${token}").build()).execute()
                        .use { r -> r.body?.bytes()?.let { BitmapFactory.decodeByteArray(it, 0, it.size) } }
                }.getOrNull()
            }
        }
        _game.value = Game(mine.optString("platform_slug"), mine.optString("rom_name"), bmp)
        return coverPath to bmp
    }

    @SuppressLint("MissingPermission")
    private fun bluetoothName(): String? =
        runCatching { (context.getSystemService(Context.BLUETOOTH_SERVICE) as BluetoothManager).adapter?.name }.getOrNull()
}
