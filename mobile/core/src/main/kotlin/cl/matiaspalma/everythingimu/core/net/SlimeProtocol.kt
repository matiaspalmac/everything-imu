package cl.matiaspalma.everythingimu.core.net

import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * SlimeVR wire-protocol encoders. Mirrors `crates/slime-tracker` byte layout.
 *
 * Datagram envelope: `[tag: u32 BE][seq: u64 BE][payload: BE]`.
 */
object SlimeProtocol {
    const val PACKET_HEARTBEAT: Int = 0
    /** Legacy owoTrack rotation packet — 4 floats (X, Y, Z, W). */
    const val PACKET_ROTATION_LEGACY: Int = 1
    const val PACKET_GYRO: Int = 2
    const val PACKET_HANDSHAKE: Int = 3
    const val PACKET_ACCEL: Int = 4
    const val PACKET_MAG: Int = 5
    const val PACKET_PING: Int = 10
    const val PACKET_BATTERY: Int = 12
    const val PACKET_SENSOR_INFO: Int = 15
    /** Modern SlimeVR RotationData (sensor_id + data_type + quat + calibration). */
    const val PACKET_ROTATION_DATA: Int = 17
    const val PACKET_USER_ACTION: Int = 21
    const val PACKET_FEATURE_FLAGS: Int = 22
    const val PACKET_BUTTON_PUSHED: Int = 60

    /** Board type enum carried in the handshake (i32). */
    const val BOARD_OWO: Int = 13

    /** McuType used by owoTrack-compatible firmwares. */
    const val MCU_OWO: Int = 3

    /** ImuType carried in handshake (i8 after 3 pad bytes). */
    const val IMU_UNKNOWN: Byte = 0

    /**
     * Legacy owoTrack handshake uses `firmwareBuild = 8` at the position rust
     * slime-tracker labels `protocol_version`. SlimeVR-Server reads the
     * firmware string and routes to the owoTrack input plugin when it sees
     * "owoTrack8", which auto-registers a single sensor — no SensorInfo (15)
     * packet required. Matches moveTrackVR / owoTrackVR / abb128.
     */
    const val FIRMWARE_BUILD: Int = 8

    /** Firmware identifier for owoTrack lineage. */
    const val FIRMWARE_STRING: String = "owoTrack8"

    /**
     * Handshake (packet 3). Byte-for-byte parity with moveTrackVR Handshaker.java
     * to ensure SlimeVR-Server's owoTrack compatibility plugin auto-registers
     * the tracker without a follow-up SensorInfo packet.
     */
    fun handshake(seq: Long, mac: ByteArray, firmware: String = FIRMWARE_STRING): ByteArray {
        require(mac.size == 6) { "MAC must be 6 bytes" }
        val firmwareBytes = firmware.toByteArray(Charsets.US_ASCII)
        require(firmwareBytes.size <= 255) { "firmware string too long" }

        // i32 board + i32 imu + i32 mcu + 3*i32 imu_info + i32 firmwareBuild + u8 len + firmware + 6 mac + 0xFF
        val payloadSize = 4 + 4 + 4 + 12 + 4 + 1 + firmwareBytes.size + 6 + 1
        val buf = header(PACKET_HANDSHAKE, seq, payloadSize)
        buf.putInt(BOARD_OWO)
        buf.putInt(0) // imuType = Unknown, encoded as i32 (3 pad bytes + u8)
        buf.putInt(MCU_OWO)
        buf.putInt(0); buf.putInt(0); buf.putInt(0) // imu_info slots
        buf.putInt(FIRMWARE_BUILD)
        buf.put(firmwareBytes.size.toByte())
        buf.put(firmwareBytes)
        buf.put(mac)
        buf.put(0xFF.toByte()) // trailing terminator byte
        return buf.array()
    }

