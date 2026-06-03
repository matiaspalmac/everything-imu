package cl.matiaspalma.everythingimu.core.fusion

/**
 * Kotlin wrapper around the Rust VQF implementation in `crates/imu-fusion`.
 * Loaded via JNI from `libjni_android.so`.
 */
class VqfEngine private constructor(private var handle: Long) : AutoCloseable {

    private val quatBuf = FloatArray(4)
    private var magSeen = false

    // Updates run on the sensor thread; [close] may be called from another
    // thread (service teardown). Serialize native access so a freed handle is
    // never dereferenced — nativeDrop followed by a concurrent nativeUpdate*
    // would be a use-after-free in the Rust side.
    @Synchronized
    fun updateImu(gx: Float, gy: Float, gz: Float, ax: Float, ay: Float, az: Float) {
        if (handle == 0L) return
        VqfNative.nativeUpdateImu(handle, gx, gy, gz, ax, ay, az)
    }

    @Synchronized
    fun updateMarg(
        gx: Float, gy: Float, gz: Float,
        ax: Float, ay: Float, az: Float,
        mx: Float, my: Float, mz: Float,
    ) {
        if (handle == 0L) return
        VqfNative.nativeUpdateMarg(handle, gx, gy, gz, ax, ay, az, mx, my, mz)
        magSeen = true
    }

    /** Latest fused quaternion as (w, x, y, z). */
    @Synchronized
    fun quaternion(): Quaternion {
        if (handle == 0L) return Quaternion.IDENTITY
        if (magSeen) VqfNative.nativeQuat9d(handle, quatBuf)
        else VqfNative.nativeQuat6d(handle, quatBuf)
        return Quaternion(quatBuf[0], quatBuf[1], quatBuf[2], quatBuf[3])
    }

    @Synchronized
    override fun close() {
        if (handle == 0L) return
        VqfNative.nativeDrop(handle)
        handle = 0L
    }

    companion object {
        @Volatile private var loaded: Boolean = false

        @Synchronized
        fun isAvailable(): Boolean {
            ensureLoaded()
            return loaded
        }

        @Synchronized
        private fun ensureLoaded() {
            if (loaded) return
            loaded = try {
                System.loadLibrary("jni_android")
                true
            } catch (t: Throwable) {
                android.util.Log.e("VqfEngine", "failed to load libjni_android.so", t)
                false
            }
        }

        fun create(sampleRateHz: Double): VqfEngine? {
            ensureLoaded()
            if (!loaded) return null
            val handle = VqfNative.nativeNew(sampleRateHz)
            if (handle == 0L) return null
            return VqfEngine(handle)
        }
    }
}

internal object VqfNative {
    @JvmStatic external fun nativeNew(sampleRateHz: Double): Long
    @JvmStatic external fun nativeDrop(handle: Long)
    @JvmStatic external fun nativeUpdateImu(
        handle: Long,
        gx: Float, gy: Float, gz: Float,
        ax: Float, ay: Float, az: Float,
    )
    @JvmStatic external fun nativeUpdateMarg(
        handle: Long,
        gx: Float, gy: Float, gz: Float,
        ax: Float, ay: Float, az: Float,
        mx: Float, my: Float, mz: Float,
    )
    @JvmStatic external fun nativeQuat6d(handle: Long, out: FloatArray)
    @JvmStatic external fun nativeQuat9d(handle: Long, out: FloatArray)
}
