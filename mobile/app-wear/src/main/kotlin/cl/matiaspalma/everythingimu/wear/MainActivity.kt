package cl.matiaspalma.everythingimu.wear

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.view.WindowManager
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.wear.compose.material.Chip
import androidx.wear.compose.material.ChipDefaults
import androidx.wear.compose.material.MaterialTheme
import androidx.wear.compose.material.Text
import cl.matiaspalma.everythingimu.core.net.ConnectionState
import cl.matiaspalma.everythingimu.core.tracking.TrackingController
import cl.matiaspalma.everythingimu.core.update.UpdateChecker
import cl.matiaspalma.everythingimu.wear.BuildConfig
import cl.matiaspalma.everythingimu.wear.setup.HostPickerScreen
import com.google.android.gms.common.ConnectionResult
import com.google.android.gms.common.GoogleApiAvailability
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

class MainActivity : ComponentActivity() {

    private val requestNotificationPermission =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { /* ignored */ }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        TrackingController.ensureInit(this)
        maybeAskNotificationPermission()
        setContent { WearApp() }
    }

    private fun maybeAskNotificationPermission() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        val granted = ContextCompat.checkSelfPermission(
            this, Manifest.permission.POST_NOTIFICATIONS,
        ) == PackageManager.PERMISSION_GRANTED
        if (!granted) requestNotificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
    }
}

