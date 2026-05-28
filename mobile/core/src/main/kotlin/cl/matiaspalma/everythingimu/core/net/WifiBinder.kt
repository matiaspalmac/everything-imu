package cl.matiaspalma.everythingimu.core.net

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.os.Build
import android.util.Log

/**
 * Force the process to route traffic through Wi-Fi rather than Bluetooth or LTE.
 *
 * Wear OS in particular defaults to a Bluetooth proxy network when paired with
 * a phone — that proxy can't reach the SlimeVR Server on the LAN. Binding the
 * process to a Wi-Fi network ensures our UDP sockets go out the right NIC.
 */
class WifiBinder(context: Context) {

    private val cm: ConnectivityManager =
        context.applicationContext.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager

    private var callback: ConnectivityManager.NetworkCallback? = null

    fun bindToWifi() {
        if (callback != null) return
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.M) return
        val request = NetworkRequest.Builder()
            .addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
            .addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
            .build()
        val cb = object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) {
                try {
                    cm.bindProcessToNetwork(network)
                } catch (t: Throwable) {
                    Log.w("WifiBinder", "bindProcessToNetwork failed", t)
                }
            }

            override fun onLost(network: Network) {
                try {
                    cm.bindProcessToNetwork(null)
                } catch (_: Throwable) {
                    // best-effort
                }
            }
        }
        try {
            cm.requestNetwork(request, cb)
            callback = cb
        } catch (t: Throwable) {
            // CHANGE_NETWORK_STATE may be denied on some OEM ROMs; that's fine —
            // the phone is already on Wi-Fi when the user has any LAN at all.
            Log.w("WifiBinder", "requestNetwork denied; falling back to default route", t)
        }
    }

    fun release() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.M) return
        val cb = callback ?: return
        try {
            cm.unregisterNetworkCallback(cb)
        } catch (_: Throwable) {
            // best-effort
        }
        try {
            cm.bindProcessToNetwork(null)
        } catch (_: Throwable) {
            // best-effort
        }
        callback = null
    }
}
