package cl.matiaspalma.everythingimu.core.sensors

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.os.Handler
import android.os.HandlerThread
import cl.matiaspalma.everythingimu.core.calibration.GyroBiasEstimator
import cl.matiaspalma.everythingimu.core.calibration.MagCalibration
import cl.matiaspalma.everythingimu.core.fusion.Quaternion
import cl.matiaspalma.everythingimu.core.fusion.VqfEngine
import cl.matiaspalma.everythingimu.core.tracking.SensorAvailability
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

class SensorRepository(context: Context) : SensorEventListener {

    private val sensorManager: SensorManager =
        context.applicationContext.getSystemService(Context.SENSOR_SERVICE) as SensorManager

    private val gyroSensor: Sensor? = sensorManager.preferred(
        Sensor.TYPE_GYROSCOPE, Sensor.TYPE_GYROSCOPE_UNCALIBRATED,
    )
    private val accelSensor: Sensor? = sensorManager.preferred(
        Sensor.TYPE_ACCELEROMETER, Sensor.TYPE_ACCELEROMETER_UNCALIBRATED,
    )
    private val magSensor: Sensor? = sensorManager.preferred(
        Sensor.TYPE_MAGNETIC_FIELD, Sensor.TYPE_MAGNETIC_FIELD_UNCALIBRATED,
    )

    // The device's own fused orientation (6D, no mag — same basis as owoTrack).
    // Used when the user selects OS fusion instead of the on-device VQF.
    private val rotationVectorSensor: Sensor? =
        sensorManager.getDefaultSensor(Sensor.TYPE_GAME_ROTATION_VECTOR)

    private val handlerThread = HandlerThread("everythingimu-sensors").apply { start() }
    private val handler = Handler(handlerThread.looper)

    private val _snapshot = MutableStateFlow(
        SensorSnapshot(
            gyroAvailable = gyroSensor != null,
            accelAvailable = accelSensor != null,
            magAvailable = magSensor != null,
        ),
    )
    val snapshot: StateFlow<SensorSnapshot> = _snapshot.asStateFlow()

    private val _rates = MutableStateFlow(SensorRates())
    val rates: StateFlow<SensorRates> = _rates.asStateFlow()

    private val _quaternion = MutableStateFlow(Quaternion.IDENTITY)
    val quaternion: StateFlow<Quaternion> = _quaternion.asStateFlow()

    val availability: SensorAvailability = SensorAvailability(
        gyro = gyroSensor != null,
        accel = accelSensor != null,
        mag = magSensor != null,
    )

    @Volatile var fusionEnabled: Boolean = true

    /**
     * When true, orientation comes from the OS Game Rotation Vector (what
     * owoTrack uses) instead of the on-device VQF. Some devices — notably
     * Galaxy Watches — ship a well-tuned fusion that beats VQF on raw data at
     * the watch's low gyro rate. VQF stays the default (no vendor dead-zones,
     * works everywhere). [rotationVectorAvailable] reports hardware support.
     */
    @Volatile var useOsRotation: Boolean = false

    val rotationVectorAvailable: Boolean = rotationVectorSensor != null

    private var engine: VqfEngine? = null

    /**
     * Measured gyro delivery rate (Hz). VQF integrates with a fixed timestep
     * `1/rate`; if it doesn't match the device's true rate the filter
     * under-rotates and mis-tunes its bias/rest filters (drift). Phones run
     * ~200-500 Hz, watches ~50-100 Hz, so we measure the real rate from
     * hardware timestamps before building the engine instead of assuming one.
     */
    @Volatile var measuredRateHz: Double = 0.0
        private set

    private var warmupCount = 0
    private var warmupFirstTs = 0L

    private val gyroBias = GyroBiasEstimator()

    @Volatile private var magCalibration: MagCalibration = MagCalibration.IDENTITY

    /** Listener fired whenever the gyro estimator commits a new bias (UI persists it). */
    @Volatile var onGyroBiasUpdated: ((Vec3) -> Unit)? = null

