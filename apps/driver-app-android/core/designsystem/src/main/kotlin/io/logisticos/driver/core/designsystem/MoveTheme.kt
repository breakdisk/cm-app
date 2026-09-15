package io.logisticos.driver.core.designsystem

import android.graphics.Typeface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily

/**
 * Driver design tokens (the mobile design handoff).
 *
 * Every colour a screen draws comes from here. The handoff is explicit: sun mode
 * inverts the theme for direct sunlight, and it breaks the moment one screen ships
 * a literal colour. Night maps the design's cyan onto the platform palette in
 * CLAUDE.md; sun keeps the same roles at daylight contrast.
 */
@Immutable
data class MoveColors(
    val ground: Color,
    val ink: Color,
    val muted: Color,
    val panel: Color,
    val chip: Color,
    val hairline: Color,
    val accent: Color,
    val accentInk: Color,
    val accentPanel: Color,
    val accentBorder: Color,
    val amber: Color,
    val amberInk: Color,
    val amberPanel: Color,
    val amberBorder: Color,
    val success: Color,
    val penalty: Color,
    val penaltyPanel: Color,
    val penaltyBorder: Color,
    /** Opaque surface for sheets and dialogs, which cannot be translucent. */
    val surface: Color,
    val scrim: Color,
    val isSun: Boolean,
)

val NightColors = MoveColors(
    ground        = Color(0xFF050810),
    ink           = Color(0xFFFFFFFF),
    muted         = Color(0xB3F2F6FA),
    panel         = Color(0x0FFFFFFF),
    chip          = Color(0x14FFFFFF),
    hairline      = Color(0x1FFFFFFF),
    accent        = Color(0xFF00E5FF),
    accentInk     = Color(0xFF00171C),
    accentPanel   = Color(0x1A00E5FF),
    accentBorder  = Color(0x5900E5FF),
    amber         = Color(0xFFFFAB00),
    amberInk      = Color(0xFF1A1000),
    amberPanel    = Color(0x1FFFAB00),
    amberBorder   = Color(0x59FFAB00),
    success       = Color(0xFF00FF88),
    penalty       = Color(0xFFFFA0A0),
    penaltyPanel  = Color(0x1FFF8C8C),
    penaltyBorder = Color(0x47FF8C8C),
    surface       = Color(0xFF0D1220),
    scrim         = Color(0x99000000),
    isSun         = false,
)

val SunColors = MoveColors(
    ground        = Color(0xFFF3F5F7),
    ink           = Color(0xFF05080F),
    muted         = Color(0xFF3D4652),
    panel         = Color(0xFFFFFFFF),
    chip          = Color(0x0F05080F),
    hairline      = Color(0x2E05080F),
    accent        = Color(0xFF006B7A),
    accentInk     = Color(0xFFFFFFFF),
    accentPanel   = Color(0x14006B7A),
    accentBorder  = Color(0x80006B7A),
    amber         = Color(0xFF8A5A00),
    amberInk      = Color(0xFFFFFFFF),
    amberPanel    = Color(0x1A8A5A00),
    amberBorder   = Color(0x808A5A00),
    success       = Color(0xFF0B7A3E),
    penalty       = Color(0xFFB3261E),
    penaltyPanel  = Color(0x14B3261E),
    penaltyBorder = Color(0x80B3261E),
    surface       = Color(0xFFFFFFFF),
    scrim         = Color(0x66000000),
    isSun         = true,
)

/**
 * Sun mode for the whole app: every screen's header toggles the same state.
 * [toggle] is null where sun mode is not offered (the staging build, whose home
 * screen is not on these tokens) — headers then show no sun button.
 */
@Immutable
data class SunMode(val on: Boolean, val toggle: (() -> Unit)?)

val LocalMoveColors = staticCompositionLocalOf { NightColors }
val LocalSunMode = staticCompositionLocalOf { SunMode(on = false, toggle = null) }

/** The design's Barlow Condensed is not bundled; the platform condensed sans stands in. */
val Condensed: FontFamily = FontFamily(Typeface.create("sans-serif-condensed", Typeface.NORMAL))

@Composable
fun MoveTheme(sun: Boolean, onToggleSun: (() -> Unit)? = null, content: @Composable () -> Unit) {
    CompositionLocalProvider(
        LocalMoveColors provides if (sun) SunColors else NightColors,
        LocalSunMode provides SunMode(on = sun, toggle = onToggleSun),
        content = content,
    )
}
