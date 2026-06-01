package cl.matiaspalma.everythingimu.core.prefs

import android.annotation.SuppressLint
import android.content.Context
import android.provider.Settings
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.floatPreferencesKey
import androidx.datastore.preferences.core.intPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import cl.matiaspalma.everythingimu.core.calibration.CalibrationData
import cl.matiaspalma.everythingimu.core.calibration.MagCalibration
import cl.matiaspalma.everythingimu.core.sensors.Vec3
import java.util.UUID
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map

private val Context.preferencesStore: DataStore<Preferences> by preferencesDataStore(name = "everythingimu")

class AppPrefs(context: Context) {

    private val appContext = context.applicationContext
    private val store: DataStore<Preferences> = appContext.preferencesStore

    val serviceWanted: Flow<Boolean> = store.data.map { it[KEY_SERVICE_WANTED] ?: false }
    val serverHost: Flow<String> = store.data.map { it[KEY_HOST] ?: "" }
    val serverPort: Flow<Int> = store.data.map { it[KEY_PORT] ?: 6969 }
    val autoConnect: Flow<Boolean> = store.data.map { it[KEY_AUTO_CONNECT] ?: true }
    val deviceUuid: Flow<String> = store.data.map { it[KEY_UUID] ?: "" }
    val sensorWarningDismissed: Flow<Boolean> =
        store.data.map { it[KEY_SENSOR_WARNING_DISMISSED] ?: false }
    val language: Flow<String> = store.data.map { it[KEY_LANGUAGE] ?: "en" }
    val themeMode: Flow<String> = store.data.map { it[KEY_THEME] ?: "dark" }
    val sendRateHz: Flow<Int> = store.data.map { it[KEY_SEND_RATE] ?: 100 }
    // Default OFF: a raw, uncalibrated magnetometer near a PC/monitor (the
    // typical VR setup) drags VQF's heading and causes yaw drift at rest.
    // owoTrack and moveTrackVR default mag off for the same reason. Users can
    // opt in via the toggle after running the figure-8 calibration.
    val magEnabled: Flow<Boolean> = store.data.map { it[KEY_MAG_ENABLED] ?: false }
    val shakeRecenter: Flow<Boolean> = store.data.map { it[KEY_SHAKE_RECENTER] ?: true }

    // false = on-device VQF (default). true = OS Game Rotation Vector (owoTrack-style).
    val useOsRotation: Flow<Boolean> = store.data.map { it[KEY_OS_ROTATION] ?: false }

    // Start the OSC haptic server automatically on connect.
    val autostartHaptics: Flow<Boolean> = store.data.map { it[KEY_AUTOSTART_HAPTICS] ?: false }

    suspend fun setLanguage(code: String) {
        store.edit { it[KEY_LANGUAGE] = code }
    }

    suspend fun setThemeMode(code: String) {
        store.edit { it[KEY_THEME] = code }
    }

    suspend fun setSendRateHz(hz: Int) {
        store.edit { it[KEY_SEND_RATE] = hz.coerceIn(20, 400) }
    }

    suspend fun setMagEnabled(enabled: Boolean) {
        store.edit { it[KEY_MAG_ENABLED] = enabled }
    }

    suspend fun setShakeRecenter(enabled: Boolean) {
        store.edit { it[KEY_SHAKE_RECENTER] = enabled }
    }

    suspend fun setUseOsRotation(enabled: Boolean) {
        store.edit { it[KEY_OS_ROTATION] = enabled }
    }

    suspend fun setAutostartHaptics(enabled: Boolean) {
        store.edit { it[KEY_AUTOSTART_HAPTICS] = enabled }
    }

    suspend fun setServiceWanted(value: Boolean) {
        store.edit { it[KEY_SERVICE_WANTED] = value }
    }

    suspend fun setServer(host: String, port: Int) {
        store.edit {
            it[KEY_HOST] = host
            it[KEY_PORT] = port
        }
    }

    suspend fun setAutoConnect(value: Boolean) {
        store.edit { it[KEY_AUTO_CONNECT] = value }
    }

    suspend fun deviceUuidOrCreate(): String {
        val prefs = store.data.first()
        val current = prefs[KEY_UUID].orEmpty()
        val androidId = readAndroidId().ifBlank { "unknown" }
        val storedAndroidId = prefs[KEY_ANDROID_ID].orEmpty()
        if (current.isNotBlank() && storedAndroidId == androidId) return current
        val fresh = UUID.randomUUID().toString()
        store.edit {
            it[KEY_UUID] = fresh
            it[KEY_ANDROID_ID] = androidId
        }
        return fresh
    }

    suspend fun setSensorWarningDismissed(value: Boolean) {
        store.edit { it[KEY_SENSOR_WARNING_DISMISSED] = value }
    }