    /** Tap into the raw magnetometer stream (for figure-8 calibration UI). */
    @Volatile var magSampleSink: ((Vec3) -> Unit)? = null

    /** Receives accel magnitude (m/s²) for shake detection. */
    @Volatile var shakeListener: ((Float) -> Unit)? = null

    /** When false, fusion ignores magnetometer input even if present. */
    @Volatile var magInputEnabled: Boolean = true

    fun applyCalibration(bias: Vec3, mag: MagCalibration) {
        gyroBias.seed(bias)
        magCalibration = mag
    }

    fun gyroBiasCalibrated(): Boolean = gyroBias.isCalibrated()
    fun gyroBiasValue(): Vec3 = gyroBias.bias()
    fun magCalibrationValue(): MagCalibration = magCalibration

    private var latestAccel: FloatArray? = null
    private var latestMag: FloatArray? = null

    // Linear acceleration (gravity removed) for the owoTrack-compat accel
    // packet. A slow low-pass tracks gravity; movement is the residual.
    private val gravityLp = FloatArray(3)
    private var gravityInit = false
    @Volatile private var linearAccel: FloatArray? = null

    fun latestLinearAccel(): FloatArray? = linearAccel

    private fun updateLinearAccel(a: FloatArray) {
        if (!gravityInit) {
            gravityLp[0] = a[0]; gravityLp[1] = a[1]; gravityLp[2] = a[2]
            gravityInit = true
        } else {
            gravityLp[0] = GRAVITY_LP_ALPHA * gravityLp[0] + (1f - GRAVITY_LP_ALPHA) * a[0]
            gravityLp[1] = GRAVITY_LP_ALPHA * gravityLp[1] + (1f - GRAVITY_LP_ALPHA) * a[1]
            gravityLp[2] = GRAVITY_LP_ALPHA * gravityLp[2] + (1f - GRAVITY_LP_ALPHA) * a[2]
        }
        linearAccel = floatArrayOf(a[0] - gravityLp[0], a[1] - gravityLp[1], a[2] - gravityLp[2])
    }

    private var gyroCount = 0
    private var accelCount = 0
    private var magCount = 0
    private var rateWindowStartNanos = 0L
    private var running = false

    // UI snapshot is emitted at a capped rate. Sensors arrive up to ~400 Hz;
    // pushing every sample into the StateFlow makes Compose recompose at sensor
    // rate and the UI janks. Fusion and networking read the raw samples
    // directly, so throttling only the display flow costs nothing in accuracy.
    private var pendingSnapshot: SensorSnapshot = _snapshot.value
    private var lastSnapshotEmitNanos = 0L

    fun start() {
        if (running) return
        running = true
        // Engine is created lazily once the real gyro rate is measured (see
        // maybeInitEngine). Reset warmup only when there is no engine yet so a
        // stop/start cycle keeps the existing orientation.
        if (engine == null) {
            warmupCount = 0
            warmupFirstTs = 0L
        }
        gyroSensor?.let {
            sensorManager.registerListener(this, it, SensorManager.SENSOR_DELAY_FASTEST, handler)
        }
        accelSensor?.let {
            sensorManager.registerListener(this, it, SensorManager.SENSOR_DELAY_FASTEST, handler)
        }
        magSensor?.let {
            sensorManager.registerListener(this, it, SensorManager.SENSOR_DELAY_FASTEST, handler)
        }
        // Always register the rotation vector when present: it's cheap (OS
        // already computes it) and lets the user switch fusion source live.
        rotationVectorSensor?.let {
            sensorManager.registerListener(this, it, SensorManager.SENSOR_DELAY_FASTEST, handler)
        }
    }

    fun stop() {
        if (!running) return
        running = false
        sensorManager.unregisterListener(this)
    }

    fun release() {
        stop()
        engine?.close()
        engine = null
        handlerThread.quitSafely()
    }

