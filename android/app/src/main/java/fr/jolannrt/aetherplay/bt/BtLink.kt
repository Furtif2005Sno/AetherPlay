package fr.jolannrt.aetherplay.bt

import android.annotation.SuppressLint
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothServerSocket
import android.bluetooth.BluetoothSocket
import android.content.Context
import android.graphics.BitmapFactory
import android.os.Build
import android.util.Base64
import fr.jolannrt.aetherplay.net.Game
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import org.json.JSONObject
import java.util.UUID

/**
 * Second Bluetooth channel beside the controller: a serial link (RFCOMM)
 * AetherPlay connects to, with the same messages as the Wi-Fi link (one JSON
 * object per line). The controller itself stays the plain, reliable one: if
 * this channel fails, only the game info is missing.
 */
@SuppressLint("MissingPermission") // Started once BLUETOOTH_CONNECT is granted.
class BtLink(private val context: Context) {
    private val _connected = MutableStateFlow(false)
    val connected: StateFlow<Boolean> = _connected
    private val _game = MutableStateFlow<Game?>(null)
    val game: StateFlow<Game?> = _game
    private val _player = MutableStateFlow(0)
    val player: StateFlow<Int> = _player

    @Volatile private var server: BluetoothServerSocket? = null
    @Volatile private var socket: BluetoothSocket? = null
    @Volatile private var running = false

    fun start() {
        if (running) return
        val adapter = (context.getSystemService(Context.BLUETOOTH_SERVICE) as? BluetoothManager)?.adapter ?: return
        running = true
        Thread({
            while (running) {
                try {
                    val s = server ?: adapter.listenUsingRfcommWithServiceRecord(SERVICE_NAME, SERVICE_UUID).also { server = it }
                    serve(s.accept())
                } catch (_: Exception) {
                    // Bluetooth off or socket closed: try again a bit later.
                    runCatching { server?.close() }
                    server = null
                    if (running) Thread.sleep(3000)
                }
            }
        }, "aetherplay-bt-link").start()
    }

    private fun serve(s: BluetoothSocket) {
        socket = s
        _connected.value = true
        try {
            write(JSONObject().put("t", "hello").put("v", 1).put("name", "${Build.MANUFACTURER} ${Build.MODEL}".trim()))
            val reader = s.inputStream.bufferedReader(Charsets.UTF_8)
            while (running) {
                val line = reader.readLine() ?: break
                val msg = runCatching { JSONObject(line) }.getOrNull() ?: continue
                when (msg.optString("t")) {
                    "game" -> _game.value = Game(
                        platform = msg.optString("platform"),
                        name = msg.optString("name"),
                        cover = msg.optString("cover").takeIf { it.isNotEmpty() && it != "null" }?.let { decode(it) },
                    )
                    "idle" -> _game.value = null
                    "player" -> _player.value = msg.optInt("n", 0)
                }
            }
        } catch (_: Exception) {
        } finally {
            runCatching { s.close() }
            socket = null
            _connected.value = false
            _game.value = null
            _player.value = 0
        }
    }

    private fun write(msg: JSONObject) {
        val s = socket ?: return
        runCatching {
            synchronized(this) {
                s.outputStream.write((msg.toString() + "\n").toByteArray(Charsets.UTF_8))
                s.outputStream.flush()
            }
        }
    }

    /** Asks AetherPlay to close the running game. */
    fun quitGame() {
        Thread { write(JSONObject().put("t", "quit")) }.start()
    }

    fun stop() {
        running = false
        runCatching { socket?.close() }
        runCatching { server?.close() }
        server = null
    }

    private fun decode(b64: String) = runCatching {
        val bytes = Base64.decode(b64, Base64.DEFAULT)
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
    }.getOrNull()

    companion object {
        const val SERVICE_NAME = "AetherPlay"
        /** Same UUID on the computer side (src-tauri/src/bt_link.rs). */
        val SERVICE_UUID: UUID = UUID.fromString("6e3a5c1e-7f2b-4c1a-9d6e-ae7e4b1a0001")
    }
}