    /**
     * SensorInfo (id 15). Required for server to register the tracker after handshake.
     * @param magEnabled wire bitmask: 0x0003 mag enabled, 0x0002 supported-but-disabled, 0x0000 unsupported.
     */
    fun sensorInfo(
        seq: Long,
        sensorId: Int = 0,
        imuType: Int = 0,
        magEnabled: Int = 0x0003,
        trackerPosition: Int = 0,
        trackerDataType: Int = 0,
    ): ByteArray {
        val payloadSize = 1 + 1 + 1 + 2 + 1 + 1 + 1
        val buf = header(PACKET_SENSOR_INFO, seq, payloadSize)
        buf.put(sensorId.toByte())
        buf.put(1.toByte()) // sensor_status = Ok
        buf.put(imuType.toByte()) // ImuType = Unknown (0) — server still registers it
        buf.putShort(magEnabled.toShort())
        buf.put(0.toByte()) // has_completed_rest_calibration
        buf.put(trackerPosition.toByte()) // 0 = None (user assigns in UI)
        buf.put(trackerDataType.toByte()) // 0 = Rotation
        return buf.array()
    }

    fun heartbeat(seq: Long, trackerId: Int = 0): ByteArray {
        val buf = header(PACKET_HEARTBEAT, seq, 1)
        buf.put(trackerId.toByte())
        return buf.array()
    }

    /**
     * Legacy rotation packet (id 1) used by owoTrack-compat mode. Just 4 floats
     * in (X, Y, Z, W) order. No sensor_id, no data_type, no calibration byte.
     * Matches moveTrackVR provide_rot / owoTrackVRSyncMobile reference.
     *
     * @param wxyz quaternion as (w, x, y, z) — converted to wire (X=x, Y=y, Z=z, W=w).
     */
    fun rotation(seq: Long, sensorId: Int, wxyz: FloatArray, calibrationInfo: Int = 0): ByteArray {
        require(wxyz.size == 4) { "quaternion must be 4 floats" }
        // 4 floats only, owoTrack legacy layout.
        val payloadSize = 4 * 4
        val buf = header(PACKET_ROTATION_LEGACY, seq, payloadSize)
        buf.putFloat(wxyz[1]) // X
        buf.putFloat(wxyz[2]) // Y
        buf.putFloat(wxyz[3]) // Z
        buf.putFloat(wxyz[0]) // W
        return buf.array()
    }

    /** Echo ping (id 10) back to the server unchanged. */
    fun pong(seq: Long, challenge: ByteArray): ByteArray {
        require(challenge.size == 4) { "ping challenge must be 4 bytes" }
        val buf = header(PACKET_PING, seq, 4)
        buf.put(challenge)
        return buf.array()
    }

    fun battery(seq: Long, voltageVolts: Float, level: Float): ByteArray {
        val buf = header(PACKET_BATTERY, seq, 8)
        buf.putFloat(voltageVolts)
        buf.putFloat(level)
        return buf.array()
    }

    fun userAction(seq: Long, action: Int): ByteArray {
        val buf = header(PACKET_USER_ACTION, seq, 4)
        buf.putInt(action)
        return buf.array()
    }

    /** Legacy owoTrack recenter — packet 60 with empty payload (just header). */
    fun buttonPushed(seq: Long): ByteArray {
        val buf = header(PACKET_BUTTON_PUSHED, seq, 0)
        return buf.array()
    }

    private fun header(tag: Int, seq: Long, payloadSize: Int): ByteBuffer {
        val buf = ByteBuffer.allocate(12 + payloadSize).order(ByteOrder.BIG_ENDIAN)
        buf.putInt(tag)
        buf.putLong(seq)
        return buf
    }

    /**
     * Parse just the tag of an incoming datagram. Returns -1 if too short.
     */
    fun peekTag(data: ByteArray, length: Int): Int {
        if (length < 4) return -1
        val buf = ByteBuffer.wrap(data, 0, length).order(ByteOrder.BIG_ENDIAN)
        return buf.int
    }

    /** Extract a 4-byte ping challenge from an incoming datagram (offset past header). */
    fun extractPingChallenge(data: ByteArray, length: Int): ByteArray? {
        if (length < 12 + 4) return null
        return data.copyOfRange(12, 16)
    }
}
