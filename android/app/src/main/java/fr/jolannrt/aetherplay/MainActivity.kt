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
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import kotlinx.coroutines.launch
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
import fr.jolannrt.aetherplay.net.Link
import fr.jolannrt.aetherplay.net.Server
import fr.jolannrt.aetherplay.pad.PadState
import fr.jolannrt.aetherplay.pad.TweakStore
import fr.jolannrt.aetherplay.pad.layoutFor
import fr.jolannrt.aetherplay.ui.Backdrop
import fr.jolannrt.aetherplay.ui.ConnectScreen
import fr.jolannrt.aetherplay.ui.PadScreen

class MainActivity : ComponentActivity() {
    // The links live in Controller, kept running by ControllerService.
    private val discovery get() = Controller.discovery
    private val link get() = Controller.link
    private val hid get() = Controller.hid
    private lateinit var tweakStore: TweakStore

    private val btPermissions = buildList {
        if (Build.VERSION.SDK_INT >= 31) {
            add(Manifest.permission.BLUETOOTH_CONNECT)
            add(Manifest.permission.BLUETOOTH_ADVERTISE)
        }
        // The "manette connectée" notification of the background service.
        if (Build.VERSION.SDK_INT >= 33) add(Manifest.permission.POST_NOTIFICATIONS)
    }

