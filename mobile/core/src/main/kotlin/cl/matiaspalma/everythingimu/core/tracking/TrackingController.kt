package cl.matiaspalma.everythingimu.core.tracking

import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.os.BatteryManager
import cl.matiaspalma.everythingimu.core.calibration.CalibrationData
import cl.matiaspalma.everythingimu.core.calibration.MagCalibration
import cl.matiaspalma.everythingimu.core.calibration.MagCalibrationSession
import cl.matiaspalma.everythingimu.core.haptics.HapticBridge
import cl.matiaspalma.everythingimu.core.fusion.Quaternion
import cl.matiaspalma.everythingimu.core.fusion.inverse
import cl.matiaspalma.everythingimu.core.fusion.times
import cl.matiaspalma.everythingimu.core.fusion.normalized
import cl.matiaspalma.everythingimu.core.fusion.VqfEngine
import cl.matiaspalma.everythingimu.core.net.ClientStats
import cl.matiaspalma.everythingimu.core.net.ConnectionState
import cl.matiaspalma.everythingimu.core.net.SlimeVrClient
import cl.matiaspalma.everythingimu.core.net.WifiBinder
import cl.matiaspalma.everythingimu.core.prefs.AppPrefs
import cl.matiaspalma.everythingimu.core.sensors.SensorRates
import cl.matiaspalma.everythingimu.core.sensors.SensorRepository
import cl.matiaspalma.everythingimu.core.sensors.SensorSnapshot
import cl.matiaspalma.everythingimu.core.sensors.Vec3
import cl.matiaspalma.everythingimu.core.service.TrackingService
import cl.matiaspalma.everythingimu.core.wearable.WearableConfigSender
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * Process-wide singleton owning sensor + fusion + network components.
 * UI reads StateFlows here regardless of whether the foreground service is bound.
 */
object TrackingController {

    private var sensors: SensorRepository? = null
    private var client: SlimeVrClient? = null
    private var prefs: AppPrefs? = null
    private var wifiBinder: WifiBinder? = null
    private var pumpJob: Job? = null
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private var initialized = false

    private val _running = MutableStateFlow(false)
    val running: StateFlow<Boolean> = _running.asStateFlow()

    private val _connection = MutableStateFlow(ConnectionState.Disconnected)
    val connection: StateFlow<ConnectionState> = _connection.asStateFlow()

    private val _clientStats = MutableStateFlow(ClientStats())
    val clientStats: StateFlow<ClientStats> = _clientStats.asStateFlow()

    private val _lastError = MutableStateFlow<String?>(null)
    val lastError: StateFlow<String?> = _lastError.asStateFlow()

    private val _tps = MutableStateFlow(0f)
    val tps: StateFlow<Float> = _tps.asStateFlow()

    private val _batteryLevel = MutableStateFlow(-1)
    val batteryLevel: StateFlow<Int> = _batteryLevel.asStateFlow()

    @Volatile var sendRateHz: Int = 100
        private set
    @Volatile var magEnabled: Boolean = true
        private set
    @Volatile var shakeEnabled: Boolean = true
        private set

    private var batteryJob: Job? = null
    private var tpsJob: Job? = null
    private var netCallback: ConnectivityManager.NetworkCallback? = null
    private var appCtx: Context? = null
    private var lastShakeNanos: Long = 0L

    val snapshot: StateFlow<SensorSnapshot>
        get() = sensors?.snapshot ?: emptySnapshot

    val rates: StateFlow<SensorRates>
        get() = sensors?.rates ?: emptyRates

    val availability: SensorAvailability
        get() = sensors?.availability ?: SensorAvailability()

    val quaternion: StateFlow<Quaternion>
        get() = sensors?.quaternion ?: emptyQuat

    private val _mountedQuaternion = MutableStateFlow(Quaternion.IDENTITY)
    val mountedQuaternion: StateFlow<Quaternion> = _mountedQuaternion.asStateFlow()

    val fusionAvailable: Boolean get() = VqfEngine.isAvailable()

    val magCalibrationSession: MagCalibrationSession = MagCalibrationSession()

    private var hapticBridge: HapticBridge? = null

    fun hapticBridge(): HapticBridge? = hapticBridge

    @Volatile private var calibration: CalibrationData = CalibrationData.EMPTY
    @Volatile private var mountOffset: Quaternion = Quaternion.IDENTITY

    fun calibration(): CalibrationData = calibration

