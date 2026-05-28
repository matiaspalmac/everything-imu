package cl.matiaspalma.everythingimu.mobile.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp

/** Token bundle mirroring apps/ui styles.css :root variables. */
data class Palette(
    val BgBase: Color,
    val BgPanel: Color,
    val BgElevated: Color,
    val FgPrimary: Color,
    val FgSecondary: Color,
    val FgMuted: Color,
    val Accent: Color,
    val AccentBright: Color,
    val AccentDeep: Color,
    val Danger: Color,
    val Success: Color,
    val Warn: Color,
    val Outline: Color,
    val WarnSoft: Color,
    val AccentSoft: Color,
)

/** Dark sumi-ink — soft blue accent hue 232. */
val DarkPalette = Palette(
    BgBase = Color(0xFF16191E),
    BgPanel = Color(0xFF1B1E25),
    BgElevated = Color(0xFF22262E),
    FgPrimary = Color(0xFFEDF0F4),
    FgSecondary = Color(0xFFB0B6C0),
    FgMuted = Color(0xFF7B838F),
    Accent = Color(0xFF9CB4DA),
    AccentBright = Color(0xFFB7CCEC),
    AccentDeep = Color(0xFF6B86B0),
    Danger = Color(0xFFD86A4F),
    Success = Color(0xFF5CC8A0),
    Warn = Color(0xFFD9C173),
    Outline = Color(0x33EDF0F4),
    WarnSoft = Color(0x24D9C173),
    AccentSoft = Color(0x249CB4DA),
)

/** Washi paper — same hue anchors, inverted lightness. */
val LightPalette = Palette(
    BgBase = Color(0xFFDFE2E6),
    BgPanel = Color(0xFFD2D6DC),
    BgElevated = Color(0xFFC2C7CF),
    FgPrimary = Color(0xFF2D3239),
    FgSecondary = Color(0xFF494F58),
    FgMuted = Color(0xFF6E7480),
    Accent = Color(0xFF5C75A6),
    AccentBright = Color(0xFF7790C3),
    AccentDeep = Color(0xFF425C88),
    Danger = Color(0xFFB05738),
    Success = Color(0xFF348964),
    Warn = Color(0xFF9C802C),
    Outline = Color(0x332D3239),
    WarnSoft = Color(0x2A9C802C),
    AccentSoft = Color(0x2A5C75A6),
)

enum class ThemeMode(val code: String) {
    Dark("dark"),
    Light("light"),
    System("system"),
    ;
    companion object {
        fun fromCode(code: String?): ThemeMode = when (code) {
            "light" -> Light
            "system" -> System
            else -> Dark
        }
    }
}

/**
 * Backing store mirrored across the app. Reads via @Composable observe via Compose snapshot;
 * non-composable reads (e.g. ColorScheme builders) snapshot the current value at call time.
 */
object EimuPalette {
    var active: Palette by mutableStateOf(DarkPalette)

    val BgBase: Color get() = active.BgBase
    val BgPanel: Color get() = active.BgPanel
    val BgElevated: Color get() = active.BgElevated
    val FgPrimary: Color get() = active.FgPrimary
    val FgSecondary: Color get() = active.FgSecondary
    val FgMuted: Color get() = active.FgMuted
    val Accent: Color get() = active.Accent
    val AccentBright: Color get() = active.AccentBright
    val AccentDeep: Color get() = active.AccentDeep
    val Danger: Color get() = active.Danger
    val Success: Color get() = active.Success
    val Warn: Color get() = active.Warn
    val Outline: Color get() = active.Outline
    val WarnSoft: Color get() = active.WarnSoft
    val AccentSoft: Color get() = active.AccentSoft
}

@Composable
private fun eimuScheme(): ColorScheme {
    val p = EimuPalette.active
    val base = if (p == LightPalette) lightColorScheme() else darkColorScheme()
    return base.copy(
        primary = p.Accent,
        onPrimary = p.BgBase,
        primaryContainer = p.AccentDeep,
        onPrimaryContainer = p.FgPrimary,
        secondary = p.AccentBright,
        onSecondary = p.BgBase,
        background = p.BgBase,
        onBackground = p.FgPrimary,
        surface = p.BgPanel,
        onSurface = p.FgPrimary,
        surfaceVariant = p.BgElevated,
        onSurfaceVariant = p.FgSecondary,
        outline = p.Outline,
        outlineVariant = p.FgMuted,
        error = p.Danger,
        onError = p.BgBase,
        tertiary = p.Success,
    )
}

private val EimuTypography = Typography(
    titleLarge = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.Medium,
        fontSize = 22.sp,
        letterSpacing = 0.sp,
    ),
    titleMedium = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.Medium,
        fontSize = 16.sp,
    ),
    bodyMedium = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.Normal,
        fontSize = 14.sp,
    ),
    bodySmall = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.Normal,
        fontSize = 12.sp,
    ),
)

/** Resolves the active palette before composing children — call once at root. */
@Composable
fun applyThemeMode(mode: ThemeMode) {
    val sysDark = isSystemInDarkTheme()
    val next = when (mode) {
        ThemeMode.Dark -> DarkPalette
        ThemeMode.Light -> LightPalette
        ThemeMode.System -> if (sysDark) DarkPalette else LightPalette
    }
    if (EimuPalette.active != next) EimuPalette.active = next
}

@Composable
fun EimuTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = eimuScheme(),
        typography = EimuTypography,
        content = content,
    )
}
