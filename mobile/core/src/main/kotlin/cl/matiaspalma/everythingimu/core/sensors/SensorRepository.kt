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

    private var engine: VqfEngine? = null

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

    private var gyroCount = 0
    private var accelCount = 0
    private var magCount = 0
    private var rateWindowStartNanos = 0L
    private var running = false

    fun start() {
        if (running) return
        running = true
        if (engine == null && fusionEnabled) {
            engine = VqfEngine.create(sampleRateHz = 400.0)
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
                val corrected = applyGyroBias(v, ts)
                _snapshot.update { it.copy(gyro = SensorSample(corrected, ts, acc, false)) }
                runFusion(corrected)
            }
            Sensor.TYPE_GYROSCOPE_UNCALIBRATED -> {
                gyroCount++
                val corrected = applyGyroBias(v, ts)
                _snapshot.update { it.copy(gyro = SensorSample(corrected, ts, acc, true)) }
                runFusion(corrected)
            }
            Sensor.TYPE_ACCELEROMETER -> {
                accelCount++
                _snapshot.update { it.copy(accel = SensorSample(v, ts, acc, false)) }
                latestAccel = event.values.copyOf(3)
                shakeListener?.invoke(kotlin.math.sqrt(v.x * v.x + v.y * v.y + v.z * v.z))
            }
            Sensor.TYPE_ACCELEROMETER_UNCALIBRATED -> {
                accelCount++
                _snapshot.update { it.copy(accel = SensorSample(v, ts, acc, true)) }
                latestAccel = event.values.copyOf(3)
                shakeListener?.invoke(kotlin.math.sqrt(v.x * v.x + v.y * v.y + v.z * v.z))
            }
            Sensor.TYPE_MAGNETIC_FIELD -> {
                magCount++
                magSampleSink?.invoke(v)
                val calibrated = magCalibration.transform(v)
                _snapshot.update { it.copy(mag = SensorSample(calibrated, ts, acc, false)) }
                latestMag = floatArrayOf(calibrated.x, calibrated.y, calibrated.z)
            }
            Sensor.TYPE_MAGNETIC_FIELD_UNCALIBRATED -> {
                magCount++
                magSampleSink?.invoke(v)
                val calibrated = magCalibration.transform(v)
                _snapshot.update { it.copy(mag = SensorSample(calibrated, ts, acc, true)) }
                latestMag = floatArrayOf(calibrated.x, calibrated.y, calibrated.z)
            }
        }
        maybeFlushRates(ts)
    }

    private fun applyGyroBias(raw: Vec3, ts: Long): Vec3 {
        val wasCalibrated = gyroBias.isCalibrated()
        val corrected = gyroBias.apply(raw, ts)
        if (!wasCalibrated && gyroBias.isCalibrated()) {
            onGyroBiasUpdated?.invoke(gyroBias.bias())
        }
        return corrected
    }

    private fun runFusion(gyroCorrected: Vec3) {
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

        private fun SensorManager.preferred(primaryType: Int, fallbackType: Int): Sensor? =
            getDefaultSensor(primaryType) ?: getDefaultSensor(fallbackType)
    }
}

data class SensorRates(
    val gyroHz: Float = 0f,
    val accelHz: Float = 0f,
    val magHz: Float = 0f,
)
