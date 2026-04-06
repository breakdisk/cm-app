package io.logisticos.driver.feature.auth.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.feature.auth.presentation.OtpViewModel
import kotlinx.coroutines.delay

private val Canvas = Color(0xFF050810)
private val Cyan = Color(0xFF00E5FF)
private val BorderWhite = Color(0x14FFFFFF)

@Composable
fun OtpScreen(
    phone: String,
    onAuthenticated: () -> Unit,
    viewModel: OtpViewModel = hiltViewModel()
) {
    val state by viewModel.uiState.collectAsState()
    var resendSeconds by remember { mutableIntStateOf(60) }
    var resendTrigger by remember { mutableIntStateOf(0) }

    LaunchedEffect(resendTrigger) {
        resendSeconds = 60
        while (resendSeconds > 0) {
            delay(1000)
            resendSeconds--
        }
    }
    LaunchedEffect(state.isSuccess) {
        if (state.isSuccess) onAuthenticated()
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Canvas),
        contentAlignment = Alignment.Center
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 32.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(20.dp)
        ) {
            Text("Verify OTP", fontSize = 24.sp, fontWeight = FontWeight.Bold, color = Color.White)
            Text(
                text = "Enter the 6-digit code sent to $phone",
                fontSize = 14.sp,
                color = Color.White.copy(alpha = 0.6f),
                textAlign = TextAlign.Center
            )

            OutlinedTextField(
                value = state.otp,
                onValueChange = viewModel::onOtpChanged,
                label = { Text("6-digit OTP") },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword),
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
                colors = OutlinedTextFieldDefaults.colors(
                    focusedBorderColor = Cyan,
                    unfocusedBorderColor = BorderWhite,
                    focusedTextColor = Color.White,
                    unfocusedTextColor = Color.White,
                    focusedLabelColor = Cyan,
                    unfocusedLabelColor = Color.White.copy(alpha = 0.5f),
                    cursorColor = Cyan
                )
            )

            if (state.error != null) {
                Text(text = state.error!!, color = Color(0xFFFF3B5C), fontSize = 14.sp)
            }

            Button(
                onClick = { viewModel.verifyOtp(phone, state.otp) },
                enabled = state.otp.length == 6 && !state.isLoading,
                modifier = Modifier
                    .fillMaxWidth()
                    .height(52.dp),
                colors = ButtonDefaults.buttonColors(containerColor = Cyan)
            ) {
                if (state.isLoading) {
                    CircularProgressIndicator(color = Canvas, modifier = Modifier.size(20.dp))
                } else {
                    Text("Verify", color = Canvas, fontWeight = FontWeight.Bold)
                }
            }

            TextButton(
                onClick = { resendTrigger++; viewModel.resendOtp(phone) },
                enabled = resendSeconds == 0
            ) {
                Text(
                    text = if (resendSeconds > 0) "Resend in ${resendSeconds}s" else "Resend OTP",
                    color = if (resendSeconds == 0) Cyan else Color.White.copy(alpha = 0.4f)
                )
            }
        }
    }
}
