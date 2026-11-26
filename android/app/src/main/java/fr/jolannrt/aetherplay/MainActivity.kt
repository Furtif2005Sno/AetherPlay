package fr.jolannrt.aetherplay

import android.Manifest
import android.bluetooth.BluetoothAdapter
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.core.content.ContextCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import fr.jolannrt.aetherplay.bt.HidGamepad
import fr.jolannrt.aetherplay.net.Discovery
import fr.jolannrt.aetherplay.net.Link
import fr.jolannrt.aetherplay.net.Server
import fr.jolannrt.aetherplay.pad.PadState
import fr.jolannrt.aetherplay.pad.TweakStore
import fr.jolannrt.aetherplay.pad.layoutFor
import fr.jolannrt.aetherplay.ui.Backdrop
import fr.jolannrt.aetherplay.ui.ConnectScreen
import fr.jolannrt.aetherplay.ui.PadScreen

class MainActivity : ComponentActivity() {
    private lateinit var discovery: Discovery
    private val link = Link()
    private lateinit var hid: HidGamepad
    private lateinit var tweakStore: TweakStore

    private val btPermissions = if (Build.VERSION.SDK_INT >= 31) {
        arrayOf(Manifest.permission.BLUETOOTH_CONNECT, Manifest.permission.BLUETOOTH_ADVERTISE)
    } else {
        emptyArray()
    }

    private val askPermissions = registerForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) {
        hid.start()
    }

    private val askVisible = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) {}

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        WindowCompat.setDecorFitsSystemWindows(window, false)
        WindowInsetsControllerCompat(window, window.decorView).apply {
            hide(WindowInsetsCompat.Type.systemBars())
            systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        }

        discovery = Discovery(this)
        hid = HidGamepad(this)
        tweakStore = TweakStore(this)
        val prefs = getSharedPreferences("link", MODE_PRIVATE)
        val deviceName = "${Build.MANUFACTURER} ${Build.MODEL}".trim()
        // Test builds only: `adb shell am start ... --es preview snes` shows a layout without AetherPlay.
        val debuggable = applicationInfo.flags and android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE != 0
        val preview = intent.getStringExtra("preview")?.takeIf { debuggable }

        fun connect(server: Server) {
            prefs.edit().putString("server", server.name).apply()
            link.connect(server, prefs.getString("token:${server.name}", null), deviceName)
        }

        setContent {
            val servers by discovery.servers.collectAsState()
            val linkStatus by link.status.collectAsState()
            val linkGame by link.game.collectAsState()
            val game = preview?.let { fr.jolannrt.aetherplay.net.Game(it, "Aperçu : $it", null) } ?: linkGame
            val token by link.token.collectAsState()
            val btStatus by hid.status.collectAsState()
            val player by link.player.collectAsState()
            val wifiInput by link.wifiInput.collectAsState()
            var showPad by remember { mutableStateOf(preview != null) }
            var backdrop by remember {
                mutableStateOf(runCatching { Backdrop.valueOf(prefs.getString("backdrop", null)!!) }.getOrDefault(Backdrop.CONTROLLER))
            }

            // Remember the pairing token AetherPlay issued.
            LaunchedEffect(token, linkStatus) {
                val name = prefs.getString("server", null)
                if (token != null && name != null && linkStatus is Link.Status.Connected) {
                    prefs.edit().putString("token:$name", token).apply()
                }
            }
            // Reconnect to the last AetherPlay as soon as it is found again.
            LaunchedEffect(servers) {
                val last = prefs.getString("server", null)
                val current = link.status.value
                if (current is Link.Status.Disconnected || current is Link.Status.Failed) {
                    servers.firstOrNull { it.name == last }?.let { connect(it) }
                }
            }
            // Straight to the controller once something is connected.
            LaunchedEffect(linkStatus, btStatus) {
                if (linkStatus is Link.Status.Connected || btStatus is HidGamepad.Status.Connected) showPad = true
            }

            val btConnected = btStatus is HidGamepad.Status.Connected
            if (showPad) {
                val layout = layoutFor(game?.platform)
                var tweaks by remember(layout) { mutableStateOf(tweakStore.load(layout)) }
                PadScreen(
                    layout = layout,
                    player = player,
                    wifiInput = wifiInput,
                    backdrop = backdrop,
                    onBackdrop = { backdrop = it; prefs.edit().putString("backdrop", it.name).apply() },
                    tweaks = tweaks,
                    onTweaks = { tweaks = it; tweakStore.save(layout, it) },
                    game = game,
                    bluetooth = btConnected,
                    wifi = linkStatus is Link.Status.Connected,
                    onState = { state -> send(state) },
                    onQuitGame = { link.quitGame() },
                    onSettings = { showPad = false },
                )
            } else {
                ConnectScreen(
                    servers = servers,
                    link = linkStatus,
                    bluetooth = btStatus,
                    onConnect = ::connect,
                    onPin = link::sendPin,
                    onMakeVisible = {
                        askVisible.launch(
                            Intent(BluetoothAdapter.ACTION_REQUEST_DISCOVERABLE)
                                .putExtra(BluetoothAdapter.EXTRA_DISCOVERABLE_DURATION, 120),
                        )
                    },
                    onRetryBluetooth = { startBluetooth() },
                    onOpenPad = { showPad = true },
                )
            }
        }
    }

    /** Bluetooth carries the input to the games when connected; Wi-Fi always gets it (menus, fallback). */
    private fun send(state: PadState) {
        val viaBluetooth = hid.send(state)
        link.sendPad(state, viaBluetooth)
    }

    private fun startBluetooth() {
        hid.stop()
        val missing = btPermissions.filter { ContextCompat.checkSelfPermission(this, it) != PackageManager.PERMISSION_GRANTED }
        if (missing.isEmpty()) hid.start() else askPermissions.launch(missing.toTypedArray())
    }

    override fun onStart() {
        super.onStart()
        discovery.start()
        startBluetooth()
    }

    override fun onStop() {
        super.onStop()
        discovery.stop()
        hid.stop()
        link.disconnect()
    }
}
