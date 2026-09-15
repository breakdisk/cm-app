package io.logisticos.driver.feature.auth.ui

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.core.designsystem.*
import io.logisticos.driver.feature.auth.BuildConfig
import io.logisticos.driver.feature.auth.presentation.OtpViewModel
import kotlinx.coroutines.delay

@Composable
fun OtpScreen(
    identifier: String,
    tenantSlug: String,
    onAuthenticated: () -> Unit,
    viewModel: OtpViewModel = hiltViewModel()
) {
    val state by viewModel.uiState.collectAsState()
    val c = LocalMoveColors.current
    var resendSeconds by remember { mutableIntStateOf(60) }
    var resendTrigger by remember { mutableIntStateOf(0) }

    // Forward tenantSlug into the ViewModel once. Uses tenantSlug as the key
    // so a slug change (unlikely but safe) re-initialises the effect.
    LaunchedEffect(tenantSlug) {
        viewModel.setTenantSlug(tenantSlug)
    }

    LaunchedEffect(resendTrigger) {
        resendSeconds = 60
        while (resendSeconds > 0) {
            delay(1000)
            resendSeconds--
        }
    }
    LaunchedEffect(state.isSuccess) {
        if (state.isSuccess) {
            viewModel.onSuccessConsumed()
            onAuthenticated()
        }
    }

    AuthColumn {
        AuthBrandRow()

        Column(Modifier.padding(top = 20.dp, bottom = 4.dp)) {
            Text("Enter your code", color = c.ink, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 40.sp, lineHeight = 44.sp)
            Text(
                text = "We sent a 6-digit code to $identifier",
                color = c.muted,
                fontSize = 16.sp,
                modifier = Modifier.padding(top = 8.dp),
            )
        }

        MoveTextField(
            value = state.otp,
            onValueChange = viewModel::onOtpChanged,
            label = "6-digit code",
            keyboardType = KeyboardType.NumberPassword,
            large = true,
        )

        state.error?.let {
            MoveNotice(title = "Sign-in didn't work", body = it, tone = MoveTone.Penalty)
        }

        MoveBigButton(
            label = "VERIFY",
            onClick = { viewModel.verifyOtp(identifier, state.otp) },
            enabled = state.otp.length == 6,
            loading = state.isLoading,
        )

        MoveBigButton(
            label = if (resendSeconds > 0) "RESEND IN ${resendSeconds}s" else "RESEND CODE",
            onClick = { resendTrigger++; viewModel.resendOtp(identifier) },
            filled = false,
            enabled = resendSeconds == 0,
            height = 56.dp,
        )

        // ── Dev shortcut — stripped from release builds ───────────────────
        // Fills 123456 in one tap while waiting for Twilio approval.
        // BuildConfig.DEBUG is false in release APKs so this block is
        // dead-code-eliminated by ProGuard/R8 and never ships to production.
        if (BuildConfig.DEBUG) {
            TextButton(
                onClick = { viewModel.onOtpChanged("123456") },
                modifier = Modifier.align(Alignment.CenterHorizontally).heightIn(min = 48.dp),
            ) {
                Text(text = "⚡ Dev: fill 123456", color = c.muted, fontSize = 12.sp)
            }
        }
    }
}