    override fun onSensorChanged(event: SensorEvent) {
        val v = Vec3(event.values[0], event.values[1], event.values[2])
        val ts = event.timestamp
        val acc = event.accuracy
        when (event.sensor.type) {
            Sensor.TYPE_GYROSCOPE -> {
                gyroCount++
                maybeInitEngine(ts)
                val corrected = applyGyroBias(v, ts)
                pendingSnapshot = pendingSnapshot.copy(gyro = SensorSample(corrected, ts, acc, false))
                emitSnapshot(ts)
                runFusion(corrected)
            }
            Sensor.TYPE_GYROSCOPE_UNCALIBRATED -> {
                gyroCount++
                maybeInitEngine(ts)
                val corrected = applyGyroBias(v, ts)
                pendingSnapshot = pendingSnapshot.copy(gyro = SensorSample(corrected, ts, acc, true))
                emitSnapshot(ts)
                runFusion(corrected)
            }
            Sensor.TYPE_ACCELEROMETER -> {
                accelCount++
                pendingSnapshot = pendingSnapshot.copy(accel = SensorSample(v, ts, acc, false))
                emitSnapshot(ts)
                latestAccel = event.values.copyOf(3)
                updateLinearAccel(event.values)
                shakeListener?.invoke(kotlin.math.sqrt(v.x * v.x + v.y * v.y + v.z * v.z))
            }
            Sensor.TYPE_ACCELEROMETER_UNCALIBRATED -> {
                accelCount++
                pendingSnapshot = pendingSnapshot.copy(accel = SensorSample(v, ts, acc, true))
                emitSnapshot(ts)
                latestAccel = event.values.copyOf(3)
                updateLinearAccel(event.values)
                shakeListener?.invoke(kotlin.math.sqrt(v.x * v.x + v.y * v.y + v.z * v.z))
            }
            Sensor.TYPE_MAGNETIC_FIELD -> {
                magCount++
                magSampleSink?.invoke(v)
                val calibrated = magCalibration.transform(v)
                pendingSnapshot = pendingSnapshot.copy(mag = SensorSample(calibrated, ts, acc, false))
                emitSnapshot(ts)
                latestMag = floatArrayOf(calibrated.x, calibrated.y, calibrated.z)
            }
            Sensor.TYPE_MAGNETIC_FIELD_UNCALIBRATED -> {
                magCount++
                magSampleSink?.invoke(v)
                val calibrated = magCalibration.transform(v)
                pendingSnapshot = pendingSnapshot.copy(mag = SensorSample(calibrated, ts, acc, true))
                emitSnapshot(ts)
                latestMag = floatArrayOf(calibrated.x, calibrated.y, calibrated.z)
            }
            Sensor.TYPE_GAME_ROTATION_VECTOR -> {
                if (useOsRotation) {
                    val q = FloatArray(4)
                    SensorManager.getQuaternionFromVector(q, event.values)
                    _quaternion.value = Quaternion(q[0], q[1], q[2], q[3])
                }
            }
        }
        maybeFlushRates(ts)
    }

    private fun emitSnapshot(nowNanos: Long) {
        if (nowNanos - lastSnapshotEmitNanos >= UI_SNAPSHOT_INTERVAL_NANOS) {
            _snapshot.value = pendingSnapshot
            lastSnapshotEmitNanos = nowNanos
        }
    }

    private fun applyGyroBias(raw: Vec3, ts: Long): Vec3 {
        val wasCalibrated = gyroBias.isCalibrated()
        val corrected = gyroBias.apply(raw, ts)
        if (!wasCalibrated && gyroBias.isCalibrated()) {
            onGyroBiasUpdated?.invoke(gyroBias.bias())
        }
        return corrected
    }

