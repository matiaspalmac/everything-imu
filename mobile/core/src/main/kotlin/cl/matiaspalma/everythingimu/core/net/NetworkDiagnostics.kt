package cl.matiaspalma.everythingimu.core.net

import android.annotation.SuppressLint
import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.wifi.WifiManager
import android.os.Build
import androidx.core.content.getSystemService
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetSocketAddress

data class DiagnosticsReport(
    val wifiConnected: Boolean,
    val wifiSsid: String?,
    val localIp: String?,
    val hostReachable: Boolean,
    val serverResponded: Boolean,
    val hints: List<String>,
)

/**
 * One-shot network sanity check. The owoTrack discord is full of users
 * stuck on "Connection failed" because their phone joined a guest network,
 * the PC was on Ethernet behind a different subnet, or the firewall blocked
 * UDP. This surfaces those conditions in plain language.
 */
object NetworkDiagnostics {

    @SuppressLint("MissingPermission")
    suspend fun run(
        context: Context,
        host: String,
        port: Int,
        mac: ByteArray,
    ): DiagnosticsReport {
        val cm = context.getSystemService<ConnectivityManager>()
        val wifi = context.getSystemService<WifiManager>()
        val wifiConnected = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
            val active = cm?.activeNetwork
            val caps = active?.let { cm.getNetworkCapabilities(it) }
            caps?.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) == true
        } else {
            wifi?.isWifiEnabled == true && (wifi.connectionInfo?.networkId ?: -1) != -1
        }
        val ssid = wifi?.connectionInfo?.ssid?.trim('"').takeIf { !it.isNullOrBlank() && it != "<unknown ssid>" }
        val localIp = wifi?.connectionInfo?.ipAddress?.takeIf { it != 0 }?.let { ip ->
            "${ip and 0xff}.${(ip shr 8) and 0xff}.${(ip shr 16) and 0xff}.${(ip shr 24) and 0xff}"
        }

        val (hostReachable, serverResponded) = if (host.isBlank()) {
            false to false
        } else {
            probeServer(host, port, mac)
        }

        val hints = buildList {
            if (!wifiConnected) add("Phone is not on Wi-Fi. Tracking needs LAN, mobile data won't reach the PC.")
            if (wifiConnected && localIp == null) add("Wi-Fi connected but no IPv4 address. Network may be captive.")
            if (host.isNotBlank() && !hostReachable) add("Cannot send UDP to $host:$port. Check the address is the PC's IPv4.")
            if (hostReachable && !serverResponded) add("Server didn't reply. Make sure SlimeVR Server is running and the Windows firewall allows it (Private network).")
            if (localIp != null && host.isNotBlank() && !sameSubnet24(localIp, host)) {
                add("Phone IP ($localIp) and server IP ($host) look like different /24 subnets — guest network or VLAN isolation likely.")
            }
        }

        return DiagnosticsReport(
            wifiConnected = wifiConnected,
            wifiSsid = ssid,
            localIp = localIp,
            hostReachable = hostReachable,
            serverResponded = serverResponded,
            hints = hints,
        )
    }

    private fun probeServer(host: String, port: Int, mac: ByteArray): Pair<Boolean, Boolean> {
        val sock = DatagramSocket()
        return try {
            sock.soTimeout = 1500
            val handshake = SlimeProtocol.handshake(0L, mac)
            val addr = InetSocketAddress(host, port).address ?: return false to false
            sock.send(DatagramPacket(handshake, handshake.size, addr, port))
            val buf = ByteArray(1024)
            val recv = DatagramPacket(buf, buf.size)
            sock.receive(recv)
            true to true
        } catch (_: java.net.SocketTimeoutException) {
            true to false
        } catch (_: Throwable) {
            false to false
        } finally {
            sock.close()
        }
    }

    private fun sameSubnet24(a: String, b: String): Boolean {
        val pa = a.split(".")
        val pb = b.split(".")
        if (pa.size != 4 || pb.size != 4) return true // can't compare — don't warn
        return pa[0] == pb[0] && pa[1] == pb[1] && pa[2] == pb[2]
    }
}