@Composable
private fun WearApp() {
    MaterialTheme {
        val context = LocalContext.current
        val connection by TrackingController.connection.collectAsStateWithLifecycle()

        // Wear OS parks the Wi-Fi radio and throttles the wakelock when the
        // screen turns off while Bluetooth-paired, which stops UDP streaming —
        // a platform power policy no normal app can override. Keeping the screen
        // on during a session keeps the device interactive so Wi-Fi, sensors and
        // CPU stay alive. Costs battery, but it's the only reliable way to track
        // with the wrist down. Cleared as soon as tracking stops.
        val keepScreenOn = connection == ConnectionState.Connected ||
            connection == ConnectionState.Connecting ||
            connection == ConnectionState.Reconnecting
        val activity = context as? Activity
        DisposableEffect(keepScreenOn) {
            activity?.window?.apply {
                if (keepScreenOn) addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                else clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
            }
            onDispose {
                activity?.window?.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
            }
        }
        val stats by TrackingController.clientStats.collectAsStateWithLifecycle()
        val lastError by TrackingController.lastError.collectAsStateWithLifecycle()
        val scope = rememberCoroutineScope()

        var host by remember { mutableStateOf("") }
        var port by remember { mutableStateOf(6969) }
        var showPicker by remember { mutableStateOf(false) }
        var screensaver by remember { mutableStateOf(false) }
        var updateInfo by remember { mutableStateOf<UpdateChecker.UpdateInfo?>(null) }
        LaunchedEffect(Unit) {
            host = TrackingController.savedHost()
            port = TrackingController.savedPort()
            if (host.isBlank()) {
                val gmsOk = GoogleApiAvailability.getInstance()
                    .isGooglePlayServicesAvailable(context) == ConnectionResult.SUCCESS
                if (gmsOk) {
                    // Give the paired phone's Data Layer push a moment to land
                    // before falling back to manual entry.
                    withTimeoutOrNull(3000) {
                        TrackingController.serverHostFlow()?.first { it.isNotBlank() }
                    }
                    host = TrackingController.savedHost()
                    port = TrackingController.savedPort()
                }
                // No GMS (AOSP / de-Googled), or no push arrived → picker.
                showPicker = host.isBlank()
            }
            // Auto-connect on launch when enabled and a host is known.
            if (host.isNotBlank() && TrackingController.autoConnectEnabled()) {
                TrackingController.start(context)
                TrackingController.connect(host, port)
            }
            // Background update check. Wear has no browser of its own, so the
            // ACTION_VIEW intent below typically prompts the user to open the
            // release page on the paired phone instead.
            updateInfo = UpdateChecker.check(BuildConfig.VERSION_NAME)
        }

        if (showPicker) {
            HostPickerScreen(
                initialHost = host,
                initialPort = port,
                onSave = { h, p ->
                    scope.launch {
                        TrackingController.persistServer(h, p)
                        host = h
                        port = p
                        showPicker = false
                    }
                },
            )
            return@MaterialTheme
        }

        if (screensaver) {
            ScreensaverScreen(
                connection = connection,
                endpoint = stats.targetEndpoint,
                onExit = { screensaver = false },
            )
            return@MaterialTheme
        }

        Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Column(
                modifier = Modifier
                    .padding(8.dp)
                    .verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(6.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Text("everything-imu", style = MaterialTheme.typography.title3)
                Text(
                    "${connection.name} · ${stats.packetsSent} pkt",
                    style = MaterialTheme.typography.caption2,
                )
                Text(
                    if (host.isBlank()) "host: not set" else "$host:$port",
                    style = MaterialTheme.typography.caption2,
                )
                if (!lastError.isNullOrBlank()) {
                    Text(lastError.orEmpty(), style = MaterialTheme.typography.caption2)
                }

                val active = connection == ConnectionState.Connected ||
                    connection == ConnectionState.Connecting ||
                    connection == ConnectionState.Reconnecting

                // Single action: Connect brings up the foreground service
                // (sensors + wakelock + Wi-Fi bind) and opens the UDP socket
                // together; Disconnect tears both down. No separate "Start".
                Chip(
                    onClick = {
                        if (active) {
                            TrackingController.disconnect()
                            TrackingController.stop(context)
                        } else if (host.isNotBlank()) {
                            TrackingController.start(context)
                            scope.launch { TrackingController.connect(host, port) }
                        }
                    },
                    enabled = active || host.isNotBlank(),
                    label = { Text(if (active) "Disconnect" else "Connect") },
                    colors = if (active) ChipDefaults.secondaryChipColors() else ChipDefaults.primaryChipColors(),
                    modifier = Modifier.fillMaxWidth(),
                )

                Chip(
                    onClick = { TrackingController.sendRecenter() },
                    enabled = connection == ConnectionState.Connected,
                    label = { Text("Recenter") },
                    colors = ChipDefaults.secondaryChipColors(),
                    modifier = Modifier.fillMaxWidth(),
                )

                // Dim almost-black status screen for wrist-down use. Keeps the
                // screen technically on (Wi-Fi/sensors/CPU alive) but draws
                // near-zero on OLED, and shifts text each minute to avoid burn-in.
                if (active) {
                    Chip(
                        onClick = { screensaver = true },
                        label = { Text("Screensaver") },
                        colors = ChipDefaults.secondaryChipColors(),
                        modifier = Modifier.fillMaxWidth(),
                    )
                }

                Chip(
                    onClick = { showPicker = true },
                    label = { Text(if (host.isBlank()) "Set IP" else "Edit IP") },
                    colors = ChipDefaults.secondaryChipColors(),
                    modifier = Modifier.fillMaxWidth(),
                )

                // Wear parks Wi-Fi when Bluetooth-paired; this opens the watch's
                // Wi-Fi settings so the user can join the LAN the PC is on.
                Chip(
                    onClick = {
                        try {
                            context.startActivity(
                                Intent("com.google.android.clockwork.settings.connectivity.wifi.ADD_NETWORK_SETTINGS"),
                            )
                        } catch (_: Throwable) {
                            context.startActivity(Intent(android.provider.Settings.ACTION_WIFI_SETTINGS))
                        }
                    },
                    label = { Text("Wi-Fi") },
                    colors = ChipDefaults.secondaryChipColors(),
                    modifier = Modifier.fillMaxWidth(),
                )

                if (updateInfo?.updateAvailable == true) {
                    val target = updateInfo!!
                    Text(
                        "Update: v${target.latestVersion}",
                        style = MaterialTheme.typography.caption2,
                    )
                    Chip(
                        onClick = {
                            if (target.releaseUrl.isNotBlank()) {
                                context.startActivity(
                                    Intent(Intent.ACTION_VIEW, Uri.parse(target.releaseUrl)),
                                )
                            }
                        },
                        label = { Text("Open release") },
                        colors = ChipDefaults.secondaryChipColors(),
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            }
        }
    }
}

/**
 * Burn-in-safe always-on status view. The screen stays on (so Wi-Fi, sensors
 * and CPU keep running and tracking survives a wrist-down), but it renders a
 * near-black frame with dim text and drops the backlight to minimum. The text
 * hops to a new corner every minute so static pixels never sit long enough to
 * burn in on OLED. Back exits.
 */
@Composable
private fun ScreensaverScreen(
    connection: ConnectionState,
    endpoint: String?,
    onExit: () -> Unit,
) {
    BackHandler(onBack = onExit)
    val context = LocalContext.current
    val activity = context as? Activity

    DisposableEffect(Unit) {
        val window = activity?.window
        val previous = window?.attributes?.screenBrightness ?: -1f
        window?.let {
            it.attributes = it.attributes.apply { screenBrightness = 0.02f }
        }
        onDispose {
            window?.let {
                it.attributes = it.attributes.apply { screenBrightness = previous }
            }
        }
    }

    var slot by remember { mutableStateOf(0) }
    LaunchedEffect(Unit) {
        while (true) {
            delay(60_000)
            slot = (slot + 1) % 4
        }
    }
    val alignment = when (slot) {
        0 -> Alignment.TopStart
        1 -> Alignment.TopEnd
        2 -> Alignment.BottomEnd
        else -> Alignment.BottomStart
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black),
        contentAlignment = alignment,
    ) {
        Column(modifier = Modifier.padding(18.dp)) {
            Text(connection.name, color = Color(0xFF2E7D32), style = MaterialTheme.typography.caption2)
            if (!endpoint.isNullOrBlank()) {
                Text(endpoint, color = Color(0xFF1B3A1B), style = MaterialTheme.typography.caption2)
            }
            Text("back to exit", color = Color(0xFF161616), style = MaterialTheme.typography.caption2)
        }
    }
}