    private val askPermissions = registerForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) {
        hid.start()
        Controller.btLink.start()
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

        Controller.init(this)
        tweakStore = TweakStore(this)
        val prefs = getSharedPreferences("link", MODE_PRIVATE)
        val deviceName = "${Build.MANUFACTURER} ${Build.MODEL}".trim()
        // Test builds only: `adb shell am start ... --es preview snes` shows a layout without AetherPlay.
        val debuggable = applicationInfo.flags and android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE != 0
        val preview = intent.getStringExtra("preview")?.takeIf { debuggable }
        // `--es screen 256x192`: a fake second screen (test pattern) in the preview.
        val previewScreen = intent.getStringExtra("screen")?.takeIf { debuggable }
            ?.split("x")?.mapNotNull { it.toIntOrNull() }?.takeIf { it.size == 2 }?.let { it[0] to it[1] }
        val previewFrame = previewScreen?.let { (w, h) ->
            android.graphics.Bitmap.createBitmap(w, h, android.graphics.Bitmap.Config.ARGB_8888).apply {
                for (y in 0 until h) for (x in 0 until w) {
                    setPixel(x, y, android.graphics.Color.rgb(x * 255 / w, y * 255 / h, if ((x / 16 + y / 16) % 2 == 0) 200 else 60))
                }
            }
        }

        fun connect(server: Server) {
            prefs.edit().putString("server", server.name).apply()
            link.connect(server, prefs.getString("token:${server.name}", null), deviceName)
        }

        setContent {
            val servers by discovery.servers.collectAsState()
            val linkStatus by link.status.collectAsState()
            val linkGame by link.game.collectAsState()
            val linkStatusNow by link.status.collectAsState()
            // Bluetooth only (no Wi-Fi link): AetherPlay sends game and player over Bluetooth.
            val btInfo by hid.info.collectAsState()
            val btEvents by hid.events.collectAsState()
            var btExtended by remember { mutableStateOf(hid.extended) }
            var btSerial by remember { mutableStateOf(Controller.btLink.enabled) }
            val wifiUp = linkStatusNow is Link.Status.Connected
            // Bluetooth serial channel (with cover), else the experimental HID channel.
            val btLinkUp by Controller.btLink.connected.collectAsState()
            val btLinkGame by Controller.btLink.game.collectAsState()
            val btLinkPlayer by Controller.btLink.player.collectAsState()
            // Through the RomM server when there is no Wi-Fi link (Bluetooth only).
            val relayLinked by Controller.relay.linked.collectAsState()
            val relayGame by Controller.relay.game.collectAsState()
            val relayPlayer by Controller.relay.player.collectAsState()
            val relayProblem by Controller.relay.problem.collectAsState()
            LaunchedEffect(relayLinked, wifiUp) { Controller.relay.setActive(relayLinked && !wifiUp) }
            val btGame = when {
                btLinkUp -> btLinkGame
                relayLinked -> relayGame
                else -> btInfo?.platform?.let { fr.jolannrt.aetherplay.net.Game(it, btInfo?.name ?: it, null) }
            }
            val game = preview?.let { fr.jolannrt.aetherplay.net.Game(if (it == "menu") "" else it, "Aperçu : $it", null) }
                ?: if (wifiUp) linkGame else btGame
            val token by link.token.collectAsState()
            val btStatus by hid.status.collectAsState()
            val linkPlayer by link.player.collectAsState()
            val player = when {
                wifiUp -> linkPlayer
                btLinkUp -> btLinkPlayer
                relayLinked && relayPlayer > 0 -> relayPlayer
                else -> btInfo?.player ?: 0
            }
            val wifiInput by link.wifiInput.collectAsState()
            val linkScreen by link.screen.collectAsState()
            val linkFrame by link.frame.collectAsState()
            val screenSize = previewScreen ?: linkScreen
            val frame = previewFrame ?: linkFrame
            var showPad by remember { mutableStateOf(preview != null) }
            var buttonStyle by remember {
                mutableStateOf(runCatching { fr.jolannrt.aetherplay.ui.ButtonStyle.valueOf((intent.getStringExtra("style")?.takeIf { debuggable } ?: prefs.getString("button_style", null))!!) }
                    .getOrDefault(fr.jolannrt.aetherplay.ui.ButtonStyle.ORIGINAL))
            }
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
            // Update of the app from the GitHub releases, offered once per start.
            var update by remember { mutableStateOf<AppUpdate.Release?>(null) }
            var updating by remember { mutableStateOf(false) }
            val scope = rememberCoroutineScope()
            LaunchedEffect(Unit) { if (preview == null) update = AppUpdate.check(this@MainActivity) }
            update?.let { release ->
                AlertDialog(
                    onDismissRequest = { update = null },
                    title = { Text("Mise à jour disponible") },
                    text = { Text(if (updating) "Téléchargement de la version ${release.version}…" else "La version ${release.version} de l'app AetherPlay est disponible.") },
                    confirmButton = {
                        TextButton(enabled = !updating, onClick = {
                            updating = true
                            scope.launch {
                                runCatching { AppUpdate.install(this@MainActivity, release) }
                                updating = false
                                update = null
                            }
                        }) { Text("Installer") }
                    },
                    dismissButton = { TextButton(onClick = { update = null }) { Text("Plus tard") } },
                )
            }
            // One controller per phone: AetherPlay's virtual one when it can create
            // it (Bluetooth would add an idle player), else the Bluetooth one.
            val useBluetooth by link.useBluetooth.collectAsState()
            LaunchedEffect(useBluetooth) {
                when (useBluetooth) {
                    false -> hid.stop()
                    true -> startBluetooth()
                    null -> {}
                }
            }
            // Straight to the controller once something is connected.
            LaunchedEffect(linkStatus, btStatus) {
                if (linkStatus is Link.Status.Connected || btStatus is HidGamepad.Status.Connected) showPad = true
            }

            val btConnected = btStatus is HidGamepad.Status.Connected
            if (showPad) {
                val layout = layoutFor(game?.platform, screen = screenSize != null)
                var tweaks by remember(layout) { mutableStateOf(tweakStore.load(layout)) }
                PadScreen(
                    layout = layout,
                    player = player,
                    wifiInput = wifiInput,
                    notice = relayProblem.takeIf { relayLinked && !wifiUp },
                    backdrop = backdrop,
                    buttonStyle = buttonStyle,
                    onButtonStyle = { buttonStyle = it; prefs.edit().putString("button_style", it.name).apply() },
                    frame = frame,
                    screenSize = screenSize,
                    onTouch = link::sendTouch,
                    onBackdrop = { backdrop = it; prefs.edit().putString("backdrop", it.name).apply() },
                    tweaks = tweaks,
                    onTweaks = { tweaks = it; tweakStore.save(layout, it) },
                    game = game,
                    bluetooth = btConnected,
                    wifi = linkStatus is Link.Status.Connected,
                    onState = { state -> send(state) },
                    onQuitGame = { if (wifiUp) link.quitGame() else Controller.btLink.quitGame() },
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
                    onRetryBluetooth = { startBluetooth(restart = true) },
                    onOpenPad = { showPad = true },
                    btEvents = btEvents,
                    btExtended = btExtended,
                    onBtExtended = { hid.extended = it; btExtended = it },
                    btSerial = btSerial,
                    serverLinked = relayLinked,
                    serverUrl = Controller.relay.server,
                    onServerLink = { url, code, done ->
                        scope.launch { done(Controller.relay.link(url, code).exceptionOrNull()?.message) }
                    },
                    onServerUnlink = { Controller.relay.unlink() },
                    onBtSerial = { Controller.btLink.enabled = it; btSerial = it },
                )
            }
        }
    }

    private fun send(state: PadState) = Controller.send(state)

    /** Registers the Bluetooth controller once (not again on every return to the app). */
    private fun startBluetooth(restart: Boolean = false) {
        // AetherPlay feeds the games through a virtual controller: no Bluetooth one.
        if (link.useBluetooth.value == false && !restart) return
        if (restart) hid.stop()
        val missing = btPermissions.filter { ContextCompat.checkSelfPermission(this, it) != PackageManager.PERMISSION_GRANTED }
        if (missing.isEmpty()) {
            hid.start()
            Controller.btLink.start()
        } else {
            askPermissions.launch(missing.toTypedArray())
        }
    }

    override fun onStart() {
        super.onStart()
        // Keeps Bluetooth and Wi-Fi connected when the app is left (notification).
        ControllerService.start(this)
        startBluetooth()
    }
}
