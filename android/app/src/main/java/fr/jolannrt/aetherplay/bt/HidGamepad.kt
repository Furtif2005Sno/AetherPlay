package fr.jolannrt.aetherplay.bt

import android.annotation.SuppressLint
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothHidDevice
import android.bluetooth.BluetoothHidDeviceAppQosSettings
import android.bluetooth.BluetoothHidDeviceAppSdpSettings
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothProfile
import android.content.Context
import fr.jolannrt.aetherplay.pad.PadState
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

/**
 * The phone as a Bluetooth controller (HID device profile, Android 9+): the
 * computer pairs with it like with any wireless gamepad, no driver needed on
 * Windows, macOS or SteamOS.
 */
@SuppressLint("MissingPermission") // Callers check BLUETOOTH_CONNECT first.
class HidGamepad(private val context: Context) {

    sealed interface Status {
        /** Bluetooth off, missing, or the HID device profile disabled by the phone maker. */
        data class Unavailable(val reason: String) : Status
        data object Starting : Status
        /** Registered, waiting for the computer to pair / connect. */
        data object Ready : Status
        data class Connected(val host: String) : Status
    }

    private val _status = MutableStateFlow<Status>(Status.Starting)
    val status: StateFlow<Status> = _status

    /** What AetherPlay tells over Bluetooth (no Wi-Fi needed): player number and running game. */
    data class Info(val player: Int, val platform: String?, val name: String?)

    private val _info = MutableStateFlow<Info?>(null)
    val info: StateFlow<Info?> = _info

    /** Last Bluetooth events (shown in the app, to understand a dropping link). */
    private val _events = MutableStateFlow<List<String>>(emptyList())
    val events: StateFlow<List<String>> = _events

    private fun event(text: String) {
        val time = java.text.SimpleDateFormat("HH:mm:ss", java.util.Locale.FRANCE).format(java.util.Date())
        _events.value = (_events.value + "$time  $text").takeLast(12)
    }

    /**
     * With the AetherPlay channel (game + player over Bluetooth) or as a plain
     * controller (the 0.4.2 one). Changing it needs the computer to pair again.
     */
    var extended: Boolean
        get() = prefs().getBoolean("extended", true)
        set(value) {
            prefs().edit().putBoolean("extended", value).apply()
            stop()
            start()
        }

    private val adapter: BluetoothAdapter? =
        (context.getSystemService(Context.BLUETOOTH_SERVICE) as? BluetoothManager)?.adapter
    private var hid: BluetoothHidDevice? = null
    private var host: BluetoothDevice? = null
    private val executor = Executors.newSingleThreadScheduledExecutor()
    /** Set while the profile is wanted (between start and stop): reconnect on drops. */
    @Volatile private var active = false
    @Volatile private var last = PadState.IDLE

    private val callback = object : BluetoothHidDevice.Callback() {
        override fun onAppStatusChanged(pluggedDevice: BluetoothDevice?, registered: Boolean) {
            if (!registered) {
                event("profil manette désactivé")
                _status.value = Status.Unavailable("Mode manette Bluetooth refusé par le téléphone")
                return
            }
            event(if (extended) "manette prête (avec infos AetherPlay)" else "manette prête (simple)")
            _status.value = Status.Ready
            // Reconnect to the computer used last time, or to the one that woke us.
            val target = pluggedDevice ?: lastHost()
            if (target != null) hid?.connect(target)
        }

        // The computer asks for a report: the controller state (asked by some
        // hosts when connecting: refusing it can make them drop the link), or
        // the signature AetherPlay reads to recognise the phone.
        override fun onGetReport(device: BluetoothDevice, type: Byte, id: Byte, bufferSize: Int) {
            event("le PC demande le rapport $id")
            when (id.toInt()) {
                0, REPORT_ID -> hid?.replyReport(device, type, REPORT_ID.toByte(), last.hidReport())
                SIGNATURE_REPORT -> if (extended) {
                    hid?.replyReport(device, type, id, SIGNATURE)
                } else {
                    hid?.reportError(device, BluetoothHidDevice.ERROR_RSP_INVALID_RPT_ID)
                }
                else -> hid?.reportError(device, BluetoothHidDevice.ERROR_RSP_INVALID_RPT_ID)
            }
        }

        override fun onSetReport(device: BluetoothDevice, type: Byte, id: Byte, data: ByteArray) {
            if (id.toInt() == INFO_REPORT) readInfo(data)
            hid?.reportError(device, BluetoothHidDevice.ERROR_RSP_SUCCESS)
        }

        override fun onInterruptData(device: BluetoothDevice, reportId: Byte, data: ByteArray) {
            if (reportId.toInt() == INFO_REPORT) readInfo(data)
        }

        override fun onVirtualCableUnplug(device: BluetoothDevice) {
            event("le PC a oublié la manette (association supprimée)")
        }

        override fun onConnectionStateChanged(device: BluetoothDevice, state: Int) {
            when (state) {
                BluetoothProfile.STATE_CONNECTED -> {
                    host = device
                    saveHost(device)
                    event("connectée à ${device.name ?: device.address}")
                    _status.value = Status.Connected(device.name ?: device.address)
                }
                BluetoothProfile.STATE_CONNECTING -> event("connexion à ${device.name ?: device.address}…")
                BluetoothProfile.STATE_DISCONNECTED -> {
                    event("déconnectée de ${device.name ?: device.address}")
                    if (host == device) host = null
                    _info.value = null
                    if (_status.value is Status.Connected) _status.value = Status.Ready
                    // Dropped (sleep, range, Windows power saving): call the computer back.
                    if (active && device.address == lastHost()?.address) {
                        executor.schedule({ if (active && host == null) hid?.connect(device) }, 3, TimeUnit.SECONDS)
                    }
                }
            }
        }
    }

