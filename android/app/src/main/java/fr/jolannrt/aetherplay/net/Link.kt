package fr.jolannrt.aetherplay.net

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.util.Base64
import fr.jolannrt.aetherplay.pad.PadState
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.TimeUnit

/** The game AetherPlay is running, which picks the controller layout. */
data class Game(val platform: String, val name: String, val cover: Bitmap?)

/**
 * Wi-Fi link to AetherPlay (WebSocket, JSON messages, see PROTOCOL.md):
 * pairing, the running game, and the controller state, which drives
 * AetherPlay's menus and, without Bluetooth, the emulators.
 */
class Link {
    sealed interface Status {
        data object Disconnected : Status
        data object Connecting : Status
        /** First connection: AetherPlay shows a code to type on the phone. */
        data object NeedPin : Status
        data class Connected(val server: String) : Status
        data class Failed(val reason: String) : Status
    }

    private val _status = MutableStateFlow<Status>(Status.Disconnected)
    val status: StateFlow<Status> = _status
    private val _game = MutableStateFlow<Game?>(null)
    val game: StateFlow<Game?> = _game
    /** False when AetherPlay cannot create a virtual controller (macOS, or no driver on Windows). */
    private val _wifiInput = MutableStateFlow(true)
    val wifiInput: StateFlow<Boolean> = _wifiInput
    /** Second screen streamed by AetherPlay (DS bottom screen, Wii U GamePad): its size, or null. */
    private val _screen = MutableStateFlow<Pair<Int, Int>?>(null)
    val screen: StateFlow<Pair<Int, Int>?> = _screen
    /** Latest image of that screen. */
    private val _frame = MutableStateFlow<Bitmap?>(null)
    val frame: StateFlow<Bitmap?> = _frame
    /** RetroTogether: this phone's player number (1, 2, 3…), 0 until known. */
    private val _player = MutableStateFlow(0)
    val player: StateFlow<Int> = _player
    /** Token issued at pairing; persisted by the caller. */
    private val _token = MutableStateFlow<String?>(null)
    val token: StateFlow<String?> = _token

    private val client = OkHttpClient.Builder()
        .pingInterval(5, TimeUnit.SECONDS)
        .connectTimeout(5, TimeUnit.SECONDS)
        .build()
    private var socket: WebSocket? = null
    private var target: Server? = null
    private var deviceName = "Android"
    private var lastSent: PadState? = null

    fun connect(server: Server, token: String?, name: String) {
        disconnect()
        target = server
        deviceName = name
        _token.value = token
        _status.value = Status.Connecting
        val request = Request.Builder().url("ws://${server.host}:${server.port}/phone").build()
        socket = client.newWebSocket(request, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                val hello = JSONObject().put("t", "hello").put("v", 1).put("name", deviceName)
                token?.let { hello.put("token", it) }
                webSocket.send(hello.toString())
            }

            override fun onMessage(webSocket: WebSocket, text: String) = handle(JSONObject(text))

            override fun onMessage(webSocket: WebSocket, bytes: okio.ByteString) {
                if (_screen.value == null) return
                val data = bytes.toByteArray()
                BitmapFactory.decodeByteArray(data, 0, data.size)?.let { _frame.value = it }
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                if (webSocket == socket) _status.value = Status.Failed("AetherPlay injoignable (${t.message ?: "erreur réseau"})")
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                if (webSocket == socket && _status.value is Status.Connected) _status.value = Status.Disconnected
            }
        })
    }

    private fun handle(msg: JSONObject) {
        when (msg.optString("t")) {
            "pin" -> _status.value = Status.NeedPin
            "welcome" -> {
                msg.optString("token").takeIf { it.isNotEmpty() }?.let { _token.value = it }
                _status.value = Status.Connected(msg.optString("server", target?.name ?: "AetherPlay"))
                lastSent = null
            }
            "denied" -> {
                _token.value = null
                _status.value = if (msg.optBoolean("retry")) Status.NeedPin else Status.Failed(msg.optString("reason", "Refusé"))
            }
            "game" -> _game.value = Game(
                platform = msg.optString("platform"),
                name = msg.optString("name"),
                cover = msg.optString("cover").takeIf { it.isNotEmpty() }?.let { decode(it) },
            )
            "idle" -> _game.value = null
            "virtual" -> _wifiInput.value = msg.optBoolean("ok", false)
            "player" -> _player.value = msg.optInt("n", 0)
            "screen" -> if (msg.optBoolean("on")) {
                _screen.value = msg.optInt("w", 256) to msg.optInt("h", 192)
            } else {
                _screen.value = null
                _frame.value = null
            }
        }
    }

    fun sendPin(pin: String) {
        val hello = JSONObject().put("t", "hello").put("v", 1).put("name", deviceName).put("pin", pin)
        socket?.send(hello.toString())
    }

    /** Sends the state if it changed. [bluetooth]: whether Bluetooth already feeds the games. */
    fun sendPad(state: PadState, bluetooth: Boolean) {
        if (_status.value !is Status.Connected || state == lastSent) return
        lastSent = state
        val msg = JSONObject().put("t", "pad").put("b", state.buttons).put("bt", bluetooth)
            .put("a", JSONArray(state.axes.map { (it * 100).toInt() / 100.0 }))
        socket?.send(msg.toString())
    }

    /** A touch on the second screen, [x] / [y] in 0..1 over its image. */
    fun sendTouch(x: Float, y: Float, down: Boolean) {
        val msg = JSONObject().put("t", "touch").put("x", x.toDouble()).put("y", y.toDouble()).put("down", down)
        socket?.send(msg.toString())
    }

    /** Asks AetherPlay to close the running game. */
    fun quitGame() {
        socket?.send(JSONObject().put("t", "quit").toString())
    }

    fun disconnect() {
        socket?.close(1000, null)
        socket = null
        _game.value = null
        _screen.value = null
        _frame.value = null
        _player.value = 0
        _status.value = Status.Disconnected
    }

    private fun decode(b64: String): Bitmap? = runCatching {
        val bytes = Base64.decode(b64, Base64.DEFAULT)
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
    }.getOrNull()
}
