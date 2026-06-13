package io.logisticos.driver.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import io.logisticos.driver.core.network.branding.BrandingState

val Cyan = Color(0xFF00E5FF)
val Purple = Color(0xFFA855F7)
val Green = Color(0xFF00FF88)
val Amber = Color(0xFFFFAB00)
val Red = Color(0xFFFF3B5C)
val Canvas = Color(0xFF050810)
val GlassWhite = Color(0x0AFFFFFF)
val BorderWhite = Color(0x14FFFFFF)

/**
 * Parse a hex colour ("#RRGGBB" or "#AARRGGBB") to a Compose [Color].
 * Returns null for malformed/blank input so callers fall back to defaults.
 */
fun parseHexColor(hex: String?): Color? {
    val s = hex?.trim()?.removePrefix("#") ?: return null
    return try {
        when (s.length) {
            6 -> Color(("FF$s").toLong(16))
            8 -> Color(s.toLong(16))
            else -> null
        }
    } catch (_: NumberFormatException) {
        null
    }
}

private fun colorScheme(branding: BrandingState) = darkColorScheme(
    primary = parseHexColor(branding.primaryHex) ?: Cyan,
    secondary = parseHexColor(branding.secondaryHex) ?: Purple,
    tertiary = parseHexColor(branding.accentHex) ?: Green,
    background = Canvas,
    surface = Color(0xFF0A0E1A),
    error = Red,
    onPrimary = Canvas,
    onBackground = Color.White,
    onSurface = Color.White,
)

@Composable
fun DriverAppTheme(
    branding: BrandingState = BrandingState(),
    content: @Composable () -> Unit,
) {
    MaterialTheme(
        colorScheme = colorScheme(branding),
        content = content
    )
}
