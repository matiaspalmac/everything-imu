package cl.matiaspalma.everythingimu.core.calibration

import cl.matiaspalma.everythingimu.core.sensors.Vec3

/**
 * Aggregated calibration state persisted via [cl.matiaspalma.everythingimu.core.prefs.AppPrefs].
 * Stored as floats keyed by axis so a corrupt single value doesn't invalidate the whole set.
 */
data class CalibrationData(
    val gyroBias: Vec3 = Vec3.ZERO,
    val mag: MagCalibration = MagCalibration.IDENTITY,
) {
    companion object {
        val EMPTY = CalibrationData()
    }
}
