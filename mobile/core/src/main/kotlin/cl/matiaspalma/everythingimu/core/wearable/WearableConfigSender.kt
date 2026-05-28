package cl.matiaspalma.everythingimu.core.wearable

import android.content.Context
import com.google.android.gms.wearable.PutDataMapRequest
import com.google.android.gms.wearable.Wearable
import kotlinx.coroutines.tasks.await

/**
 * Pushes the SlimeVR server `host:port` to any paired Wear OS device over the
 * Wearable Data Layer. The watch persists it locally so a standalone connect
 * works without re-entering the address on the tiny screen.
 *
 * Best-effort: a phone with no watch (or no Play Services) just no-ops.
 */
object WearableConfigSender {

    /** DataItem path shared with the wear-side listener service. */
    const val SERVER_CONFIG_PATH = "/eimu/server"

    suspend fun push(context: Context, host: String, port: Int): Result<Unit> = runCatching {
        val client = Wearable.getDataClient(context.applicationContext)
        val req = PutDataMapRequest.create(SERVER_CONFIG_PATH).apply {
            dataMap.putString("host", host)
            dataMap.putInt("port", port)
            // Bump a timestamp so an unchanged host:port still emits a fresh
            // DataItem — the Data Layer dedupes identical payloads otherwise.
            dataMap.putLong("ts", System.currentTimeMillis())
        }.asPutDataRequest().setUrgent()
        client.putDataItem(req).await()
        Unit
    }
}
