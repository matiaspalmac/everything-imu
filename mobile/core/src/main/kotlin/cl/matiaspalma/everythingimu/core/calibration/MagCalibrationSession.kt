package cl.matiaspalma.everythingimu.core.calibration

import cl.matiaspalma.everythingimu.core.sensors.Vec3
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Drives a figure-8 magnetometer calibration session. The UI starts the
 * session, then forwards every raw mag sample through [feed] until
 * [progress] reports `complete`. UI calls [accept] to persist or [discard]
 * to bail.
 */
class MagCalibrationSession(
    /** µT spread required on each axis before we consider coverage complete. */
    private val minSpreadTarget: Float = 30f,
    /** Floor on sample count to avoid converging on a small noisy bubble. */
    private val minSamples: Int = 256,
) {
    private val calibrator = MagCalibrator()

    private val _progress = MutableStateFlow(MagProgress.IDLE)
    val progress: StateFlow<MagProgress> = _progress.asStateFlow()

    @Volatile private var active: Boolean = false

    fun start() {
        calibrator.reset()
        _progress.value = MagProgress.IDLE
        active = true
    }

    fun stop() {
        active = false
    }

    fun feed(sample: Vec3) {
        if (!active) return
        calibrator.addSample(sample)
        val spread = calibrator.minSpread()
        val ratio = (spread / minSpreadTarget).coerceIn(0f, 1f)
        val samples = calibrator.sampleCount()
        val complete = spread >= minSpreadTarget && samples >= minSamples
        _progress.value = MagProgress(
            samples = samples,
            coverage = calibrator.coverage(),
            ratio = ratio,
            complete = complete,
            preview = calibrator.partial,
        )
    }

    /** Accept the current best-fit. Returns null if not yet good enough. */
    fun accept(): MagCalibration? {
        val result = calibrator.finish()
        if (result.isIdentity) return null
        active = false
        return result
    }

    fun discard() {
        active = false
        calibrator.reset()
        _progress.value = MagProgress.IDLE
    }
}

data class MagProgress(
    val samples: Int,
    val coverage: Vec3,
    val ratio: Float,
    val complete: Boolean,
    val preview: MagCalibration,
) {
    companion object {
        val IDLE = MagProgress(
            samples = 0,
            coverage = Vec3.ZERO,
            ratio = 0f,
            complete = false,
            preview = MagCalibration.IDENTITY,
        )
    }
}
