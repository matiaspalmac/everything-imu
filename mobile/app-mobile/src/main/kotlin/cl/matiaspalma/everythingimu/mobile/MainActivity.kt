package cl.matiaspalma.everythingimu.mobile

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.view.KeyEvent
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import cl.matiaspalma.everythingimu.core.fusion.Quaternion
import cl.matiaspalma.everythingimu.core.fusion.toEuler
import cl.matiaspalma.everythingimu.core.net.ClientStats
import cl.matiaspalma.everythingimu.core.net.ConnectionState
import cl.matiaspalma.everythingimu.core.net.SlimeVrClient
import cl.matiaspalma.everythingimu.core.prefs.AppPrefs
import cl.matiaspalma.everythingimu.core.sensors.SensorSample
import cl.matiaspalma.everythingimu.core.tracking.TrackingController
import cl.matiaspalma.everythingimu.core.tracking.SensorAvailability
import cl.matiaspalma.everythingimu.mobile.calibration.CalibrationScreen
import cl.matiaspalma.everythingimu.mobile.haptics.HapticsScreen
import cl.matiaspalma.everythingimu.mobile.i18n.Language
import cl.matiaspalma.everythingimu.mobile.i18n.LocalStrings
import cl.matiaspalma.everythingimu.mobile.i18n.stringsFor
import cl.matiaspalma.everythingimu.mobile.i18n.tr
import cl.matiaspalma.everythingimu.mobile.settings.SettingsScreen
import cl.matiaspalma.everythingimu.mobile.theme.CardTitle
import cl.matiaspalma.everythingimu.mobile.theme.EimuCard
import cl.matiaspalma.everythingimu.mobile.theme.EimuPalette
import cl.matiaspalma.everythingimu.mobile.theme.EimuShell
import cl.matiaspalma.everythingimu.mobile.theme.EimuTab
import cl.matiaspalma.everythingimu.mobile.theme.EimuTheme
import cl.matiaspalma.everythingimu.mobile.theme.ThemeMode
import cl.matiaspalma.everythingimu.mobile.theme.applyThemeMode
import cl.matiaspalma.everythingimu.mobile.theme.SectionHeader
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlin.math.PI

class MainActivity : ComponentActivity() {

    private val requestNotificationPermission =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { /* ignored */ }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        TrackingController.ensureInit(this)
        maybeAskNotificationPermission()
        setContent {
            RootScreen()
        }
    }

    private fun maybeAskNotificationPermission() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        val granted = ContextCompat.checkSelfPermission(
            this, Manifest.permission.POST_NOTIFICATIONS,
        ) == PackageManager.PERMISSION_GRANTED
        if (!granted) requestNotificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
    }

    private var volDownTimeNanos: Long = 0L
    private var volUpTimeNanos: Long = 0L

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        if (keyCode == KeyEvent.KEYCODE_VOLUME_DOWN || keyCode == KeyEvent.KEYCODE_VOLUME_UP) {
            if (event.repeatCount == 0) {
                if (keyCode == KeyEvent.KEYCODE_VOLUME_DOWN) {
                    volDownTimeNanos = System.nanoTime()
                } else {
                    volUpTimeNanos = System.nanoTime()
                }
            } else if (event.isLongPress) {
                TrackingController.sendRecenter()
                return true
            }
        }
        return super.onKeyDown(keyCode, event)
    }

    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean {
        if (keyCode == KeyEvent.KEYCODE_VOLUME_DOWN || keyCode == KeyEvent.KEYCODE_VOLUME_UP) {
            val downAt = if (keyCode == KeyEvent.KEYCODE_VOLUME_DOWN) volDownTimeNanos else volUpTimeNanos
            val heldMs = if (downAt == 0L) 0L else (System.nanoTime() - downAt) / 1_000_000
            if (heldMs >= LONG_PRESS_MS) {
                TrackingController.sendRecenter()
                return true
            }
        }
        return super.onKeyUp(keyCode, event)
    }

    companion object {
        private const val LONG_PRESS_MS = 600L
    }
}

