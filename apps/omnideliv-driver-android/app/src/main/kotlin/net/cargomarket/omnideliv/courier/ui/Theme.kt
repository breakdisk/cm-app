package net.cargomarket.omnideliv.courier.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

/**
 * Design tokens for a screen used outdoors, in sunlight, one-handed, sometimes
 * with gloves.
 *
 * The platform palette and type are kept. What is dropped is glassmorphism:
 * `backdrop-blur` and translucent panels collapse contrast in direct sunlight,
 * which the specification names as this app's primary condition. Every surface
 * here is opaque. Recorded as a deliberate divergence from the design system,
 * not an oversight.
 */
object Tokens {
    val Base = Color(0xFF050810)
    val Surface = Color(0xFF12141B)
    val SurfaceRaised = Color(0xFF171A22)
    val Border = Color(0xFF232733)

    val Text = Color(0xFFFFFFFF)
    val TextMuted = Color(0xFF8A93A0)

    /** Primary action, and the "done" state on the rail. */
    val Signal = Color(0xFF00FF88)
    val SignalInk = Color(0xFF04140B)

    /** Pickup. */
    val Cyan = Color(0xFF00E5FF)
    /** Warning, and the offline / render-cache indicator. */
    val Amber = Color(0xFFFFAB00)
    /** Pharmacy and other handled verticals. */
    val Plasma = Color(0xFFC48BFF)

    /**
     * Minimum size for anything that advances state.
     *
     * 56, not Material's 48. A gloved thumb on a bike mount is the design case,
     * and the eight extra dp are the cheapest reliability in the app.
     */
    val MinTarget = 56.dp

    /**
     * Extra slop around a primary control.
     *
     * Android's default touch slop turns a tap during a bump into a swallowed
     * drag, which on a mount is the normal case rather than the exception.
     */
    val TouchSlop = 8.dp
}

private val Scheme = darkColorScheme(
    primary = Tokens.Signal,
    onPrimary = Tokens.SignalInk,
    background = Tokens.Base,
    onBackground = Tokens.Text,
    surface = Tokens.Surface,
    onSurface = Tokens.Text,
    error = Tokens.Amber,
)

@Composable
fun CourierTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = Scheme,
        typography = Typography(),
        content = content,
    )
}
