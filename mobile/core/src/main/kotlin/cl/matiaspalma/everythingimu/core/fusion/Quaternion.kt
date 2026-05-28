package cl.matiaspalma.everythingimu.core.fusion

import kotlin.math.asin
import kotlin.math.atan2
import kotlin.math.sqrt

/** Quaternion in (w, x, y, z) order. */
data class Quaternion(val w: Float, val x: Float, val y: Float, val z: Float) {
    companion object {
        val IDENTITY = Quaternion(1f, 0f, 0f, 0f)
    }
}

/** Yaw / pitch / roll in radians, ZYX order. */
data class EulerAngles(val yaw: Float, val pitch: Float, val roll: Float)

fun Quaternion.normalized(): Quaternion {
    val mag = sqrt(w * w + x * x + y * y + z * z)
    if (mag == 0f) return Quaternion.IDENTITY
    return Quaternion(w / mag, x / mag, y / mag, z / mag)
}

fun Quaternion.conjugate(): Quaternion = Quaternion(w, -x, -y, -z)

fun Quaternion.inverse(): Quaternion = conjugate().normalized()

operator fun Quaternion.times(other: Quaternion): Quaternion {
    return Quaternion(
        w = w * other.w - x * other.x - y * other.y - z * other.z,
        x = w * other.x + x * other.w + y * other.z - z * other.y,
        y = w * other.y - x * other.z + y * other.w + z * other.x,
        z = w * other.z + x * other.y - y * other.x + z * other.w,
    )
}

fun Quaternion.toEuler(): EulerAngles {
    val sinrCosp = 2f * (w * x + y * z)
    val cosrCosp = 1f - 2f * (x * x + y * y)
    val roll = atan2(sinrCosp, cosrCosp)

    val sinp = 2f * (w * y - z * x)
    val pitch = when {
        sinp >= 1f -> (Math.PI / 2.0).toFloat()
        sinp <= -1f -> (-Math.PI / 2.0).toFloat()
        else -> asin(sinp)
    }

    val sinyCosp = 2f * (w * z + x * y)
    val cosyCosp = 1f - 2f * (y * y + z * z)
    val yaw = atan2(sinyCosp, cosyCosp)

    return EulerAngles(yaw, pitch, roll)
}