    @Synchronized
    fun ensureInit(context: Context) {
        if (initialized) return
        initialized = true
        val app = context.applicationContext
        appCtx = app
        val appPrefs = AppPrefs(app)
        prefs = appPrefs
        wifiBinder = WifiBinder(app).also { it.bindToWifi() }
        hapticBridge = HapticBridge(app)
        val repo = SensorRepository(app)
        sensors = repo
        repo.onGyroBiasUpdated = { bias ->
            calibration = calibration.copy(gyroBias = bias)
            scope.launch { appPrefs.setGyroBias(bias) }
        }
        scope.launch {
            calibration = appPrefs.loadCalibration()
            repo.applyCalibration(calibration.gyroBias, calibration.mag)
            val uuid = appPrefs.deviceUuidOrCreate()
            val mac = SlimeVrClient.macFromUuid(uuid)
            client = SlimeVrClient(mac)
            launch { client!!.state.collectStateInto(_connection) }
            launch { client!!.stats.collectStateInto(_clientStats) }
            launch { client!!.lastError.collectStateInto(_lastError) }
        }
        scope.launch { appPrefs.sendRateHz.collect { sendRateHz = it } }
        scope.launch {
            appPrefs.magEnabled.collect {
                magEnabled = it
                sensors?.magInputEnabled = it
            }
        }
        scope.launch { appPrefs.shakeRecenter.collect { shakeEnabled = it } }
        registerNetworkCallback(app)
        startTpsCounter()
        startBatteryReporter(app)
        installShakeDetector()
    }

