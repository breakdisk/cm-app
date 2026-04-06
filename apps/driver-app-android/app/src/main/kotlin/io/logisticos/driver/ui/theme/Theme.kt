package io.logisticos.driver.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

val Cyan = Color(0xFF00E5FF)
val Purple = Color(0xFFA855F7)
val Green = Color(0xFF00FF88)
val Amber = Color(0xFFFFAB00)
val Red = Color(0xFFFF3B5C)
val Canvas = Color(0xFF050810)
val GlassWhite = Color(0x0AFFFFFF)
val BorderWhite = Color(0x14FFFFFF)

private val DarkColorScheme = darkColorScheme(
    primary = Cyan,
    secondary = Purple,
    tertiary = Green,
    background = Canvas,
    surface = Color(0xFF0A0E1A),
    error = Red,
    onPrimary = Canvas,
    onBackground = Color.White,
    onSurface = Color.White,
)

@Composable
fun DriverAppTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = DarkColorScheme,
        content = content
    )
}