@Composable
private fun RootScreen() {
    val ctx = LocalContext.current
    val prefs = remember { AppPrefs(ctx) }
    val langCode by prefs.language.collectAsStateWithLifecycle(initialValue = "en")
    val lang = Language.fromCode(langCode)
    val strings = stringsFor(lang)
    val themeCode by prefs.themeMode.collectAsStateWithLifecycle(initialValue = "dark")
    applyThemeMode(ThemeMode.fromCode(themeCode))

    // Auto-connect once per app launch — at the root, not inside a tab, so
    // switching tabs (which disposes and recomposes the page) can't re-trigger
    // a connect and bounce the session.
    LaunchedEffect(Unit) {
        val savedHost = TrackingController.savedHost()
        if (savedHost.isNotBlank() && prefs.autoConnect.first()) {
            TrackingController.start(ctx)
            TrackingController.connect(savedHost, TrackingController.savedPort())
            if (prefs.autostartHaptics.first()) TrackingController.hapticBridge()?.start()
        }
    }

    EimuTheme {
    CompositionLocalProvider(LocalStrings provides strings) {
        Surface(modifier = Modifier.fillMaxSize(), color = EimuPalette.BgBase) {
        val tabs = EimuTab.values()
        val pagerState = rememberPagerState(initialPage = 0, pageCount = { tabs.size })
        val scope = rememberCoroutineScope()
        var currentTab by remember { mutableStateOf(EimuTab.Home) }

        LaunchedEffect(pagerState) {
            snapshotFlow { pagerState.currentPage }.collect { page ->
                currentTab = tabs[page]
            }
        }

        EimuShell(
            current = currentTab,
            onSelect = { tab ->
                scope.launch { pagerState.animateScrollToPage(tab.ordinal) }
            },
            labelOf = { tab -> if (lang == Language.Es) tab.labelEs else tab.labelEn },
        ) { inner ->
            HorizontalPager(
                state = pagerState,
                modifier = Modifier.fillMaxSize().padding(inner),
                beyondViewportPageCount = 0,
            ) { page ->
                when (tabs[page]) {
                    EimuTab.Home -> HomeScreen()
                    EimuTab.Calibrate -> CalibrationScreen(onClose = {
                        scope.launch { pagerState.animateScrollToPage(EimuTab.Home.ordinal) }
                    })
                    EimuTab.Haptics -> HapticsScreen(onClose = {
                        scope.launch { pagerState.animateScrollToPage(EimuTab.Home.ordinal) }
                    })
                    EimuTab.Settings -> SettingsScreen()
                }
            }
        }
        }
    }
    }
}

@Composable
private fun HomeScreen() {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val snapshot by TrackingController.snapshot.collectAsStateWithLifecycle()
    val rates by TrackingController.rates.collectAsStateWithLifecycle()
    val quat by TrackingController.mountedQuaternion.collectAsStateWithLifecycle()
    val connection by TrackingController.connection.collectAsStateWithLifecycle()
    val clientStats by TrackingController.clientStats.collectAsStateWithLifecycle()
    val lastError by TrackingController.lastError.collectAsStateWithLifecycle()
    val availability = remember { TrackingController.availability }
    val prefs = remember { AppPrefs(context) }
    val sensorWarningDismissed by prefs.sensorWarningDismissed.collectAsStateWithLifecycle(initialValue = false)
    val autostartHaptics by prefs.autostartHaptics.collectAsStateWithLifecycle(initialValue = false)
    val t = tr

    var host by remember { mutableStateOf("") }
    var port by remember { mutableStateOf("6969") }
    LaunchedEffect(Unit) {
        host = TrackingController.savedHost()
        port = TrackingController.savedPort().toString()
    }

    val canConnect by remember(host, port) {
        derivedStateOf { host.isNotBlank() && port.toIntOrNull() != null }
    }
    val missingRequiredSensors = !availability.gyro || !availability.accel

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 12.dp)
            .verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        HeaderRow(connection, clientStats)

        if (missingRequiredSensors && !sensorWarningDismissed) {
            SensorWarningCard(
                availability = availability,
                onDismiss = { scope.launch { prefs.setSensorWarningDismissed(true) } },
            )
        }

        SectionHeader(t.home_connection)
        ConnectCard(
            host = host,
            port = port,
            connection = connection,
            lastError = lastError,
            enabled = canConnect,
            onHostChange = { host = it },
            onPortChange = { port = it.filter { c -> c.isDigit() }.take(5) },
            onConnect = {
                val p = port.toIntOrNull() ?: 6969
                // One action: bring up the foreground service (sensors + wakelock
                // + Wi-Fi lock) and open the UDP connection together.
                TrackingController.start(context)
                scope.launch { TrackingController.connect(host.trim(), p) }
                if (autostartHaptics) TrackingController.hapticBridge()?.start()
            },
            onDisconnect = {
                TrackingController.disconnect()
                TrackingController.stop(context)
            },
            onDiscover = {
                val p = port.toIntOrNull() ?: 6969
                scope.launch {
                    val found = withContext(Dispatchers.IO) {
                        val mac = TrackingController.deviceMac()
                        SlimeVrClient.discover(context, mac, p)
                    }
                    if (found != null) host = found.hostAddress ?: host
                }
            },
        )

        CheckRow(
            label = "Autostart haptic server",
            checked = autostartHaptics,
            onChange = { scope.launch { prefs.setAutostartHaptics(it) } },
        )

        SectionHeader(t.home_fusion)
        FusionCard(quat)

        SectionHeader(t.home_sensors)
        SensorCard(t.sensors_gyro, snapshot.gyro, availability.gyro, rates.gyroHz, units = "rad/s")
        SensorCard(t.sensors_accel, snapshot.accel, availability.accel, rates.accelHz, units = "m/s²")
        SensorCard(t.sensors_mag, snapshot.mag, availability.mag, rates.magHz, units = "µT")
    }
}

