package cl.matiaspalma.everythingimu.mobile.settings

import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import cl.matiaspalma.everythingimu.core.net.DiagnosticsReport
import cl.matiaspalma.everythingimu.core.net.NetworkDiagnostics
import cl.matiaspalma.everythingimu.core.prefs.AppPrefs
import cl.matiaspalma.everythingimu.core.service.BatteryOptHelper
import cl.matiaspalma.everythingimu.core.tracking.TrackingController
import cl.matiaspalma.everythingimu.core.update.UpdateChecker
import cl.matiaspalma.everythingimu.mobile.BuildConfig
import cl.matiaspalma.everythingimu.mobile.i18n.Language
import cl.matiaspalma.everythingimu.mobile.i18n.tr
import cl.matiaspalma.everythingimu.mobile.theme.CardTitle
import cl.matiaspalma.everythingimu.mobile.theme.EimuCard
import cl.matiaspalma.everythingimu.mobile.theme.EimuPalette
import cl.matiaspalma.everythingimu.mobile.theme.SectionHeader
import cl.matiaspalma.everythingimu.mobile.theme.ThemeMode
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@Composable
fun SettingsScreen() {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val prefs = remember { AppPrefs(ctx) }
    val langCode by prefs.language.collectAsStateWithLifecycle(initialValue = "en")
    val current = Language.fromCode(langCode)
    val themeCode by prefs.themeMode.collectAsStateWithLifecycle(initialValue = "dark")
    val themeMode = ThemeMode.fromCode(themeCode)
    val ignoring = BatteryOptHelper.isIgnoringOptimizations(ctx)
    val stats by TrackingController.clientStats.collectAsStateWithLifecycle()
    val lastError by TrackingController.lastError.collectAsStateWithLifecycle()
    val tps by TrackingController.tps.collectAsStateWithLifecycle()
    val battery by TrackingController.batteryLevel.collectAsStateWithLifecycle()
    val trackerName by prefs.trackerName.collectAsStateWithLifecycle(initialValue = "")
    val sendRateHz by prefs.sendRateHz.collectAsStateWithLifecycle(initialValue = 100)
    val magOn by prefs.magEnabled.collectAsStateWithLifecycle(initialValue = true)
    val shakeOn by prefs.shakeRecenter.collectAsStateWithLifecycle(initialValue = true)
    val t = tr

    var host by remember { mutableStateOf("") }
    var port by remember { mutableStateOf("6969") }
    var uuid by remember { mutableStateOf("") }
    LaunchedEffect(Unit) {
        host = TrackingController.savedHost()
        port = TrackingController.savedPort().toString()
        uuid = TrackingController.deviceUuid()
    }
    val portValid by remember(port) { derivedStateOf { port.toIntOrNull() != null } }
    var diagnostics by remember { mutableStateOf<DiagnosticsReport?>(null) }
    var diagnosing by remember { mutableStateOf(false) }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 12.dp)
            .verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(t.settings_title, style = MaterialTheme.typography.titleLarge, color = EimuPalette.FgPrimary)
            Text(t.settings_autosave, style = MaterialTheme.typography.bodySmall, color = EimuPalette.FgMuted)
        }

        SectionHeader(t.settings_server)
        EimuCard {
            CardTitle(t.settings_server)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                OutlinedTextField(
                    value = host,
                    onValueChange = {
                        host = it
                        autosave(scope, host, port, portValid)
                    },
                    label = { Text(t.field_host) },
                    singleLine = true,
                    modifier = Modifier.weight(2f),
                    colors = fieldColors(),
                )
                OutlinedTextField(
                    value = port,
                    onValueChange = {
                        port = it.filter { c -> c.isDigit() }.take(5)
                        autosave(scope, host, port, portValid)
                    },
                    label = { Text(t.field_port) },
                    singleLine = true,
                    modifier = Modifier.weight(1f),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                    colors = fieldColors(),
                )
            }
            Text(t.settings_server_hint, style = MaterialTheme.typography.bodySmall, color = EimuPalette.FgMuted)
        }

        SectionHeader(t.settings_tracker)
        EimuCard {
            CardTitle(t.settings_tracker_name)
            OutlinedTextField(
                value = trackerName,
                onValueChange = { scope.launch { prefs.setTrackerName(it.take(40)) } },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
                colors = fieldColors(),
            )
            Text(t.settings_tracker_name_hint, style = MaterialTheme.typography.bodySmall, color = EimuPalette.FgMuted)
        }

        SectionHeader(t.settings_sensors)
        EimuCard {
            CardTitle(t.settings_send_rate)
            Text(
                "${sendRateHz} Hz",
                style = MaterialTheme.typography.bodyMedium,
                color = EimuPalette.Accent,
                fontFamily = FontFamily.Monospace,
            )
            Slider(
                value = sendRateHz.toFloat(),
                onValueChange = { scope.launch { prefs.setSendRateHz(it.toInt()) } },
                valueRange = 30f..200f,
                steps = 16,
                colors = SliderDefaults.colors(
                    thumbColor = EimuPalette.Accent,
                    activeTrackColor = EimuPalette.Accent,
                    inactiveTrackColor = EimuPalette.BgElevated,
                ),
            )
            Text(t.settings_send_rate_hint, style = MaterialTheme.typography.bodySmall, color = EimuPalette.FgMuted)

            SwitchRow(
                label = t.settings_mag_enabled,
                hint = t.settings_mag_hint,
                checked = magOn,
                onChange = { scope.launch { prefs.setMagEnabled(it) } },
            )
            SwitchRow(
                label = t.settings_shake,
                hint = t.settings_shake_hint,
                checked = shakeOn,
                onChange = { scope.launch { prefs.setShakeRecenter(it) } },
            )
        }

        SectionHeader(t.settings_appearance)
        EimuCard {
            CardTitle(t.settings_theme)
            Column(verticalArrangement = Arrangement.spacedBy(6.dp), modifier = Modifier.fillMaxWidth()) {
                ThemeOption(t.settings_theme_dark, t.settings_theme_dark_hint, selected = themeMode == ThemeMode.Dark) {
                    scope.launch { prefs.setThemeMode(ThemeMode.Dark.code) }
                }
                ThemeOption(t.settings_theme_light, t.settings_theme_light_hint, selected = themeMode == ThemeMode.Light) {
                    scope.launch { prefs.setThemeMode(ThemeMode.Light.code) }
                }
                ThemeOption(t.settings_theme_system, t.settings_theme_system_hint, selected = themeMode == ThemeMode.System) {
                    scope.launch { prefs.setThemeMode(ThemeMode.System.code) }
                }
            }

            Text(
                t.settings_language.uppercase(),
                color = EimuPalette.FgMuted,
                fontWeight = FontWeight.SemiBold,
                fontSize = 10.sp,
                letterSpacing = 1.0.sp,
                modifier = Modifier.padding(top = 10.dp),
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                LangOption(t.settings_lang_en, current == Language.En) {
                    scope.launch { prefs.setLanguage(Language.En.code) }
                }
                LangOption(t.settings_lang_es, current == Language.Es) {
                    scope.launch { prefs.setLanguage(Language.Es.code) }
                }
            }
        }

        SectionHeader(t.settings_diagnostics)
        EimuCard {
            CardTitle(t.settings_diagnostics)
            Text(
                "${t.debug_fusion_native}: ${if (TrackingController.fusionAvailable) "loaded" else "fallback"}",
                color = if (TrackingController.fusionAvailable) EimuPalette.Success else EimuPalette.Warn,
                style = MaterialTheme.typography.bodySmall,
                fontFamily = FontFamily.Monospace,
            )
            Text(
                "${t.debug_packets}: sent ${stats.packetsSent} · recv ${stats.packetsReceived}",
                color = EimuPalette.FgSecondary,
                style = MaterialTheme.typography.bodySmall,
                fontFamily = FontFamily.Monospace,
            )
            Text(
                "${t.settings_tps}: ${"%.0f".format(tps)} pkt/s · ${t.settings_battery_level}: ${if (battery >= 0) "$battery%" else "—"}",
                color = EimuPalette.FgSecondary,
                style = MaterialTheme.typography.bodySmall,
                fontFamily = FontFamily.Monospace,
            )
            Text(
                "${t.debug_last_error}: ${lastError ?: t.debug_no_error}",
                color = if (lastError != null) EimuPalette.Danger else EimuPalette.FgMuted,
                style = MaterialTheme.typography.bodySmall,
                fontFamily = FontFamily.Monospace,
            )
            OutlinedButton(
                onClick = {
                    scope.launch {
                        diagnosing = true
                        diagnostics = withContext(Dispatchers.IO) {
                            NetworkDiagnostics.run(
                                ctx,
                                host.trim(),
                                port.toIntOrNull() ?: 6969,
                                TrackingController.deviceMac(),
                            )
                        }
                        diagnosing = false
                    }
                },
                modifier = Modifier.fillMaxWidth(),
            ) { Text(if (diagnosing) t.settings_diagnostics_running else t.settings_diagnostics_run) }
            diagnostics?.let { report ->
                Text(
                    "${t.debug_wifi}: ${if (report.wifiConnected) "ok" else "no"} · SSID: ${report.wifiSsid ?: "—"} · IP: ${report.localIp ?: "—"}",
                    style = MaterialTheme.typography.bodySmall,
                    color = EimuPalette.FgSecondary,
                )
                Text(
                    "${t.debug_reachable}: ${if (report.hostReachable) "yes" else "no"} · reply: ${if (report.serverResponded) "yes" else "no"}",
                    style = MaterialTheme.typography.bodySmall,
                    color = EimuPalette.FgSecondary,
                )
                if (report.hints.isNotEmpty()) {
                    for (hint in report.hints) {
                        Text("• $hint", style = MaterialTheme.typography.bodySmall, color = EimuPalette.FgSecondary)
                    }
                }
                OutlinedButton(onClick = { diagnostics = null }, modifier = Modifier.fillMaxWidth()) {
                    Text(t.debug_clear)
                }
            }
        }

        SectionHeader(t.settings_power)
        EimuCard {
            CardTitle(t.settings_battery)
            Text(
                if (ignoring) t.settings_battery_unrestricted else t.settings_battery_restricted,
                style = MaterialTheme.typography.bodySmall,
                color = if (ignoring) EimuPalette.Success else EimuPalette.Warn,
            )
            if (!ignoring) {
                Button(
                    onClick = { ctx.startActivity(BatteryOptHelper.requestIgnoreOptimizationsIntent(ctx)) },
                    colors = ButtonDefaults.buttonColors(
                        containerColor = EimuPalette.Accent,
                        contentColor = EimuPalette.BgBase,
                    ),
                    modifier = Modifier.fillMaxWidth(),
                ) { Text(t.settings_battery_allow) }
            }
            OutlinedButton(
                onClick = { ctx.startActivity(BatteryOptHelper.openOptimizationSettings()) },
                modifier = Modifier.fillMaxWidth(),
            ) { Text(t.settings_battery_open) }
            OutlinedButton(
                onClick = {
                    ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(BatteryOptHelper.oemGuideUrl())))
                },
                modifier = Modifier.fillMaxWidth(),
            ) { Text(t.settings_battery_oem) }
        }

        SectionHeader(t.settings_tips)
        EimuCard {
            CardTitle(t.settings_tips)
            TipRow(t.settings_tip_volume, t.settings_tip_volume_body)
            TipRow(t.settings_tip_wifi, t.settings_tip_wifi_body)
        }

        // Updates — checks GitHub releases on demand and surfaces a one-tap
        // deep-link to the release page. The OS package installer handles the
        // actual APK swap because Android forbids in-place self-replacement
        // without REQUEST_INSTALL_PACKAGES, which is too invasive a permission
        // to ask of every user.
        SectionHeader("Updates")
        EimuCard {
            CardTitle("Updates")
            UpdaterRow()
        }

        SectionHeader(t.settings_about)
        EimuCard {
            CardTitle(t.settings_about)
            AboutRow(t.settings_version, BuildConfig.VERSION_NAME, mono = true)
            AboutRow(t.settings_license, "MIT", mono = false)
            AboutRow(t.settings_protocol, "SlimeVR UDP", mono = false)
            AboutRow(t.settings_repo, "matiaspalmac/everything-imu", mono = true)
            AboutRow(t.settings_uuid, uuid.ifBlank { "—" }, mono = true)
            Text(t.settings_about_body, style = MaterialTheme.typography.bodySmall, color = EimuPalette.FgMuted)
        }
    }
}

