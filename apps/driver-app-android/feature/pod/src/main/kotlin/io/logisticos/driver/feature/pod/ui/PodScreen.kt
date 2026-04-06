package io.logisticos.driver.feature.pod.ui

import android.content.Context
import android.graphics.Bitmap
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Tab
import androidx.compose.material3.TabRow
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.feature.pod.presentation.PodViewModel
import java.io.File
import java.io.FileOutputStream

private val Canvas = Color(0xFF050810)
private val Cyan = Color(0xFF00E5FF)
private val Green = Color(0xFF00FF88)

@Composable
fun PodScreen(
    taskId: String,
    requiresPhoto: Boolean,
    requiresSignature: Boolean,
    requiresOtp: Boolean,
    onCompleted: () -> Unit,
    viewModel: PodViewModel = hiltViewModel()
) {
    val state by viewModel.uiState.collectAsState()
    val context = LocalContext.current

    LaunchedEffect(Unit) {
        viewModel.setRequirements(taskId, requiresPhoto, requiresSignature, requiresOtp)
    }

    // Show success state inline — parent navigates via button tap, no LaunchedEffect re-fire risk
    if (state.isSubmitted) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(Canvas),
            contentAlignment = Alignment.Center
        ) {
            Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(16.dp)
            ) {
                Text(
                    "POD Submitted",
                    color = Green,
                    fontSize = 24.sp,
                    fontWeight = FontWeight.Bold
                )
                Button(
                    onClick = onCompleted,
                    colors = ButtonDefaults.buttonColors(containerColor = Green)
                ) {
                    Text("Continue", color = Canvas, fontWeight = FontWeight.Bold)
                }
            }
        }
        return
    }

    var selectedTab by remember { mutableIntStateOf(0) }
    val tabs = buildList {
        if (requiresPhoto) add("Photo")
        if (requiresSignature) add("Signature")
        if (requiresOtp) add("OTP")
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(Canvas)
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            if (requiresPhoto) StepIndicator("Photo", state.photoPath != null)
            if (requiresSignature) StepIndicator("Signature", state.signaturePath != null)
            if (requiresOtp) StepIndicator("OTP", state.otpToken != null)
        }

        if (tabs.size > 1) {
            TabRow(
                selectedTabIndex = selectedTab,
                containerColor = Color(0x0AFFFFFF)
            ) {
                tabs.forEachIndexed { index, tab ->
                    Tab(
                        selected = selectedTab == index,
                        onClick = { selectedTab = index },
                        text = {
                            Text(
                                tab,
                                color = if (selectedTab == index) Cyan else Color.White.copy(alpha = 0.5f)
                            )
                        }
                    )
                }
            }
        }

        Box(
            modifier = Modifier
                .weight(1f)
                .padding(16.dp)
        ) {
            when (tabs.getOrNull(selectedTab)) {
                "Signature" -> SignatureCanvas(
                    onSigned = { bitmap ->
                        val path = saveBitmap(context, bitmap, "sig_$taskId.png")
                        viewModel.onSignatureSaved(path)
                        if (selectedTab < tabs.size - 1) selectedTab++
                    },
                    modifier = Modifier.fillMaxSize()
                )
                "OTP" -> OtpPodSection(
                    otpToken = state.otpToken,
                    onOtpEntered = { token ->
                        viewModel.onOtpEntered(token)
                        if (selectedTab < tabs.size - 1) selectedTab++
                    }
                )
                else -> Box(
                    modifier = Modifier.fillMaxSize(),
                    contentAlignment = Alignment.Center
                ) {
                    Text(
                        "Photo capture coming soon",
                        color = Color.White.copy(alpha = 0.4f)
                    )
                }
            }
        }

        Button(
            onClick = viewModel::submit,
            enabled = state.canSubmit && !state.isSubmitting,
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp)
                .height(52.dp),
            colors = ButtonDefaults.buttonColors(containerColor = Cyan)
        ) {
            if (state.isSubmitting) {
                CircularProgressIndicator(color = Canvas, modifier = Modifier.size(20.dp))
            } else {
                Text("Submit POD", color = Canvas, fontWeight = FontWeight.Bold)
            }
        }
    }
}

@Composable
private fun StepIndicator(label: String, isDone: Boolean) {
    Row(
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Text(
            if (isDone) "+" else "o",
            color = if (isDone) Green else Color.White.copy(alpha = 0.3f),
            fontSize = 14.sp
        )
        Text(
            label,
            color = if (isDone) Green else Color.White.copy(alpha = 0.4f),
            fontSize = 12.sp
        )
    }
}

@Composable
private fun OtpPodSection(
    otpToken: String?,
    onOtpEntered: (String) -> Unit
) {
    var entered by remember { mutableStateOf("") }
    Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Text("Ask recipient for their OTP", color = Color.White, fontSize = 16.sp)
        OutlinedTextField(
            value = entered,
            onValueChange = { if (it.length <= 6) entered = it },
            label = { Text("6-digit OTP") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
            colors = OutlinedTextFieldDefaults.colors(
                focusedBorderColor = Cyan,
                unfocusedBorderColor = Color(0x14FFFFFF),
                focusedTextColor = Color.White,
                unfocusedTextColor = Color.White,
                focusedLabelColor = Cyan,
                unfocusedLabelColor = Color.White.copy(alpha = 0.5f)
            )
        )
        Button(
            onClick = { if (entered.length == 6) onOtpEntered(entered) },
            enabled = entered.length == 6,
            modifier = Modifier.fillMaxWidth(),
            colors = ButtonDefaults.buttonColors(containerColor = Cyan)
        ) {
            Text("Confirm OTP", color = Canvas)
        }
    }
}

private fun saveBitmap(context: Context, bitmap: Bitmap, filename: String): String {
    val file = File(context.filesDir, filename)
    FileOutputStream(file).use { out -> bitmap.compress(Bitmap.CompressFormat.PNG, 90, out) }
    return file.absolutePath
}
