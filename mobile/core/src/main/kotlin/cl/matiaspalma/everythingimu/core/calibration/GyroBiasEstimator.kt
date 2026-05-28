package cl.matiaspalma.everythingimu.core.calibration

import cl.matiaspalma.everythingimu.core.sensors.Vec3
import kotlin.math.sqrt

/**
 * Online gyro bias estimator. While the device is stationary
 * (gyro magnitude below [stationaryThreshold] for [stationaryHoldNanos]),
 * we average the raw samples and treat the mean as the bias.
 *
 * Stationary detection follows the SlimeVR / wrangler approach: low-pass
 * the gyro norm and require the magnitude to stay under threshold for a
 * minimum window before accepting a bias update.
 */
class GyroBiasEstimator(
    private val stationaryThreshold: Float = 0.05f,   // rad/s
    private val stationaryHoldNanos: Long = 3_000_000_000L,  // 3 s
) {

    @Volatile private var biasX: Float = 0f
    @Volatile private var biasY: Float = 0f
    @Volatile private var biasZ: Float = 0f
    @Volatile private var sampleCount: Int = 0
    @Volatile private var calibrated: Boolean = false

    private var sumX: Double = 0.0
    private var sumY: Double = 0.0
    private var sumZ: Double = 0.0
    private var samples: Int = 0
    private var windowStartNanos: Long = 0L

    fun bias(): Vec3 = Vec3(biasX, biasY, biasZ)
    fun isCalibrated(): Boolean = calibrated

    /** Seed the estimator with a previously persisted bias. */
    fun seed(bias: Vec3) {
        biasX = bias.x; biasY = bias.y; biasZ = bias.z
        calibrated = bias.x != 0f || bias.y != 0f || bias.z != 0f
    }

    fun reset() {
        biasX = 0f; biasY = 0f; biasZ = 0f
        sampleCount = 0
        calibrated = false
        clearWindow()
    }

    /**
     * Feed a raw gyro sample. Returns the bias-corrected reading.
     * The bias itself updates only while stationary; consumers should pass
     * `corrected` to fusion.
     */
    fun apply(raw: Vec3, timestampNanos: Long): Vec3 {
        val mag = sqrt((raw.x * raw.x + raw.y * raw.y + raw.z * raw.z).toDouble()).toFloat()
        if (mag < stationaryThreshold) {
            if (windowStartNanos == 0L) windowStartNanos = timestampNanos
            sumX += raw.x; sumY += raw.y; sumZ += raw.z
            samples += 1
            if (timestampNanos - windowStartNanos >= stationaryHoldNanos && samples >= 32) {
                biasX = (sumX / samples).toFloat()
                biasY = (sumY / samples).toFloat()
                biasZ = (sumZ / samples).toFloat()
                sampleCount = samples
                calibrated = true
                clearWindow()
            }
        } else {
            clearWindow()
        }
        return Vec3(raw.x - biasX, raw.y - biasY, raw.z - biasZ)
    }

    private fun clearWindow() {
        sumX = 0.0; sumY = 0.0; sumZ = 0.0
        samples = 0
        windowStartNanos = 0L
    }
}