@Composable
private fun CheckRow(label: String, checked: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable { onChange(!checked) },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Checkbox(
            checked = checked,
            onCheckedChange = onChange,
            colors = CheckboxDefaults.colors(
                checkedColor = EimuPalette.Accent,
                uncheckedColor = EimuPalette.FgMuted,
                checkmarkColor = EimuPalette.BgBase,
            ),
        )
        Text(label, color = EimuPalette.FgSecondary, style = MaterialTheme.typography.bodyMedium)
    }
}

@Composable
private fun HeaderRow(connection: ConnectionState, stats: ClientStats) {
    val t = tr
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(t.home_title, style = MaterialTheme.typography.titleLarge, color = EimuPalette.FgPrimary)
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Dot(connection.color())
            Text(connection.labelOf(t), color = EimuPalette.FgSecondary, style = MaterialTheme.typography.bodyMedium)
            if (stats.targetEndpoint != null) {
                Text("· ${stats.targetEndpoint}", color = EimuPalette.FgMuted, style = MaterialTheme.typography.bodySmall)
            }
        }
    }
}

@Composable
private fun Dot(color: Color) {
    Box(
        modifier = Modifier
            .size(10.dp)
            .clip(CircleShape)
            .background(color),
    )
}

private fun ConnectionState.color(): Color = when (this) {
    ConnectionState.Connected -> EimuPalette.Success
    ConnectionState.Connecting, ConnectionState.Reconnecting -> EimuPalette.Warn
    ConnectionState.Failed -> EimuPalette.Danger
    ConnectionState.Disconnected -> EimuPalette.FgMuted
}

private fun ConnectionState.labelOf(t: cl.matiaspalma.everythingimu.mobile.i18n.Strings): String = when (this) {
    ConnectionState.Connected -> t.conn_connected
    ConnectionState.Connecting -> t.conn_connecting
    ConnectionState.Reconnecting -> t.conn_reconnecting
    ConnectionState.Failed -> t.conn_failed
    ConnectionState.Disconnected -> t.conn_disconnected
}

@Composable
private fun ConnectCard(
    host: String,
    port: String,
    connection: ConnectionState,
    lastError: String?,
    enabled: Boolean,
    onHostChange: (String) -> Unit,
    onPortChange: (String) -> Unit,
    onConnect: () -> Unit,
    onDisconnect: () -> Unit,
    onDiscover: () -> Unit,
) {
    val t = tr
    EimuCard {
        CardTitle(t.home_connection)
        Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedTextField(
                value = host,
                onValueChange = onHostChange,
                label = { Text(t.field_host) },
                singleLine = true,
                modifier = Modifier.weight(2f),
                colors = eimuFieldColors(),
            )
            OutlinedTextField(
                value = port,
                onValueChange = onPortChange,
                label = { Text(t.field_port) },
                singleLine = true,
                modifier = Modifier.weight(1f),
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                colors = eimuFieldColors(),
            )
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            val connected = connection == ConnectionState.Connected || connection == ConnectionState.Connecting || connection == ConnectionState.Reconnecting
            Button(
                onClick = { if (connected) onDisconnect() else onConnect() },
                enabled = enabled || connected,
                colors = ButtonDefaults.buttonColors(
                    containerColor = if (connected) EimuPalette.Danger else EimuPalette.Accent,
                    contentColor = EimuPalette.BgBase,
                ),
                modifier = Modifier.weight(1f),
            ) {
                Text(if (connected) t.action_disconnect else t.action_connect)
            }
            OutlinedButton(
                onClick = onDiscover,
                modifier = Modifier.weight(1f),
            ) { Text(t.action_discover) }
        }
        if (!lastError.isNullOrBlank()) {
            Text(lastError, style = MaterialTheme.typography.bodySmall, color = EimuPalette.Danger)
        }
    }
}

