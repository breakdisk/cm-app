package io.logisticos.driver.core.designsystem

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.WbSunny
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** Which role a button, panel or notice takes its colour from. */
enum class MoveTone { Accent, Amber, Penalty, Neutral }

fun MoveColors.tone(tone: MoveTone): Color = when (tone) {
    MoveTone.Accent -> accent
    MoveTone.Amber -> amber
    MoveTone.Penalty -> penalty
    MoveTone.Neutral -> ink
}

fun MoveColors.tonePanel(tone: MoveTone): Color = when (tone) {
    MoveTone.Accent -> accentPanel
    MoveTone.Amber -> amberPanel
    MoveTone.Penalty -> penaltyPanel
    MoveTone.Neutral -> panel
}

fun MoveColors.toneBorder(tone: MoveTone): Color = when (tone) {
    MoveTone.Accent -> accentBorder
    MoveTone.Amber -> amberBorder
    MoveTone.Penalty -> penaltyBorder
    MoveTone.Neutral -> hairline
}

/**
 * A screen's header: uppercase kicker, condensed title, optional back, the
 * screen's own actions, and the sun toggle every driver screen carries.
 * All controls are 56 dp — the handoff: do not shrink driver controls.
 */
@Composable
fun MoveScreenHeader(
    label: String,
    title: String,
    onBack: (() -> Unit)? = null,
    actions: @Composable RowScope.() -> Unit = {},
) {
    val c = LocalMoveColors.current
    Row(
        Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        if (onBack != null) MoveSquareButton(Icons.AutoMirrored.Filled.ArrowBack, "Back", onBack)
        Column(Modifier.weight(1f)) {
            Text(label.uppercase(), color = c.muted, fontSize = 12.sp, letterSpacing = 2.6.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(title, color = c.ink, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 22.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        actions()
        MoveSunToggle()
    }
}

/** The sun button; renders nothing where sun mode is not offered. */
@Composable
fun MoveSunToggle() {
    val sun = LocalSunMode.current
    val toggle = sun.toggle ?: return
    MoveSquareButton(Icons.Filled.WbSunny, if (sun.on) "Sun mode on" else "Sun mode off", toggle, active = sun.on)
}

@Composable
fun MoveSquareButton(icon: ImageVector, label: String, onClick: () -> Unit, active: Boolean = false, modifier: Modifier = Modifier) {
    val c = LocalMoveColors.current
    val shape = RoundedCornerShape(16.dp)
    Box(
        modifier
            .size(56.dp)
            .clip(shape)
            .background(if (active) c.accentPanel else c.chip)
            .border(1.dp, if (active) c.accentBorder else c.hairline, shape)
            .clickable(role = Role.Button, onClickLabel = label, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, contentDescription = label, tint = if (active) c.accent else c.ink, modifier = Modifier.size(24.dp))
    }
}

@Composable
fun MovePanel(
    modifier: Modifier = Modifier,
    tone: MoveTone = MoveTone.Neutral,
    padding: Dp = 18.dp,
    content: @Composable ColumnScope.() -> Unit,
) {
    val c = LocalMoveColors.current
    val shape = RoundedCornerShape(22.dp)
    Column(
        modifier.fillMaxWidth().clip(shape).background(c.tonePanel(tone)).border(1.dp, c.toneBorder(tone), shape).padding(padding),
        content = content,
    )
}

@Composable
fun MoveBigButton(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    filled: Boolean = true,
    tone: MoveTone = MoveTone.Accent,
    icon: ImageVector? = null,
    enabled: Boolean = true,
    loading: Boolean = false,
    height: Dp = 68.dp,
) {
    val c = LocalMoveColors.current
    val base = if (tone == MoveTone.Neutral) c.accent else c.tone(tone)
    val onBase = if (tone == MoveTone.Amber) c.amberInk else c.accentInk
    val shape = RoundedCornerShape(18.dp)
    val solid = filled && enabled
    Row(
        modifier
            .fillMaxWidth()
            .height(height)
            .clip(shape)
            .background(if (solid) base else c.chip)
            .border(1.dp, if (solid) base else if (tone == MoveTone.Neutral) c.hairline else c.toneBorder(tone), shape)
            .clickable(enabled = enabled && !loading, role = Role.Button, onClick = onClick)
            .padding(horizontal = 16.dp),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val fg = when {
            !enabled -> c.muted
            filled -> onBase
            tone == MoveTone.Neutral || tone == MoveTone.Accent -> c.ink
            else -> base
        }
        if (loading) {
            CircularProgressIndicator(color = fg, strokeWidth = 2.5.dp, modifier = Modifier.size(24.dp))
        } else {
            if (icon != null) {
                Icon(icon, contentDescription = null, tint = if (filled || !enabled) fg else base, modifier = Modifier.size(24.dp))
                Spacer(Modifier.size(12.dp))
            }
            Text(label, color = fg, fontSize = 16.sp, fontWeight = FontWeight.Bold, letterSpacing = 1.4.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

/**
 * A full-width call to action with a kicker, a condensed title and a line of
 * detail — "Before you roll: scan the hub manifest".
 */
@Composable
fun MoveActionPanel(kicker: String, title: String, body: String, icon: ImageVector, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val c = LocalMoveColors.current
    val shape = RoundedCornerShape(20.dp)
    Row(
        modifier
            .fillMaxWidth()
            .heightIn(min = 72.dp)
            .clip(shape)
            .background(c.chip)
            .border(1.dp, c.accentBorder, shape)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 18.dp, vertical = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Icon(icon, contentDescription = null, tint = c.accent, modifier = Modifier.size(28.dp))
        Column(Modifier.weight(1f)) {
            Text(kicker.uppercase(), color = c.accent, fontSize = 12.sp, letterSpacing = 2.4.sp)
            Text(title, color = c.ink, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 24.sp, modifier = Modifier.padding(top = 3.dp))
            Text(body, color = c.muted, fontSize = 14.sp, modifier = Modifier.padding(top = 3.dp))
        }
        Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = null, tint = c.muted, modifier = Modifier.size(24.dp))
    }
}

@Composable
fun MoveLabel(text: String, modifier: Modifier = Modifier, dot: Color? = null) {
    val c = LocalMoveColors.current
    Row(modifier, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        if (dot != null) Box(Modifier.size(8.dp).clip(CircleShape).background(dot))
        Text(text.uppercase(), color = c.muted, fontSize = 12.sp, letterSpacing = 2.4.sp)
    }
}

@Composable
fun MoveStatTile(label: String, value: String, modifier: Modifier = Modifier, meta: String? = null, valueColor: Color? = null) {
    val c = LocalMoveColors.current
    val shape = RoundedCornerShape(18.dp)
    Column(modifier.clip(shape).background(c.panel).border(1.dp, c.hairline, shape).padding(16.dp)) {
        Text(label.uppercase(), color = c.muted, fontSize = 11.sp, letterSpacing = 2.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(value, color = valueColor ?: c.ink, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 28.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        if (meta != null) Text(meta, color = c.muted, fontSize = 12.sp)
    }
}

@Composable
fun MovePill(text: String, fg: Color, bg: Color, modifier: Modifier = Modifier) {
    Text(
        text.uppercase(),
        color = fg,
        fontSize = 11.sp,
        fontWeight = FontWeight.Bold,
        letterSpacing = 1.4.sp,
        maxLines = 1,
        modifier = modifier.clip(RoundedCornerShape(999.dp)).background(bg).padding(horizontal = 11.dp, vertical = 6.dp),
    )
}

/** Outlined state pill — "Approved", "Expiring" — as on the compliance design. */
@Composable
fun MoveStatePill(text: String, color: Color, modifier: Modifier = Modifier) {
    val shape = RoundedCornerShape(999.dp)
    Text(
        text.uppercase(),
        color = color,
        fontSize = 12.sp,
        fontWeight = FontWeight.Bold,
        letterSpacing = 1.2.sp,
        maxLines = 1,
        modifier = modifier.clip(shape).border(1.dp, color, shape).padding(horizontal = 11.dp, vertical = 7.dp),
    )
}

@Composable
fun MoveNotice(title: String, body: String, tone: MoveTone = MoveTone.Amber, modifier: Modifier = Modifier) {
    val c = LocalMoveColors.current
    MovePanel(modifier, tone = tone, padding = 16.dp) {
        Text(title, color = c.tone(tone), fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 18.sp)
        Text(body, color = c.muted, fontSize = 14.sp, modifier = Modifier.padding(top = 2.dp))
    }
}

/** Equal-width, 56 dp segments — tabs a gloved thumb can hit. */
@Composable
fun MoveSegmented(options: List<String>, selected: Int, onSelect: (Int) -> Unit, modifier: Modifier = Modifier) {
    val c = LocalMoveColors.current
    Row(modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        options.forEachIndexed { i, label ->
            val on = i == selected
            val shape = RoundedCornerShape(16.dp)
            Box(
                Modifier
                    .weight(1f)
                    .height(56.dp)
                    .clip(shape)
                    .background(if (on) c.accentPanel else c.chip)
                    .border(1.dp, if (on) c.accentBorder else c.hairline, shape)
                    .selectable(selected = on, role = Role.Tab, onClick = { onSelect(i) }),
                contentAlignment = Alignment.Center,
            ) {
                Text(label.uppercase(), color = if (on) c.accent else c.ink, fontSize = 14.sp, fontWeight = FontWeight.Bold, letterSpacing = 1.1.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

/** A 48 dp selectable chip for option rows that scroll sideways. */
@Composable
fun MoveChip(label: String, selected: Boolean, onClick: () -> Unit, tone: MoveTone = MoveTone.Accent, modifier: Modifier = Modifier) {
    val c = LocalMoveColors.current
    val shape = RoundedCornerShape(14.dp)
    Box(
        modifier
            .height(48.dp)
            .clip(shape)
            .background(if (selected) c.tonePanel(tone) else c.chip)
            .border(1.dp, if (selected) c.toneBorder(tone) else c.hairline, shape)
            .selectable(selected = selected, role = Role.RadioButton, onClick = onClick)
            .padding(horizontal = 16.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = if (selected) c.tone(tone) else c.ink, fontSize = 14.sp, fontWeight = if (selected) FontWeight.Bold else FontWeight.Medium, maxLines = 1)
    }
}

/** Camera frame with the design's corner brackets drawn over the content. */
@Composable
fun MoveViewfinder(modifier: Modifier = Modifier, content: @Composable BoxScope.() -> Unit = {}) {
    val c = LocalMoveColors.current
    val shape = RoundedCornerShape(22.dp)
    Box(modifier.fillMaxWidth().clip(shape).background(c.chip).border(1.dp, c.accentBorder, shape)) {
        content()
        Canvas(Modifier.matchParentSize().padding(20.dp)) {
            val len = 32.dp.toPx()
            val w = 3.dp.toPx()
            val right = size.width
            val bottom = size.height
            listOf(
                Offset(0f, 0f) to Offset(len, 0f), Offset(0f, 0f) to Offset(0f, len),
                Offset(right, 0f) to Offset(right - len, 0f), Offset(right, 0f) to Offset(right, len),
                Offset(0f, bottom) to Offset(len, bottom), Offset(0f, bottom) to Offset(0f, bottom - len),
                Offset(right, bottom) to Offset(right - len, bottom), Offset(right, bottom) to Offset(right, bottom - len),
            ).forEach { (from, to) -> drawLine(c.accent, from, to, strokeWidth = w, cap = StrokeCap.Round) }
        }
    }
}

@Composable
fun MoveDivider(modifier: Modifier = Modifier) {
    Box(modifier.fillMaxWidth().height(1.dp).background(LocalMoveColors.current.hairline))
}

@Composable
fun MoveDetailRow(label: String, value: String, valueColor: Color? = null) {
    val c = LocalMoveColors.current
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(label, color = c.muted, fontSize = 13.sp, modifier = Modifier.width(96.dp))
        Text(value, color = valueColor ?: c.ink, fontSize = 15.sp, modifier = Modifier.weight(1f))
    }
}

/** A 64 dp field — typed into with gloves, read in sunlight. */
@Composable
fun MoveTextField(
    value: String,
    onValueChange: (String) -> Unit,
    label: String,
    modifier: Modifier = Modifier,
    placeholder: String = "",
    keyboardType: KeyboardType = KeyboardType.Text,
    enabled: Boolean = true,
    readOnly: Boolean = false,
    singleLine: Boolean = true,
    mono: Boolean = false,
    large: Boolean = false,
    supportingText: String? = null,
    isError: Boolean = false,
    trailingIcon: (@Composable () -> Unit)? = null,
) {
    val c = LocalMoveColors.current
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        modifier = modifier.fillMaxWidth().heightIn(min = 64.dp),
        enabled = enabled,
        readOnly = readOnly,
        singleLine = singleLine,
        isError = isError,
        label = { Text(label) },
        placeholder = if (placeholder.isEmpty()) null else ({ Text(placeholder) }),
        supportingText = supportingText?.let { { Text(it) } },
        trailingIcon = trailingIcon,
        keyboardOptions = KeyboardOptions(keyboardType = keyboardType),
        textStyle = TextStyle(
            fontSize = if (large) 28.sp else 18.sp,
            letterSpacing = if (large) 6.sp else 0.sp,
            fontFamily = if (mono) FontFamily.Monospace else null,
            color = c.ink,
        ),
        shape = RoundedCornerShape(16.dp),
        colors = OutlinedTextFieldDefaults.colors(
            focusedBorderColor = c.accent,
            unfocusedBorderColor = c.hairline,
            disabledBorderColor = c.hairline,
            errorBorderColor = c.penalty,
            focusedLabelColor = c.accent,
            unfocusedLabelColor = c.muted,
            disabledLabelColor = c.muted,
            errorLabelColor = c.penalty,
            cursorColor = c.accent,
            errorCursorColor = c.penalty,
            focusedTextColor = c.ink,
            unfocusedTextColor = c.ink,
            disabledTextColor = c.muted,
            errorTextColor = c.ink,
            focusedContainerColor = c.panel,
            unfocusedContainerColor = c.panel,
            disabledContainerColor = c.panel,
            errorContainerColor = c.panel,
            focusedPlaceholderColor = c.muted,
            unfocusedPlaceholderColor = c.muted,
            disabledPlaceholderColor = c.muted,
            focusedSupportingTextColor = c.muted,
            unfocusedSupportingTextColor = c.muted,
            errorSupportingTextColor = c.penalty,
            focusedTrailingIconColor = c.accent,
            unfocusedTrailingIconColor = c.muted,
            disabledTrailingIconColor = c.muted,
            errorTrailingIconColor = c.penalty,
        ),
    )
}
