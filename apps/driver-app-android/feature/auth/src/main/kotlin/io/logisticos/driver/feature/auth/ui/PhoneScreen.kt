package io.logisticos.driver.feature.auth.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.feature.auth.presentation.PhoneViewModel

private val Canvas = Color(0xFF050810)
private val Cyan = Color(0xFF00E5FF)
private val BorderWhite = Color(0x14FFFFFF)
private val SurfaceWhite = Color(0x0AFFFFFF)
private val Amber = Color(0xFFFFAB00)

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

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Canvas)
            .imePadding(),
        contentAlignment = Alignment.TopCenter
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 32.dp)
                .padding(vertical = 48.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(24.dp)
        ) {
            Text("LogisticOS", fontSize = 28.sp, fontWeight = FontWeight.Bold, color = Cyan)
            Text("Driver App", fontSize = 16.sp, color = Color.White.copy(alpha = 0.6f))

            Spacer(modifier = Modifier.height(8.dp))

            // Phone / Email toggle
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(SurfaceWhite, RoundedCornerShape(12.dp))
                    .padding(4.dp),
                horizontalArrangement = Arrangement.spacedBy(4.dp)
            ) {
                listOf(false to "Phone", true to "Email").forEach { (emailMode, label) ->
                    val selected = state.isEmailMode == emailMode
                    Button(
                        onClick = { viewModel.onToggleMode(emailMode) },
                        modifier = Modifier.weight(1f),
                        shape = RoundedCornerShape(10.dp),
                        colors = ButtonDefaults.buttonColors(
                            containerColor = if (selected) Cyan else Color.Transparent,
                            contentColor = if (selected) Canvas else Color.White.copy(alpha = 0.5f)
                        ),
                        elevation = ButtonDefaults.buttonElevation(0.dp)
                    ) {
                        Text(label, fontWeight = if (selected) FontWeight.Bold else FontWeight.Normal)
                    }
                }
            }

            if (state.isEmailMode) {
                OutlinedTextField(
                    value = state.email,
                    onValueChange = viewModel::onEmailChanged,
                    label = { Text("Email Address") },
                    placeholder = { Text("driver@example.com") },
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email),
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                    colors = outlinedFieldColors()
                )
            } else {
                OutlinedTextField(
                    value = state.phone,
                    onValueChange = if (state.phoneLocked) {{}} else viewModel::onPhoneChanged,
                    label = { Text("Phone Number") },
                    placeholder = { Text("+63 912 345 6789") },
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Phone),
                    singleLine = true,
                    readOnly = state.phoneLocked,
                    modifier = Modifier.fillMaxWidth(),
                    colors = outlinedFieldColors(),
                    trailingIcon = if (state.phoneLocked) {
                        { Text("✓", color = Cyan, fontSize = 16.sp) }
                    } else null
                )
                if (state.phoneLocked) {
                    Text(
                        text = "Phone pre-filled from your invite link",
                        color = Cyan.copy(alpha = 0.7f),
                        fontSize = 12.sp
                    )
                }
            }

            // ── Company code section ──────────────────────────────────────────
            // Shown when no invite link was tapped (tenant not yet resolved).
            // Company code = tenant slug in v1 (no extra lookup needed).
            if (state.tenantSlug.isBlank()) {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedTextField(
                        value = state.companyCode,
                        onValueChange = viewModel::onCompanyCodeChanged,
                        label = { Text("Company Code") },
                        placeholder = { Text("e.g. cargomarket-ph") },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth(),
                        colors = outlinedFieldColors(),
                        supportingText = if (state.companyCodeError != null) {
                            { Text(state.companyCodeError!!, color = Color(0xFFFF3B5C)) }
                        } else null
                    )
                    Button(
                        onClick = viewModel::applyCompanyCode,
                        modifier = Modifier.fillMaxWidth(),
                        shape = RoundedCornerShape(10.dp),
                        colors = ButtonDefaults.buttonColors(
                            containerColor = Amber.copy(alpha = 0.15f),
                            contentColor = Amber
                        ),
                        elevation = ButtonDefaults.buttonElevation(0.dp)
                    ) {
                        Text("Confirm Company Code", fontWeight = FontWeight.SemiBold)
                    }
                }
            } else if (!state.phoneLocked) {
                // Slug resolved from company code — show confirmation chip.
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp)
                ) {
                    Surface(
                        shape = RoundedCornerShape(6.dp),
                        color = Cyan.copy(alpha = 0.12f)
                    ) {
                        Text(
                            text = "Company: ${state.tenantSlug}",
                            color = Cyan,
                            fontSize = 13.sp,
                            modifier = Modifier.padding(horizontal = 10.dp, vertical = 4.dp)
                        )
                    }
                }
            }

            if (state.error != null) {
                Text(text = state.error!!, color = Color(0xFFFF3B5C), fontSize = 14.sp)
            }

            val inputFilled = if (state.isEmailMode) state.email.isNotBlank() else state.phone.isNotBlank()
            Button(
                onClick = viewModel::sendOtp,
                enabled = !state.isLoading && inputFilled && state.canSend,
                modifier = Modifier
                    .fillMaxWidth()
                    .height(52.dp),
                colors = ButtonDefaults.buttonColors(containerColor = Cyan)
            ) {
                if (state.isLoading) {
                    CircularProgressIndicator(color = Canvas, modifier = Modifier.size(20.dp))
                } else {
                    Text("Send OTP", color = Canvas, fontWeight = FontWeight.Bold)
                }
            }
        }
    }
}

@Composable
private fun outlinedFieldColors() = OutlinedTextFieldDefaults.colors(
    focusedBorderColor = Cyan,
    unfocusedBorderColor = BorderWhite,
    focusedTextColor = Color.White,
    unfocusedTextColor = Color.White,
    focusedLabelColor = Cyan,
    unfocusedLabelColor = Color.White.copy(alpha = 0.5f),
    cursorColor = Cyan
)