@Composable
private fun FusionCard(q: Quaternion) {
    val t = tr
    val scope = rememberCoroutineScope()
    EimuCard {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            CardTitle(t.home_fusion)
            OutlinedButton(onClick = {
                scope.launch { TrackingController.sendRecenter() }
            }) { Text(t.action_recenter) }
        }
        Text(
            "quat  w=${"% .4f".format(q.w)}  x=${"% .4f".format(q.x)}  y=${"% .4f".format(q.y)}  z=${"% .4f".format(q.z)}",
            fontFamily = FontFamily.Monospace,
            color = EimuPalette.FgSecondary,
            style = MaterialTheme.typography.bodySmall,
        )
        val e = q.toEuler()
        val deg = 180.0 / PI
        Text(
            "euler yaw=${"% .1f".format(e.yaw * deg)}°  pitch=${"% .1f".format(e.pitch * deg)}°  roll=${"% .1f".format(e.roll * deg)}°",
            fontFamily = FontFamily.Monospace,
            color = EimuPalette.FgSecondary,
            style = MaterialTheme.typography.bodySmall,
        )
    }
}

@Composable
private fun SensorCard(
    label: String,
    sample: SensorSample,
    available: Boolean,
    hz: Float,
    units: String,
) {
    val t = tr
    EimuCard {
        val header = when {
            !available -> "$label  · ${t.sensors_unavailable}"
            hz > 0f -> "$label  · ${"%.1f".format(hz)} Hz"
            else -> "$label  · ${t.sensors_waiting}"
        }
        CardTitle(header)
        if (available) {
            Text("x = ${"% .4f".format(sample.value.x)} $units", fontFamily = FontFamily.Monospace, color = EimuPalette.FgSecondary)
            Text("y = ${"% .4f".format(sample.value.y)} $units", fontFamily = FontFamily.Monospace, color = EimuPalette.FgSecondary)
            Text("z = ${"% .4f".format(sample.value.z)} $units", fontFamily = FontFamily.Monospace, color = EimuPalette.FgSecondary)
            if (sample.uncalibrated) {
                Text(t.sensors_uncalibrated, style = MaterialTheme.typography.bodySmall, color = EimuPalette.FgMuted)
            }
        }
    }
}

@Composable
private fun SensorWarningCard(
    availability: SensorAvailability,
    onDismiss: () -> Unit,
) {
    val t = tr
    EimuCard {
        Text(t.sensors_missing_title, style = MaterialTheme.typography.titleMedium, color = EimuPalette.Danger)
        Text(t.sensors_missing_body, style = MaterialTheme.typography.bodySmall, color = EimuPalette.FgSecondary)
        OutlinedButton(onClick = onDismiss, modifier = Modifier.fillMaxWidth()) { Text(t.action_dismiss) }
        // suppress unused param warning
        availability.gyro
    }
}

@Composable
private fun eimuFieldColors() = TextFieldDefaults.colors(
    focusedTextColor = EimuPalette.FgPrimary,
    unfocusedTextColor = EimuPalette.FgPrimary,
    focusedContainerColor = EimuPalette.BgElevated,
    unfocusedContainerColor = EimuPalette.BgElevated,
    cursorColor = EimuPalette.Accent,
    focusedLabelColor = EimuPalette.Accent,
    unfocusedLabelColor = EimuPalette.FgMuted,
    focusedIndicatorColor = EimuPalette.Accent,
    unfocusedIndicatorColor = EimuPalette.Outline,
)
