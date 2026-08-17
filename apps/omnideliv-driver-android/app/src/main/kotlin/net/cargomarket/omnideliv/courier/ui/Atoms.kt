package net.cargomarket.omnideliv.courier.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * The few pieces more than one screen draws.
 *
 * `internal` rather than `private`: these started life inside `ManifestScreen`,
 * and the offer inbox needs the same badge, the same money format and the same
 * colour for a vertical. Two copies of a money formatter is how one screen ends
 * up rounding differently from another.
 */

/**
 * Cents to a peso string.
 *
 * Integer arithmetic only. No float touches money anywhere in this app, for the
 * same reason the backend refuses to: `f64` cannot represent a cent exactly, and
 * a rounding error here is money created or destroyed on a courier's screen.
 */
internal fun pesos(cents: Long): String =
    "₱${cents / 100}.${(cents % 100).toString().padStart(2, '0')}"

/** The accent a vertical is drawn in, so a pharmacy run is recognisable at a glance. */
internal fun verticalColor(vertical: String): Color = when (vertical.lowercase()) {
    "pharmacy" -> Tokens.Plasma
    "florist" -> Tokens.Plasma
    "grocery" -> Tokens.Cyan
    else -> Tokens.Amber
}

/** Hot is warm-coloured, chilled is cold. Never colour alone — the word is always there too. */
internal fun temperatureColor(t: String): Color = when (t.lowercase()) {
    "hot" -> Tokens.Amber
    "chilled", "frozen" -> Tokens.Cyan
    else -> Tokens.TextMuted
}

/**
 * A small labelled chip.
 *
 * Carries its text as well as its colour, deliberately: WCAG 2.1 AA is a
 * platform non-negotiable and a courier squinting at a screen in direct sunlight
 * is exactly the reader it exists for. Nothing here is distinguished by colour
 * alone.
 */
@Composable
internal fun Badge(text: String, color: Color) {
    Box(
        Modifier
            .clip(RoundedCornerShape(5.dp))
            .background(Tokens.Surface)
            .border(1.dp, color, RoundedCornerShape(5.dp))
            .padding(horizontal = 7.dp, vertical = 3.dp),
    ) {
        Text(text, color = color, fontSize = 9.sp, fontWeight = FontWeight.Bold)
    }
}