    fun start() {
        // Already registered: keep the link (re-registering drops the computer).
        if (active && hid != null) return
        active = true
        val a = adapter ?: run {
            _status.value = Status.Unavailable("Pas de Bluetooth sur ce téléphone")
            return
        }
        if (!a.isEnabled) {
            _status.value = Status.Unavailable("Bluetooth désactivé")
            return
        }
        val ok = a.getProfileProxy(context, object : BluetoothProfile.ServiceListener {
            override fun onServiceConnected(profile: Int, proxy: BluetoothProfile) {
                hid = proxy as BluetoothHidDevice
                val sdp = BluetoothHidDeviceAppSdpSettings(
                    NAME, "Manette AetherPlay", "AetherPlay",
                    BluetoothHidDevice.SUBCLASS2_GAMEPAD, if (extended) DESCRIPTOR else BASE_DESCRIPTOR,
                )
                // Steady reports channel: Windows keeps the link instead of dropping it.
                val qos = BluetoothHidDeviceAppQosSettings(
                    BluetoothHidDeviceAppQosSettings.SERVICE_BEST_EFFORT, 800, 9, 0, 11250,
                    BluetoothHidDeviceAppQosSettings.MAX,
                )
                if (!proxy.registerApp(sdp, null, qos, executor, callback)) {
                    _status.value = Status.Unavailable("Mode manette Bluetooth indisponible sur ce téléphone")
                }
            }

            override fun onServiceDisconnected(profile: Int) {
                hid = null
                _status.value = Status.Unavailable("Service Bluetooth arrêté")
            }
        }, BluetoothProfile.HID_DEVICE)
        if (!ok) _status.value = Status.Unavailable("Mode manette Bluetooth non pris en charge")
    }

    fun stop() {
        active = false
        hid?.let {
            host?.let { h -> it.disconnect(h) }
            it.unregisterApp()
            adapter?.closeProfileProxy(BluetoothProfile.HID_DEVICE, it)
        }
        hid = null
        host = null
    }

    /** Sends the state to the computer; false when no computer is connected. */
    fun send(state: PadState): Boolean {
        last = state
        val h = host ?: return false
        return hid?.sendReport(h, REPORT_ID, state.hidReport()) == true
    }

    init {
        // Keep-alive: repeat the current state every 2 s, so an idle controller
        // is not dropped by the computer.
        executor.scheduleWithFixedDelay({
            val h = host
            if (active && h != null) runCatching { hid?.sendReport(h, REPORT_ID, last.hidReport()) }
        }, 2, 2, TimeUnit.SECONDS)
    }

    /** Info report: version, player (0 = unknown), platform (ASCII, 16 bytes), game name (UTF-8, 13 bytes). */
    private fun readInfo(data: ByteArray) {
        // Some stacks keep the report id in front.
        val d = if (data.size == INFO_SIZE + 1 && data[0].toInt() == INFO_REPORT) data.copyOfRange(1, data.size) else data
        if (d.size < 2 || d[0].toInt() != 1) return
        fun text(from: Int, len: Int): String? {
            if (d.size <= from) return null
            val bytes = d.copyOfRange(from, minOf(d.size, from + len))
            val end = bytes.indexOf(0).let { if (it < 0) bytes.size else it }
            return String(bytes, 0, end, Charsets.UTF_8).takeIf { it.isNotEmpty() }
        }
        val info = Info(d[1].toInt() and 0xFF, text(2, 16), text(18, INFO_SIZE - 18))
        if (info != _info.value) event("AetherPlay : joueur ${info.player}, ${info.platform ?: "menus"}")
        _info.value = info
    }

