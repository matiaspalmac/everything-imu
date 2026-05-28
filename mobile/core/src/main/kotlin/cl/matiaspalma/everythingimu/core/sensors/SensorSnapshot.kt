package cl.matiaspalma.everythingimu.core.sensors

data class Vec3(val x: Float, val y: Float, val z: Float) {
    companion object { val ZERO = Vec3(0f, 0f, 0f) }
}

data class SensorSample(
    val value: Vec3,
    val timestampNanos: Long,
    val accuracy: Int,
    val uncalibrated: Boolean,
) {
    companion object {
        val EMPTY = SensorSample(Vec3.ZERO, 0L, 0, uncalibrated = false)
    }
}

data class SensorSnapshot(
    val gyro: SensorSample = SensorSample.EMPTY,
    val accel: SensorSample = SensorSample.EMPTY,
    val mag: SensorSample = SensorSample.EMPTY,
    val gyroAvailable: Boolean = false,
    val accelAvailable: Boolean = false,
    val magAvailable: Boolean = false,
)
