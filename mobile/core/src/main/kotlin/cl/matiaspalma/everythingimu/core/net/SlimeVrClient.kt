package cl.matiaspalma.everythingimu.core.net

import android.content.Context
import android.net.wifi.WifiManager
import android.util.Log
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.InetSocketAddress
import java.util.concurrent.Executors
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

enum class ConnectionState { Disconnected, Connecting, Connected, Reconnecting, Failed }

data class ClientStats(
    val packetsSent: Long = 0,
    val packetsReceived: Long = 0,
    val lastHeartbeatAgeMs: Long = -1,
    val targetEndpoint: String? = null,
)

/**
 * Minimal SlimeVR client. Byte-for-byte parity with `crates/slime-tracker`
 * via [SlimeProtocol]. Owns the UDP socket, send/recv loops, and exponential
 * reconnect.
 */
class SlimeVrClient(
    private val mac: ByteArray,
    private val firmware: String = SlimeProtocol.FIRMWARE_STRING,
) {

    private val tag = "SlimeVrClient"
    private val seqCounter = AtomicLong(0)
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    // Single-threaded send pump. Every datagram is stamped with its sequence
    // number and written to the socket on this one thread, so the number on the
    // wire always matches the order packets are transmitted. SlimeVR-Server
    // rejects (throws + drops) any packet whose number is <= the last it saw
    // for this device ("Out of order packet received"); without serialization,
    // the rotation pump, heartbeat loop, and ping-echo each run on their own
    // thread and race the sequence counter against the socket write, which the
    // server sees as a constant stream of out-of-order packets.
    private val sendExecutor = Executors.newSingleThreadExecutor { r ->
        Thread(r, "slime-send").apply { isDaemon = true }
    }
    private var socket: DatagramSocket? = null
    private var endpoint: InetSocketAddress? = null
    private var recvJob: Job? = null
    private var heartbeatJob: Job? = null
    private var reconnectJob: Job? = null
    private var lastHeartbeatNanos: Long = 0L
    private var handshakeAcked: Boolean = false

    private val _state = MutableStateFlow(ConnectionState.Disconnected)
    val state: StateFlow<ConnectionState> = _state.asStateFlow()

    private val _stats = MutableStateFlow(ClientStats())
    val stats: StateFlow<ClientStats> = _stats.asStateFlow()

    private val _lastError = MutableStateFlow<String?>(null)
    val lastError: StateFlow<String?> = _lastError.asStateFlow()

    @Volatile private var sentCount: Long = 0
    @Volatile private var recvCount: Long = 0

    @Synchronized
    fun connect(host: String, port: Int) {
        disconnect()
        _state.value = ConnectionState.Connecting
        val targetEndpoint = InetSocketAddress(host, port)
        endpoint = targetEndpoint
        try {
            socket = DatagramSocket().apply { soTimeout = 250 }
            handshakeAcked = false
            lastHeartbeatNanos = 0L
            _lastError.value = null
            sendHandshake()
            recvJob = scope.launch { recvLoop() }
            heartbeatJob = scope.launch { heartbeatLoop() }
            reconnectJob = scope.launch { handshakeWatchdog() }
            updateStats(target = targetEndpoint.toString())
        } catch (t: Throwable) {
            Log.e(tag, "connect failed", t)
            _lastError.value = humanizeConnectError(t, host, port)
            _state.value = ConnectionState.Failed
            socket?.close()
            socket = null
        }
    }

    @Synchronized
    fun disconnect() {
        recvJob?.cancel(); recvJob = null
        heartbeatJob?.cancel(); heartbeatJob = null
        reconnectJob?.cancel(); reconnectJob = null
        socket?.close(); socket = null
        endpoint = null
        handshakeAcked = false
        updateLastError(null)
        _state.value = ConnectionState.Disconnected
    }

    fun shutdown() {
        disconnect()
        scope.cancel()
        sendExecutor.shutdownNow()
    }

    fun sendRotation(wxyz: FloatArray) {
        submitSend { seq -> SlimeProtocol.rotation(seq, sensorId = 0, wxyz = wxyz) }
    }

    fun sendUserAction(action: Int) {
        submitSend { seq -> SlimeProtocol.userAction(seq, action) }
    }

    /** Legacy owoTrack acceleration packet (4). Linear accel (m/s²), 3 floats. */
    fun sendAccel(xyz: FloatArray) {
        submitSend { seq -> SlimeProtocol.accel(seq, xyz) }
    }

    /** SlimeVR battery packet (12). Level 0..1. */
    fun sendBattery(voltageVolts: Float, level: Float) {
        submitSend { seq -> SlimeProtocol.battery(seq, voltageVolts, level) }
    }

    /** Legacy owoTrack recenter button. */
    fun sendButtonPushed() {
        submitSend { seq -> SlimeProtocol.buttonPushed(seq) }
    }

    private suspend fun heartbeatLoop() = withContext(Dispatchers.IO) {
        while (scope.isActive) {
            if (socket == null || endpoint == null) return@withContext
            submitSend { seq -> SlimeProtocol.heartbeat(seq) }
            delay(1000)
        }
    }

    private suspend fun recvLoop() = withContext(Dispatchers.IO) {
        val sock = socket ?: return@withContext
        val buf = ByteArray(2048)
        val pkt = DatagramPacket(buf, buf.size)
        while (scope.isActive && !sock.isClosed) {
            try {
                sock.receive(pkt)
                recvCount++
                lastHeartbeatNanos = System.nanoTime()
                handshakeAcked = true
                val tag = SlimeProtocol.peekTag(pkt.data, pkt.length)
                if (tag == SlimeProtocol.PACKET_PING) {
                    val challenge = SlimeProtocol.extractPingChallenge(pkt.data, pkt.length)
                    if (challenge != null) {
                        submitSend { seq -> SlimeProtocol.pong(seq, challenge) }
                    }
                }
                updateStats()
            } catch (_: java.net.SocketTimeoutException) {
                // expected; loop and re-arm
            } catch (t: Throwable) {
                if (!scope.isActive) break
                Log.w(this@SlimeVrClient.tag, "recv error", t)
                val ep = endpoint
                if (ep != null) {
                    updateLastError(humanizeConnectError(t, ep.address.hostAddress ?: "", ep.port))
                }
            }
        }
    }

    private suspend fun handshakeWatchdog() {
        var backoffMs = 1000L
        var misses = 0
        while (scope.isActive) {
            delay(2000)
            if (!handshakeAcked) {
                _state.value = ConnectionState.Reconnecting
                sendHandshake()
                delay(backoffMs)
                backoffMs = (backoffMs * 2).coerceAtMost(60_000)
                misses++
                if (misses >= 2) {
                    val ep = endpoint
                    updateLastError(
                        "No response from server. Check Wi-Fi, IP/port, and Windows firewall (Private network)." +
                            (ep?.let { " Tried ${it.address.hostAddress}:${it.port}." } ?: ""),
                    )
                }
            } else {
                _state.value = ConnectionState.Connected
                backoffMs = 1000L
                misses = 0
                updateLastError(null)
            }
        }
    }

    private fun sendHandshake() {
        // owoTrack-compat handshake auto-registers a single sensor on server-side,
        // so no SensorInfo packet is required (matches moveTrackVR / owoTrackVR).
        submitSend { seq -> SlimeProtocol.handshake(seq, mac, firmware) }
    }

    /**
     * Stamp a datagram with the next sequence number and write it, all on the
     * single send thread. Assigning the sequence number here (rather than at
     * the call site) is what guarantees the number on the wire matches the
     * transmission order — see [sendExecutor]. The socket/endpoint are read
     * inside the task so a concurrent [disconnect] simply drops the packet.
     */
    private fun submitSend(build: (Long) -> ByteArray) {
        if (sendExecutor.isShutdown) return
        try {
            sendExecutor.execute {
                val sock = socket ?: return@execute
                val ep = endpoint ?: return@execute
                rawSend(sock, ep, build(nextSeq()))
            }
        } catch (_: RejectedExecutionException) {
            // Executor shutting down — drop the packet.
        }
    }

    private fun rawSend(sock: DatagramSocket, ep: InetSocketAddress, bytes: ByteArray) {
        try {
            sock.send(DatagramPacket(bytes, bytes.size, ep.address, ep.port))
            sentCount++
            updateStats()
        } catch (t: Throwable) {
            Log.w(tag, "send failed", t)
            updateLastError(humanizeConnectError(t, ep.address.hostAddress ?: "", ep.port))
        }
    }

    private fun nextSeq(): Long = seqCounter.incrementAndGet()

    private fun updateStats(target: String? = null) {
        val now = System.nanoTime()
        val ageMs = if (lastHeartbeatNanos == 0L) -1L else (now - lastHeartbeatNanos) / 1_000_000
        _stats.value = ClientStats(
            packetsSent = sentCount,
            packetsReceived = recvCount,
            lastHeartbeatAgeMs = ageMs,
            targetEndpoint = target ?: _stats.value.targetEndpoint,
        )
    }

    private fun updateLastError(message: String?) {
        if (_lastError.value != message) _lastError.value = message
    }

    private fun humanizeConnectError(t: Throwable, host: String, port: Int): String {
        val msg = buildString {
            var cur: Throwable? = t
            while (cur != null) {
                append(cur.message ?: cur.javaClass.simpleName)
                append(' ')
                cur = cur.cause
            }
        }.lowercase()
        return when {
            "enetunreach" in msg || "network is unreachable" in msg ->
                "Network unreachable. Check that phone Wi-Fi is on and joined to the same LAN as the SlimeVR server."
            "ehostunreach" in msg || "no route" in msg ->
                "Host $host unreachable. Phone may be on a guest network or a different subnet than the server."
            "econnrefused" in msg || "connection refused" in msg ->
                "Server refused on port $port. Make sure SlimeVR Server is running."
            "etimedout" in msg || "timeout" in msg ->
                "Connection timed out. Check firewall on the PC (allow SlimeVR through Private network)."
            "permission denied" in msg || "eacces" in msg ->
                "Permission denied opening UDP socket."
            "unable to resolve" in msg || "unknownhost" in msg ->
                "Cannot resolve host \"$host\". Type the SlimeVR PC's IPv4 (e.g. 192.168.1.10)."
            "port unreachable" in msg ->
                "Port $port unreachable. Make sure the PC firewall allows SlimeVR Server on Private network."
            "address already in use" in msg ->
                "UDP socket already in use. Close other tracker apps or reboot the phone."
            else -> "Connect failed: ${t.message ?: t.javaClass.simpleName}"
        }
    }

    companion object {
        /** Generate a stable 6-byte MAC from the UUID persisted to DataStore. */
        fun macFromUuid(uuidHex: String): ByteArray {
            val cleaned = uuidHex.replace("-", "").ifBlank { "000000000000" }
            val padded = if (cleaned.length < 12) cleaned.padEnd(12, '0') else cleaned
            val out = ByteArray(6)
            for (i in 0 until 6) {
                val idx = (i * 2) % padded.length
                out[i] = padded.substring(idx, idx + 2).toInt(16).toByte()
            }
            // Force locally-administered + unicast bit.
            out[0] = ((out[0].toInt() and 0xFE) or 0x02).toByte()
            return out
        }

        /** Broadcast discovery: blast a handshake to known broadcast targets and wait for a reply. */
        fun discover(
            context: Context,
            mac: ByteArray,
            port: Int = 6969,
            timeoutMs: Int = 1500,
            attempts: Int = 3,
        ): InetAddress? {
            val sock = DatagramSocket()
            return try {
                sock.broadcast = true
                sock.soTimeout = timeoutMs
                val bytes = SlimeProtocol.handshake(0L, mac)
                val targets = broadcastTargets(context, port)
                repeat(attempts) {
                    for (target in targets) {
                        sock.send(DatagramPacket(bytes, bytes.size, target.address, target.port))
                    }
                    val recvBuf = ByteArray(1024)
                    val recvPkt = DatagramPacket(recvBuf, recvBuf.size)
                    try {
                        sock.receive(recvPkt)
                        return recvPkt.address
                    } catch (_: java.net.SocketTimeoutException) {
                        // retry
                    }
                }
                null
            } catch (_: Throwable) {
                null
            } finally {
                sock.close()
            }
        }

        // WifiManager.dhcpInfo is deprecated but remains the only synchronous way
        // to read the DHCP-assigned netmask needed to compute the subnet
        // broadcast address. The non-deprecated LinkProperties path is async and
        // doesn't expose the legacy netmask field, so keep the legacy call.
        @Suppress("DEPRECATION")
        private fun broadcastTargets(context: Context, port: Int): List<InetSocketAddress> {
            val targets = LinkedHashSet<InetSocketAddress>()
            targets.add(InetSocketAddress("255.255.255.255", port))
            val wifi = context.applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
            val dhcp = wifi?.dhcpInfo
            if (dhcp != null && dhcp.ipAddress != 0 && dhcp.netmask != 0) {
                val broadcast = (dhcp.ipAddress and dhcp.netmask) or dhcp.netmask.inv()
                val addr = InetAddress.getByName(intToIpv4(broadcast))
                targets.add(InetSocketAddress(addr, port))
            }
            return targets.toList()
        }

        private fun intToIpv4(value: Int): String {
            return "${value and 0xff}.${(value shr 8) and 0xff}.${(value shr 16) and 0xff}.${(value shr 24) and 0xff}"
        }
    }
}