    /** Read the full calibration bundle in a single DataStore round-trip. */
    suspend fun loadCalibration(): CalibrationData {
        val prefs = store.data.first()
        val bias = Vec3(
            prefs[KEY_GYRO_BIAS_X] ?: 0f,
            prefs[KEY_GYRO_BIAS_Y] ?: 0f,
            prefs[KEY_GYRO_BIAS_Z] ?: 0f,
        )
        val mag = MagCalibration(
            offset = Vec3(
                prefs[KEY_MAG_OFFSET_X] ?: 0f,
                prefs[KEY_MAG_OFFSET_Y] ?: 0f,
                prefs[KEY_MAG_OFFSET_Z] ?: 0f,
            ),
            scale = Vec3(
                prefs[KEY_MAG_SCALE_X] ?: 1f,
                prefs[KEY_MAG_SCALE_Y] ?: 1f,
                prefs[KEY_MAG_SCALE_Z] ?: 1f,
            ),
        )
        return CalibrationData(gyroBias = bias, mag = mag)
    }

    suspend fun setGyroBias(bias: Vec3) {
        store.edit {
            it[KEY_GYRO_BIAS_X] = bias.x
            it[KEY_GYRO_BIAS_Y] = bias.y
            it[KEY_GYRO_BIAS_Z] = bias.z
        }
    }

    suspend fun setMagCalibration(cal: MagCalibration) {
        store.edit {
            it[KEY_MAG_OFFSET_X] = cal.offset.x
            it[KEY_MAG_OFFSET_Y] = cal.offset.y
            it[KEY_MAG_OFFSET_Z] = cal.offset.z
            it[KEY_MAG_SCALE_X] = cal.scale.x
            it[KEY_MAG_SCALE_Y] = cal.scale.y
            it[KEY_MAG_SCALE_Z] = cal.scale.z
        }
    }

    suspend fun clearCalibration() {
        store.edit {
            it.remove(KEY_GYRO_BIAS_X); it.remove(KEY_GYRO_BIAS_Y); it.remove(KEY_GYRO_BIAS_Z)
            it.remove(KEY_MAG_OFFSET_X); it.remove(KEY_MAG_OFFSET_Y); it.remove(KEY_MAG_OFFSET_Z)
            it.remove(KEY_MAG_SCALE_X); it.remove(KEY_MAG_SCALE_Y); it.remove(KEY_MAG_SCALE_Z)
        }
    }

    companion object {
        private val KEY_SERVICE_WANTED = booleanPreferencesKey("service_wanted")
        private val KEY_HOST = stringPreferencesKey("server_host")
        private val KEY_PORT = intPreferencesKey("server_port")
        private val KEY_AUTO_CONNECT = booleanPreferencesKey("auto_connect")
        private val KEY_UUID = stringPreferencesKey("device_uuid")
        private val KEY_ANDROID_ID = stringPreferencesKey("android_id")
        private val KEY_SENSOR_WARNING_DISMISSED = booleanPreferencesKey("sensor_warning_dismissed")
        private val KEY_LANGUAGE = stringPreferencesKey("language")
        private val KEY_THEME = stringPreferencesKey("theme_mode")
        private val KEY_SEND_RATE = intPreferencesKey("send_rate_hz")
        private val KEY_MAG_ENABLED = booleanPreferencesKey("mag_enabled")
        private val KEY_SHAKE_RECENTER = booleanPreferencesKey("shake_recenter")
        private val KEY_OS_ROTATION = booleanPreferencesKey("use_os_rotation")
        private val KEY_AUTOSTART_HAPTICS = booleanPreferencesKey("autostart_haptics")
        private val KEY_GYRO_BIAS_X = floatPreferencesKey("gyro_bias_x")
        private val KEY_GYRO_BIAS_Y = floatPreferencesKey("gyro_bias_y")
        private val KEY_GYRO_BIAS_Z = floatPreferencesKey("gyro_bias_z")
        private val KEY_MAG_OFFSET_X = floatPreferencesKey("mag_offset_x")
        private val KEY_MAG_OFFSET_Y = floatPreferencesKey("mag_offset_y")
        private val KEY_MAG_OFFSET_Z = floatPreferencesKey("mag_offset_z")
        private val KEY_MAG_SCALE_X = floatPreferencesKey("mag_scale_x")
        private val KEY_MAG_SCALE_Y = floatPreferencesKey("mag_scale_y")
        private val KEY_MAG_SCALE_Z = floatPreferencesKey("mag_scale_z")
    }

    @SuppressLint("HardwareIds")
    private fun readAndroidId(): String {
        return Settings.Secure.getString(appContext.contentResolver, Settings.Secure.ANDROID_ID).orEmpty()
    }
}