    private fun registerNetworkCallback(app: Context) {
        if (netCallback != null) return
        val cm = app.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager ?: return
        val req = NetworkRequest.Builder()
            .addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
            .addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
            .build()
        val cb = object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) {
                val state = client?.state?.value ?: return
                if (state == ConnectionState.Failed || state == ConnectionState.Reconnecting) {
                    scope.launch {
                        val h = savedHost()
                        val p = savedPort()
                        if (h.isNotBlank()) client?.connect(h, p)
                    }
                }
            }
        }
        netCallback = cb
        try { cm.registerNetworkCallback(req, cb) } catch (_: Throwable) {}
    }

    private fun startTpsCounter() {
        if (tpsJob != null) return
        tpsJob = scope.launch {
            var prev = 0L
            while (true) {
                delay(1000)
                val cur = client?.stats?.value?.packetsSent ?: 0L
                _tps.value = (cur - prev).coerceAtLeast(0L).toFloat()
                prev = cur
            }
        }
    }

    private fun startBatteryReporter(app: Context) {
        if (batteryJob != null) return
        batteryJob = scope.launch {
            val bm = app.getSystemService(Context.BATTERY_SERVICE) as? BatteryManager
            while (true) {
                val pct = bm?.getIntProperty(BatteryManager.BATTERY_PROPERTY_CAPACITY) ?: -1
                _batteryLevel.value = pct
                if (pct in 0..100) {
                    val voltage = readBatteryVoltage(app)
                    client?.sendBattery(voltage, pct / 100f)
                }
                delay(30_000)
            }
        }
    }

    private fun readBatteryVoltage(app: Context): Float {
        val intent = app.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))
        val mv = intent?.getIntExtra(BatteryManager.EXTRA_VOLTAGE, -1) ?: -1
        return if (mv > 0) mv / 1000f else 3.7f
    }

    private fun installShakeDetector() {
        val repo = sensors ?: return
        repo.shakeListener = { mag ->
            if (shakeEnabled && mag > SHAKE_THRESHOLD_MS2) {
                val now = System.nanoTime()
                if (now - lastShakeNanos > SHAKE_COOLDOWN_NS) {
                    lastShakeNanos = now
                    sendRecenter()
                }
            }
        }
    }

    fun beginMagCalibration() {
        val repo = sensors ?: return
        magCalibrationSession.start()
        repo.magSampleSink = { sample -> magCalibrationSession.feed(sample) }
    }

    fun cancelMagCalibration() {
        sensors?.magSampleSink = null
        magCalibrationSession.discard()
    }

    fun applyMagCalibration(): MagCalibration? {
        val result = magCalibrationSession.accept() ?: return null
        sensors?.magSampleSink = null
        sensors?.applyCalibration(calibration.gyroBias, result)
        calibration = calibration.copy(mag = result)
        scope.launch { prefs?.setMagCalibration(result) }
        return result
    }

    fun resetCalibration() {
        val identity = CalibrationData.EMPTY
        calibration = identity
        sensors?.applyCalibration(identity.gyroBias, identity.mag)
        scope.launch { prefs?.clearCalibration() }
    }

    fun currentGyroBias(): Vec3 = sensors?.gyroBiasValue() ?: calibration.gyroBias
    fun gyroBiasCalibrated(): Boolean = sensors?.gyroBiasCalibrated() == true

    fun start(context: Context) {
        ensureInit(context)
        TrackingService.start(context)
    }

    fun stop(context: Context) {
        TrackingService.stop(context)
    }

    suspend fun connect(host: String, port: Int) {
        val c = client ?: return
        c.connect(host, port)
        prefs?.setServer(host, port)
        startQuaternionPump()
    }

    fun disconnect() {
        client?.disconnect()
        pumpJob?.cancel()
        pumpJob = null
    }

    /** owoTrack-compat recenter (BUTTON_PUSHED packet 60). */
    fun sendRecenter() {
        client?.sendButtonPushed()
    }

    suspend fun persistServer(host: String, port: Int) {
        prefs?.setServer(host, port)
    }

    /**
     * Phone-side persist that also pushes the address to a paired watch over the
     * Wearable Data Layer. The wear app must NOT call this — it would re-emit the
     * DataItem its own listener just consumed and loop. Wear uses [persistServer].
     */
    suspend fun persistAndSyncServer(context: Context, host: String, port: Int) {
        prefs?.setServer(host, port)
        if (host.isNotBlank()) {
            scope.launch { WearableConfigSender.push(context, host, port) }
        }
    }

    /** Wear waits on this for a phone-pushed host before falling back to the picker. */
    fun serverHostFlow(): kotlinx.coroutines.flow.Flow<String>? = prefs?.serverHost

    suspend fun savedHost(): String = prefs?.serverHost?.first() ?: ""
    suspend fun savedPort(): Int = (prefs?.serverPort?.first() ?: 6969).let { if (it == 0) 6969 else it }
    suspend fun deviceUuid(): String = prefs?.deviceUuidOrCreate().orEmpty()

    suspend fun deviceMac(): ByteArray {
        val uuid = prefs?.deviceUuidOrCreate().orEmpty().ifBlank { DEFAULT_UUID }
        return SlimeVrClient.macFromUuid(uuid)
    }

    internal fun onServiceStart() {
        sensors?.start()
        _running.value = true
        startQuaternionPump()
    }

    internal fun onServiceStop() {
        sensors?.stop()
        _running.value = false
        pumpJob?.cancel()
        pumpJob = null
        mountOffset = Quaternion.IDENTITY
        _mountedQuaternion.value = Quaternion.IDENTITY
    }

    private fun startQuaternionPump() {
        if (pumpJob != null) return
        val flow = sensors?.quaternion ?: return
        pumpJob = scope.launch {
            var lastSendNanos = 0L
            flow.collect { q ->
                val mounted = applyMountOffset(q)
                _mountedQuaternion.value = mounted
                val intervalNs = 1_000_000_000L / sendRateHz.coerceAtLeast(20)
                val now = System.nanoTime()
                if (now - lastSendNanos >= intervalNs) {
                    client?.sendRotation(floatArrayOf(mounted.w, mounted.x, mounted.y, mounted.z))
                    lastSendNanos = now
                }
            }
        }
    }

    fun autoMount(): Boolean {
        val current = sensors?.quaternion?.value ?: return false
        mountOffset = current.inverse()
        return true
    }

    private fun applyMountOffset(raw: Quaternion): Quaternion = (mountOffset * raw).normalized()

    private suspend fun <T> StateFlow<T>.collectStateInto(target: MutableStateFlow<T>) {
        collect { target.value = it }
    }

    private val emptySnapshot = MutableStateFlow(SensorSnapshot()).asStateFlow()
    private val emptyRates = MutableStateFlow(SensorRates()).asStateFlow()
    private val emptyQuat = MutableStateFlow(Quaternion.IDENTITY).asStateFlow()

    private const val DEFAULT_UUID = "00000000-0000-0000-0000-000000000000"
    private const val SHAKE_THRESHOLD_MS2 = 25f
    private const val SHAKE_COOLDOWN_NS = 1_500_000_000L
}

data class SensorAvailability(
    val gyro: Boolean = false,
    val accel: Boolean = false,
    val mag: Boolean = false,
)