    /**
     * Build the VQF engine once we've measured the device's true gyro rate.
     * The first few samples after registration can arrive in a batch with
     * bogus spacing, so they're skipped; the rate is then the sample count
     * over the hardware-timestamp span, clamped to a sane band.
     */
    private fun maybeInitEngine(ts: Long) {
        if (engine != null || !fusionEnabled || ts <= 0L) return
        warmupCount++
        if (warmupCount <= WARMUP_SKIP) {
            warmupFirstTs = ts
            return
        }
        val span = ts - warmupFirstTs
        val intervals = warmupCount - WARMUP_SKIP
        if (intervals < WARMUP_MIN_INTERVALS || span < WARMUP_MIN_SPAN_NS) return
        val rate = (intervals.toDouble() * 1_000_000_000.0 / span.toDouble())
            .coerceIn(MIN_RATE_HZ, MAX_RATE_HZ)
        measuredRateHz = rate
        engine = VqfEngine.create(sampleRateHz = rate)
    }

    private fun runFusion(gyroCorrected: Vec3) {
        if (useOsRotation) return
        val eng = engine ?: return
        val a = latestAccel ?: return
        val m = if (magInputEnabled) latestMag else null
        if (m != null) {
            eng.updateMarg(gyroCorrected.x, gyroCorrected.y, gyroCorrected.z, a[0], a[1], a[2], m[0], m[1], m[2])
        } else {
            eng.updateImu(gyroCorrected.x, gyroCorrected.y, gyroCorrected.z, a[0], a[1], a[2])
        }
        _quaternion.value = eng.quaternion()
    }

    override fun onAccuracyChanged(sensor: Sensor?, accuracy: Int) {
        // No-op: accuracy lands on each event already.
    }

    private fun maybeFlushRates(nowNanos: Long) {
        if (rateWindowStartNanos == 0L) {
            rateWindowStartNanos = nowNanos
            return
        }
        val elapsed = nowNanos - rateWindowStartNanos
        if (elapsed < RATE_WINDOW_NANOS) return
        val seconds = elapsed / 1_000_000_000.0f
        _rates.value = SensorRates(
            gyroHz = gyroCount / seconds,
            accelHz = accelCount / seconds,
            magHz = magCount / seconds,
        )
        gyroCount = 0
        accelCount = 0
        magCount = 0
        rateWindowStartNanos = nowNanos
    }

    companion object {
        private const val RATE_WINDOW_NANOS = 1_000_000_000L

        // ~30 Hz cap on UI snapshot emissions (sensors arrive far faster).
        private const val UI_SNAPSHOT_INTERVAL_NANOS = 33_000_000L

        // Gravity low-pass for linear-acceleration extraction (slow = isolates gravity).
        private const val GRAVITY_LP_ALPHA = 0.9f

        // Gyro-rate warmup: discard the registration burst, then require both a
        // minimum sample count and a minimum time span so the estimate is solid
        // on slow watches (~50 Hz) and fast phones (~500 Hz) alike.
        private const val WARMUP_SKIP = 5
        private const val WARMUP_MIN_INTERVALS = 40
        private const val WARMUP_MIN_SPAN_NS = 300_000_000L
        private const val MIN_RATE_HZ = 20.0
        private const val MAX_RATE_HZ = 1000.0

        // Prefer the wake-up variant of each sensor. A wake-up sensor wakes the
        // application processor out of deep sleep to deliver samples, so the
        // listener keeps firing once the screen turns off; the non-wake-up
        // variant stops when the CPU suspends, freezing tracking. This is the
        // approach Samsung documents for screen-off sensor streaming on Galaxy
        // Watch. Fall back to non-wake-up where a wake-up variant is absent.
        private fun SensorManager.preferred(primaryType: Int, fallbackType: Int): Sensor? =
            getDefaultSensor(primaryType, true)
                ?: getDefaultSensor(primaryType)
                ?: getDefaultSensor(fallbackType, true)
                ?: getDefaultSensor(fallbackType)
    }
}

data class SensorRates(
    val gyroHz: Float = 0f,
    val accelHz: Float = 0f,
    val magHz: Float = 0f,
)