private fun autosave(
    scope: kotlinx.coroutines.CoroutineScope,
    host: String,
    port: String,
    portValid: Boolean,
) {
    if (!portValid) return
    val p = port.toIntOrNull() ?: return
    scope.launch { TrackingController.persistServer(host.trim(), p) }
}

@Composable
private fun ThemeOption(
    title: String,
    hint: String,
    selected: Boolean,
    onClick: () -> Unit,
) {
    val border = if (selected) EimuPalette.Accent else EimuPalette.Outline
    val bg = if (selected) EimuPalette.WarnSoft else EimuPalette.BgElevated
    val fg = if (selected) EimuPalette.Accent else EimuPalette.FgSecondary
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .border(1.dp, border, RoundedCornerShape(8.dp))
            .background(bg, RoundedCornerShape(8.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Box(
            modifier = Modifier
                .size(10.dp)
                .background(if (selected) EimuPalette.Accent else EimuPalette.Outline, RoundedCornerShape(50)),
        )
        Column(modifier = Modifier.weight(1f)) {
            Text(title, color = fg, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
            Text(hint, color = EimuPalette.FgMuted, fontSize = 10.sp)
        }
    }
}

@Composable
private fun LangOption(label: String, selected: Boolean, onClick: () -> Unit) {
    val border = if (selected) EimuPalette.Accent else EimuPalette.Outline
    val bg = if (selected) EimuPalette.WarnSoft else EimuPalette.BgElevated
    val fg = if (selected) EimuPalette.Accent else EimuPalette.FgSecondary
    Box(
        modifier = Modifier
            .border(1.dp, border, RoundedCornerShape(8.dp))
            .background(bg, RoundedCornerShape(8.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 8.dp),
    ) {
        Text(label, color = fg, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
    }
}

@Composable
private fun SwitchRow(label: String, hint: String, checked: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(top = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Text(label, color = EimuPalette.FgPrimary, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.SemiBold)
            Text(hint, color = EimuPalette.FgMuted, fontSize = 10.sp)
        }
        Switch(
            checked = checked,
            onCheckedChange = onChange,
            colors = SwitchDefaults.colors(
                checkedThumbColor = EimuPalette.AccentBright,
                checkedTrackColor = EimuPalette.AccentDeep,
                uncheckedThumbColor = EimuPalette.FgMuted,
                uncheckedTrackColor = EimuPalette.BgElevated,
            ),
        )
    }
}

@Composable
private fun TipRow(title: String, body: String) {
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(title, color = EimuPalette.FgPrimary, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.SemiBold)
        Text(body, color = EimuPalette.FgSecondary, style = MaterialTheme.typography.bodySmall)
    }
}

@Composable
private fun UpdaterRow() {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var info by remember { mutableStateOf<UpdateChecker.UpdateInfo?>(null) }
    var checking by remember { mutableStateOf(false) }
    var checked by remember { mutableStateOf(false) }

    LaunchedEffect(Unit) {
        // One-shot check on screen open. Surface the result either way so the
        // user can verify the check ran and didn't silently fail behind some
        // captive-portal Wi-Fi.
        checking = true
        info = UpdateChecker.check(BuildConfig.VERSION_NAME)
        checking = false
        checked = true
    }

    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        val running = "v${BuildConfig.VERSION_NAME}"
        when {
            checking -> Text(
                "Checking for updates… (running $running)",
                color = EimuPalette.FgSecondary,
                style = MaterialTheme.typography.bodyMedium,
            )
            info == null && checked -> Text(
                "Update check failed (running $running). Check your network and retry.",
                color = EimuPalette.FgSecondary,
                style = MaterialTheme.typography.bodyMedium,
            )
            info?.updateAvailable == true -> {
                Text(
                    "Update available: v${info!!.latestVersion} (you have $running).",
                    color = EimuPalette.FgPrimary,
                    style = MaterialTheme.typography.bodyMedium,
                    fontWeight = FontWeight.SemiBold,
                )
                Text(
                    "The release page opens in your browser. Tap the phone APK to install.",
                    color = EimuPalette.FgMuted,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
            info != null -> Text(
                "You're on the latest release ($running).",
                color = EimuPalette.FgSecondary,
                style = MaterialTheme.typography.bodyMedium,
            )
        }
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            OutlinedButton(
                onClick = {
                    scope.launch {
                        checking = true
                        info = UpdateChecker.check(BuildConfig.VERSION_NAME)
                        checking = false
                        checked = true
                    }
                },
                enabled = !checking,
            ) { Text(if (checking) "Checking…" else "Check again") }
            val openable = info?.releaseUrl?.takeIf { it.isNotBlank() }
            Button(
                onClick = {
                    val url = openable ?: return@Button
                    context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
                },
                enabled = info?.updateAvailable == true && openable != null,
                colors = ButtonDefaults.buttonColors(
                    containerColor = EimuPalette.Accent,
                    contentColor = EimuPalette.BgBase,
                ),
            ) { Text("Open release") }
        }
    }
}

@Composable
private fun AboutRow(label: String, value: String, mono: Boolean) {
    Column(verticalArrangement = Arrangement.spacedBy(1.dp)) {
        Text(
            label.uppercase(),
            color = EimuPalette.FgMuted,
            fontSize = 10.sp,
            letterSpacing = 1.0.sp,
            fontWeight = FontWeight.Medium,
        )
        Text(
            value,
            color = EimuPalette.FgPrimary,
            style = MaterialTheme.typography.bodyMedium,
            fontFamily = if (mono) FontFamily.Monospace else FontFamily.SansSerif,
        )
    }
}

@Composable
private fun fieldColors() = TextFieldDefaults.colors(
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