    private fun prefs() = context.getSharedPreferences("bt", Context.MODE_PRIVATE)
    private fun saveHost(d: BluetoothDevice) = prefs().edit().putString("host", d.address).apply()
    private fun lastHost(): BluetoothDevice? =
        prefs().getString("host", null)?.let { addr -> runCatching { adapter?.getRemoteDevice(addr) }.getOrNull() }

    companion object {
        /** HID service name (the computer lists the phone under the phone's own name). */
        const val NAME = "AetherPlay Manette"
        private const val REPORT_ID = 1
        /** Output report: AetherPlay -> phone (player number, running game). */
        private const val INFO_REPORT = 2
        private const val INFO_SIZE = 31
        /** Feature report read by AetherPlay to recognise this controller. */
        private const val SIGNATURE_REPORT = 3
        val SIGNATURE = "AETHERPLAY-PAD01".toByteArray(Charsets.US_ASCII)

        /** The controller part, common to both descriptors. */
        private val PAD = byteArrayOf(
            0x05, 0x01, //       Usage Page (Generic Desktop)
            0x09, 0x05, //       Usage (Game Pad)
            0xA1.toByte(), 0x01, // Collection (Application)
            0x85.toByte(), REPORT_ID.toByte(), // Report ID
            0x05, 0x09, //         Usage Page (Button)
            0x19, 0x01, //         Usage Minimum (1)
            0x29, 0x11, //         Usage Maximum (17)
            0x15, 0x00, //         Logical Minimum (0)
            0x25, 0x01, //         Logical Maximum (1)
            0x75, 0x01, //         Report Size (1)
            0x95.toByte(), 0x11, // Report Count (17)
            0x81.toByte(), 0x02, // Input (Data, Variable, Absolute)
            0x75, 0x01, //         Report Size (1)
            0x95.toByte(), 0x07, // Report Count (7): padding to 3 bytes
            0x81.toByte(), 0x03, // Input (Constant)
            0x05, 0x01, //         Usage Page (Generic Desktop)
            0x09, 0x30, //         Usage (X)
            0x09, 0x31, //         Usage (Y)
            0x09, 0x33, //         Usage (Rx)
            0x09, 0x34, //         Usage (Ry)
            0x15, 0x81.toByte(), // Logical Minimum (-127)
            0x25, 0x7F, //         Logical Maximum (127)
            0x75, 0x08, //         Report Size (8)
            0x95.toByte(), 0x04, // Report Count (4)
            0x81.toByte(), 0x02, // Input (Data, Variable, Absolute)
        )

        /**
         * Gamepad with 17 buttons in W3C standard order (so browsers and RomM's
         * interface read them right without a mapping) and 4 axes: X/Y left
         * stick, Rx/Ry right stick, each -127..127.
         */
        val BASE_DESCRIPTOR = PAD + byteArrayOf(0xC0.toByte())

        /** Same controller plus the AetherPlay channel. */
        val DESCRIPTOR = PAD + byteArrayOf(
            // AetherPlay channel (vendor page): signature to read, info to receive.
            0x06, 0x00, 0xFF.toByte(), // Usage Page (Vendor 0xFF00)
            0x15, 0x00, //         Logical Minimum (0)
            0x26, 0xFF.toByte(), 0x00, // Logical Maximum (255)
            0x75, 0x08, //         Report Size (8)
            0x85.toByte(), SIGNATURE_REPORT.toByte(), // Report ID (3)
            0x09, 0x01, //         Usage (1)
            0x95.toByte(), 0x10, // Report Count (16)
            0xB1.toByte(), 0x02, // Feature (Data, Variable, Absolute)
            0x85.toByte(), INFO_REPORT.toByte(), // Report ID (2)
            0x09, 0x02, //         Usage (2)
            0x95.toByte(), INFO_SIZE.toByte(), // Report Count (63)
            0x91.toByte(), 0x02, // Output (Data, Variable, Absolute)
            0xC0.toByte(), //    End Collection
        )
    }
}
