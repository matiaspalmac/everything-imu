package cl.matiaspalma.everythingimu.wear

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import android.content.Intent
import android.net.Uri
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.wear.compose.material.Button
import androidx.wear.compose.material.MaterialTheme
import androidx.wear.compose.material.Text
import cl.matiaspalma.everythingimu.core.net.ConnectionState
import cl.matiaspalma.everythingimu.core.tracking.TrackingController
import cl.matiaspalma.everythingimu.core.update.UpdateChecker
import cl.matiaspalma.everythingimu.wear.BuildConfig
import kotlinx.coroutines.launch

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
        val running by TrackingController.running.collectAsStateWithLifecycle()
        val connection by TrackingController.connection.collectAsStateWithLifecycle()
        val stats by TrackingController.clientStats.collectAsStateWithLifecycle()
        val lastError by TrackingController.lastError.collectAsStateWithLifecycle()
        val scope = rememberCoroutineScope()

        var host by remember { mutableStateOf("") }
        var port by remember { mutableStateOf(6969) }
        var updateInfo by remember { mutableStateOf<UpdateChecker.UpdateInfo?>(null) }
        LaunchedEffect(Unit) {
            host = TrackingController.savedHost()
            port = TrackingController.savedPort()
            // Background update check. Wear has no browser of its own, so the
            // ACTION_VIEW intent below typically prompts the user to open the
            // release page on the paired phone instead.
            updateInfo = UpdateChecker.check(BuildConfig.VERSION_NAME)
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

                Button(onClick = {
                    if (running) {
                        TrackingController.disconnect()
                        TrackingController.stop(context)
                    } else {
                        TrackingController.start(context)
                    }
                }) { Text(if (running) "Stop" else "Start") }

                Button(
                    onClick = {
                        scope.launch {
                            if (host.isNotBlank()) TrackingController.connect(host, port)
                        }
                    },
                    enabled = host.isNotBlank() && connection != ConnectionState.Connected,
                ) { Text("Connect") }

                Button(
                    onClick = { TrackingController.sendRecenter() },
                    enabled = connection == ConnectionState.Connected,
                ) { Text("Recenter") }

                if (updateInfo?.updateAvailable == true) {
                    val target = updateInfo!!
                    Text(
                        "Update: v${target.latestVersion}",
                        style = MaterialTheme.typography.caption2,
                    )
                    Button(onClick = {
                        if (target.releaseUrl.isNotBlank()) {
                            context.startActivity(
                                Intent(Intent.ACTION_VIEW, Uri.parse(target.releaseUrl)),
                            )
                        }
                    }) { Text("Open release") }
                }
            }
        }
    }
}
