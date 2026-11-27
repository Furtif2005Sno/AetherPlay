package fr.jolannrt.aetherplay

import android.content.Context
import fr.jolannrt.aetherplay.bt.HidGamepad
import fr.jolannrt.aetherplay.net.Discovery
import fr.jolannrt.aetherplay.net.Link
import fr.jolannrt.aetherplay.pad.PadState

/**
 * The links to the computer, kept for the whole life of the app process
 * (ControllerService keeps it alive): switching apps or locking the phone
 * no longer drops the Bluetooth controller nor the Wi-Fi link.
 */
object Controller {
    lateinit var discovery: Discovery
        private set
    lateinit var hid: HidGamepad
        private set
    val link = Link()
    private var ready = false

    fun init(context: Context) {
        if (ready) return
        val app = context.applicationContext
        discovery = Discovery(app)
        hid = HidGamepad(app)
        ready = true
    }

    /** Bluetooth carries the input to the games when connected; Wi-Fi always gets it (menus, fallback). */
    fun send(state: PadState) {
        val viaBluetooth = hid.send(state)
        link.sendPad(state, viaBluetooth)
    }

    fun shutdown() {
        if (!ready) return
        discovery.stop()
        hid.stop()
        link.disconnect()
    }
}
