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
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.feature.auth.presentation.PhoneViewModel

private val Canvas = Color(0xFF050810)
private val Cyan = Color(0xFF00E5FF)
private val BorderWhite = Color(0x14FFFFFF)

@Composable
fun PhoneScreen(
    onOtpSent: (phone: String) -> Unit,
    viewModel: PhoneViewModel = hiltViewModel()
) {
    val state by viewModel.uiState.collectAsState()

    LaunchedEffect(state.otpSent) {
        if (state.otpSent) onOtpSent(state.phone)
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
            verticalArrangement = Arrangement.spacedBy(24.dp)
        ) {
            Text(
                text = "LogisticOS",
                fontSize = 28.sp,
                fontWeight = FontWeight.Bold,
                color = Cyan
            )
            Text(
                text = "Driver App",
                fontSize = 16.sp,
                color = Color.White.copy(alpha = 0.6f)
            )

            Spacer(modifier = Modifier.height(16.dp))

            OutlinedTextField(
                value = state.phone,
                onValueChange = viewModel::onPhoneChanged,
                label = { Text("Phone Number") },
                placeholder = { Text("+63 912 345 6789") },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Phone),
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
                onClick = viewModel::sendOtp,
                enabled = !state.isLoading && state.phone.isNotBlank(),
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
