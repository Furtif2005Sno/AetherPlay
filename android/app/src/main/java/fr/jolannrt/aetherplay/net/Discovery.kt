package fr.jolannrt.aetherplay.net

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.net.wifi.WifiManager
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/** An AetherPlay found on the local network. */
data class Server(val name: String, val host: String, val port: Int)

/** Finds AetherPlay on the local network (mDNS service `_aetherplay._tcp`). */
@Suppress("DEPRECATION") // resolveService: its replacement needs Android 14.
class Discovery(context: Context) {
    private val nsd = context.getSystemService(Context.NSD_SERVICE) as NsdManager
    private val wifi = context.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
    private var lock: WifiManager.MulticastLock? = null
    private val _servers = MutableStateFlow<List<Server>>(emptyList())
    val servers: StateFlow<List<Server>> = _servers
    private var listener: NsdManager.DiscoveryListener? = null

    fun start() {
        if (listener != null) return
        // Some phones drop multicast packets (mDNS) without this.
        lock = wifi.createMulticastLock("aetherplay").apply { setReferenceCounted(false); acquire() }
        val l = object : NsdManager.DiscoveryListener {
            override fun onServiceFound(info: NsdServiceInfo) {
                nsd.resolveService(info, object : NsdManager.ResolveListener {
                    override fun onResolveFailed(info: NsdServiceInfo, errorCode: Int) {}
                    override fun onServiceResolved(info: NsdServiceInfo) {
                        val host = info.host?.hostAddress ?: return
                        val server = Server(info.serviceName, host, info.port)
                        _servers.value = (_servers.value.filter { it.name != server.name } + server).sortedBy { it.name }
                    }
                })
            }

            override fun onServiceLost(info: NsdServiceInfo) {
                _servers.value = _servers.value.filter { it.name != info.serviceName }
            }

            override fun onDiscoveryStarted(serviceType: String) {}
            override fun onDiscoveryStopped(serviceType: String) {}
            override fun onStartDiscoveryFailed(serviceType: String, errorCode: Int) {}
            override fun onStopDiscoveryFailed(serviceType: String, errorCode: Int) {}
        }
        listener = l
        nsd.discoverServices(SERVICE_TYPE, NsdManager.PROTOCOL_DNS_SD, l)
    }

    fun stop() {
        listener?.let { runCatching { nsd.stopServiceDiscovery(it) } }
        listener = null
        // Found again on the next start, which triggers the automatic reconnection.
        _servers.value = emptyList()
        lock?.release()
        lock = null
    }

    companion object {
        const val SERVICE_TYPE = "_aetherplay._tcp."
        const val DEFAULT_PORT = 47820
    }
}
