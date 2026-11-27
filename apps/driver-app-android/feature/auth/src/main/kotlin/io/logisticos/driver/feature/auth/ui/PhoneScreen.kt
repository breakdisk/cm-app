package io.logisticos.driver.feature.auth.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.core.designsystem.*
import io.logisticos.driver.feature.auth.presentation.PhoneViewModel

/**
 * @param onOtpSent  Called with the identifier (phone/email) AND the resolved
 *                   tenantSlug so the OTP screen can forward slug to OtpViewModel.
 */
@Composable
fun PhoneScreen(
    onOtpSent: (identifier: String, tenantSlug: String) -> Unit,
    viewModel: PhoneViewModel = hiltViewModel()
) {
    val state by viewModel.uiState.collectAsState()
    val c = LocalMoveColors.current

    // Read pending invite from SessionManager once on first composition.
    LaunchedEffect(Unit) {
        viewModel.loadPendingInvite()
    }

    // Navigate to OTP screen after sendOtp() succeeds.
    LaunchedEffect(state.otpSent) {
        if (state.otpSent) {
            viewModel.onOtpSentConsumed()
            onOtpSent(state.identifier, state.tenantSlug)
        }
    }

    AuthColumn {
        AuthBrandRow()

        Column(Modifier.padding(top = 20.dp, bottom = 4.dp)) {
            Text("Sign in to drive", color = c.ink, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 40.sp, lineHeight = 44.sp)
            Text(
                "We send you a one-time code — no password to remember.",
                color = c.muted,
                fontSize = 16.sp,
                modifier = Modifier.padding(top = 8.dp),
            )
        }

        // Phone / Email toggle
        MoveSegmented(
            options = listOf("Phone", "Email"),
            selected = if (state.isEmailMode) 1 else 0,
            onSelect = { viewModel.onToggleMode(it == 1) },
        )

        if (state.isEmailMode) {
            MoveTextField(
                value = state.email,
                onValueChange = viewModel::onEmailChanged,
                label = "Email address",
                placeholder = "driver@example.com",
                keyboardType = KeyboardType.Email,
            )
        } else {
            MoveTextField(
                value = state.phone,
                onValueChange = if (state.phoneLocked) {{}} else viewModel::onPhoneChanged,
                label = "Phone number",
                placeholder = "+63 912 345 6789",
                keyboardType = KeyboardType.Phone,
                readOnly = state.phoneLocked,
                trailingIcon = if (state.phoneLocked) {
                    { Icon(Icons.Filled.CheckCircle, contentDescription = "From your invite", tint = c.accent) }
                } else null,
            )
            if (state.phoneLocked) {
                Text(text = "Phone pre-filled from your invite link", color = c.accent, fontSize = 13.sp)
            }
        }

        // ── Company code section ──────────────────────────────────────────
        // Shown when no invite link was tapped (tenant not yet resolved).
        // Company code = tenant slug in v1 (no extra lookup needed).
        if (state.tenantSlug.isBlank()) {
            MoveTextField(
                value = state.companyCode,
                onValueChange = viewModel::onCompanyCodeChanged,
                label = "Company code",
                placeholder = "e.g. cargomarket-ph",
                supportingText = state.companyCodeError,
                isError = state.companyCodeError != null,
            )
            MoveBigButton(
                label = "CONFIRM COMPANY CODE",
                onClick = viewModel::applyCompanyCode,
                filled = false,
                tone = MoveTone.Amber,
                height = 56.dp,
            )
        } else if (!state.phoneLocked) {
            // Slug resolved from company code — show confirmation chip.
            Text(
                text = "Company: ${state.tenantSlug}",
                color = c.accent,
                fontSize = 14.sp,
                fontWeight = FontWeight.SemiBold,
                modifier = Modifier
                    .clip(RoundedCornerShape(999.dp))
                    .background(c.accentPanel)
                    .padding(horizontal = 14.dp, vertical = 8.dp),
            )
        }

        state.error?.let {
            MoveNotice(title = "Couldn't send the code", body = it, tone = MoveTone.Penalty)
        }

        val inputFilled = if (state.isEmailMode) state.email.isNotBlank() else state.phone.isNotBlank()
        MoveBigButton(
            label = "SEND CODE",
            onClick = viewModel::sendOtp,
            enabled = inputFilled && state.canSend,
            loading = state.isLoading,
        )
    }
}

/** The sign-in screens' scrolling column, clear of the keyboard and status bar. */
@Composable
internal fun AuthColumn(content: @Composable ColumnScope.() -> Unit) {
    val c = LocalMoveColors.current
    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(c.ground)
            .statusBarsPadding()
            .imePadding()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 20.dp, vertical = 16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
        content = content,
    )
}

/** Brand line with the sun toggle — sun mode reaches sign-in too. */
@Composable
internal fun AuthBrandRow() {
    val c = LocalMoveColors.current
    Row(
        modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(10.dp).clip(CircleShape).background(c.accent))
        Spacer(Modifier.width(10.dp))
        Text("LOGISTICOS · DRIVER", color = c.muted, fontSize = 12.sp, letterSpacing = 2.6.sp, modifier = Modifier.weight(1f))
        MoveSunToggle()
    }
}
