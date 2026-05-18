package io.logisticos.driver.feature.pod.ui

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.graphics.Bitmap
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.CameraAlt
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import androidx.core.content.FileProvider
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.core.common.ImageCompressor
import io.logisticos.driver.feature.pod.presentation.FailureReason
import io.logisticos.driver.feature.pod.presentation.PodViewModel
import java.io.File
import java.io.FileOutputStream
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

private val Canvas = Color(0xFF050810)
private val Cyan   = Color(0xFF00E5FF)
private val Green  = Color(0xFF00FF88)
private val Amber  = Color(0xFFFFAB00)
private val Red    = Color(0xFFFF3B5C)
private val Glass  = Color(0x0AFFFFFF)
private val Border = Color(0x14FFFFFF)

@Composable
fun PodScreen(
    taskId: String,
    shipmentId: String = "",
    recipientName: String = "",
    requiresPhoto: Boolean,
    requiresSignature: Boolean,
    requiresOtp: Boolean,
    isCod: Boolean = false,
    codAmount: Double = 0.0,
    onCompleted: () -> Unit,
    onFailed: (reason: String) -> Unit = {},
    onBack: () -> Unit = {},
    viewModel: PodViewModel = hiltViewModel()
) {
    val state by viewModel.uiState.collectAsState()
    val context = LocalContext.current

    LaunchedEffect(taskId) {
        viewModel.setRequirements(
            taskId = taskId,
            shipmentId = shipmentId,
            recipientName = recipientName,
            requiresPhoto = requiresPhoto,
            requiresSignature = requiresSignature,
            requiresOtp = requiresOtp,
            isCod = isCod,
            codAmount = codAmount
        )
        // Always load task meta to populate GPS fallback coordinates (taskLat/taskLng).
        // Also backfills shipmentId/recipientName when not passed via nav args.
        viewModel.loadTaskMeta(taskId)
    }

    // Success state
    if (state.isSubmitted) {
        Box(
            modifier = Modifier.fillMaxSize().background(Canvas),
            contentAlignment = Alignment.Center
        ) {
            Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(16.dp)
            ) {
                Box(
                    modifier = Modifier
                        .size(80.dp)
                        .clip(RoundedCornerShape(40.dp))
                        .background(Green.copy(alpha = 0.12f))
                        .border(2.dp, Green.copy(alpha = 0.4f), RoundedCornerShape(40.dp)),
                    contentAlignment = Alignment.Center
                ) {
                    Icon(Icons.Default.Check, contentDescription = null, tint = Green, modifier = Modifier.size(40.dp))
                }
                Text("POD Submitted", color = Green, fontSize = 24.sp, fontWeight = FontWeight.Bold)
                if (isCod && state.codCollected) {
                    Text(
                        "COD ₱${"%,.2f".format(codAmount)} collected",
                        color = Amber,
                        fontSize = 14.sp,
                        fontWeight = FontWeight.SemiBold
                    )
                }
                Button(
                    onClick = onCompleted,
                    modifier = Modifier.width(200.dp).height(48.dp),
                    colors = ButtonDefaults.buttonColors(containerColor = Green),
                    shape = RoundedCornerShape(12.dp)
                ) {
                    Text("Continue", color = Canvas, fontWeight = FontWeight.Bold)
                }
            }
        }
        return
    }

    // Failure reason sheet
    if (state.showFailureSheet) {
        FailureReasonSheet(
            onSelect = { reason ->
                viewModel.submitFailure(taskId, reason) { onFailed(reason.name) }
            },
            onDismiss = { viewModel.dismissFailureSheet() }
        )
        return
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(Canvas)
            .verticalScroll(rememberScrollState())
    ) {
        // Header
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 16.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                IconButton(
                    onClick = onBack,
                    modifier = Modifier.size(36.dp),
                    colors = IconButtonDefaults.iconButtonColors(contentColor = Color.White.copy(alpha = 0.7f)),
                ) {
                    Icon(
                        imageVector = Icons.AutoMirrored.Filled.ArrowBack,
                        contentDescription = "Back",
                        modifier = Modifier.size(20.dp),
                    )
                }
                Column {
                    Text("PROOF OF DELIVERY", color = Cyan, fontSize = 11.sp, fontWeight = FontWeight.Bold, letterSpacing = 1.sp)
                    Text("Capture evidence", color = Color.White, fontSize = 18.sp, fontWeight = FontWeight.Bold)
                }
            }
            // Step indicators
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                if (requiresPhoto)     StepDot("P", state.photoPath != null)
                if (requiresSignature) StepDot("S", state.signaturePath != null)
                if (requiresOtp)       StepDot("O", state.otpToken != null)
                if (isCod)             StepDot("₱", state.codCollected)
            }
        }

        // COD section
        if (isCod) {
            CodSection(
                amount = codAmount,
                collected = state.codCollected,
                onToggle = viewModel::onCodToggled
            )
            Spacer(Modifier.height(12.dp))
        }

        // Photo capture
        if (requiresPhoto) {
            PhotoSection(
                captured = state.photoPath != null,
                onCaptured = { path ->
                    viewModel.onPhotoCaptured(path)
                },
                taskId = taskId,
                context = context
            )
            Spacer(Modifier.height(12.dp))
        }

        // Signature
        if (requiresSignature) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 20.dp)
                    .clip(RoundedCornerShape(14.dp))
                    .background(Glass)
                    .border(
                        1.dp,
                        if (state.signaturePath != null) Green.copy(alpha = 0.3f) else Border,
                        RoundedCornerShape(14.dp)
                    )
                    .padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp)
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween
                ) {
                    Text("Signature", color = Color.White.copy(alpha = 0.6f), fontSize = 12.sp)
                    if (state.signaturePath != null) Text("Captured ✓", color = Green, fontSize = 11.sp)
                }
                SignatureCanvas(
                    onSigned = { bitmap ->
                        val path = saveBitmap(context, bitmap, "sig_$taskId.png")
                        viewModel.onSignatureSaved(path)
                    },
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(160.dp)
                        .clip(RoundedCornerShape(10.dp))
                )
            }
            Spacer(Modifier.height(12.dp))
        }

        // OTP
        if (requiresOtp) {
            OtpPodSection(
                otpToken = state.otpToken,
                recipientPhone = state.recipientPhone,
                otpSent = state.otpSent,
                isSendingOtp = state.isSendingOtp,
                isVerifyingOtp = state.isVerifyingOtp,
                otpError = state.otpError,
                dummyOtpCode = state.dummyOtpCode,
                onSendOtp = viewModel::sendOtpToRecipient,
                onConfirmOtp = viewModel::confirmOtp
            )
            Spacer(Modifier.height(12.dp))
        }

        Spacer(Modifier.height(12.dp))

        // Surface submit-time failures so the user sees the real error instead
        // of the old silent-enqueue-to-sync-queue behaviour.
        state.error?.let { err ->
            Surface(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 20.dp),
                color = Red.copy(alpha = 0.12f),
                border = androidx.compose.foundation.BorderStroke(1.dp, Red.copy(alpha = 0.4f)),
                shape = RoundedCornerShape(12.dp)
            ) {
                Text(
                    text = err,
                    color = Red,
                    fontSize = 12.sp,
                    modifier = Modifier.padding(12.dp)
                )
            }
            Spacer(Modifier.height(8.dp))
        }

        // Submit POD
        Button(
            onClick = { viewModel.submit(taskId) },
            enabled = state.canSubmit && !state.isSubmitting,
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 20.dp)
                .height(56.dp),
            shape = RoundedCornerShape(14.dp),
            colors = ButtonDefaults.buttonColors(
                containerColor = Cyan,
                disabledContainerColor = Color.White.copy(alpha = 0.08f)
            )
        ) {
            if (state.isSubmitting) {
                CircularProgressIndicator(color = Canvas, modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
            } else {
                Text(
                    "Submit POD",
                    color = if (state.canSubmit) Canvas else Color.White.copy(alpha = 0.3f),
                    fontWeight = FontWeight.Bold,
                    fontSize = 16.sp
                )
            }
        }

        // Hint listing what's still required when the submit button is disabled
        if (!state.canSubmit) {
            val missing = buildList {
                if (requiresPhoto && state.photoPath == null)         add("parcel photo")
                if (requiresSignature && state.signaturePath == null) add("signature")
                if (requiresOtp && state.otpToken == null)            add("OTP verification")
            }
            if (missing.isNotEmpty()) {
                Text(
                    "Still needed: ${missing.joinToString(", ")}",
                    color = Color.White.copy(alpha = 0.35f),
                    fontSize = 12.sp,
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 20.dp, vertical = 4.dp),
                    textAlign = androidx.compose.ui.text.style.TextAlign.Center
                )
            }
        }

        Spacer(Modifier.height(8.dp))

        // Failed delivery button
        TextButton(
            onClick = { viewModel.showFailureSheet() },
            modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp)
        ) {
            Text("Mark as Failed Delivery", color = Red.copy(alpha = 0.7f), fontSize = 14.sp)
        }

        Spacer(Modifier.navigationBarsPadding().height(16.dp))
    }
}

@Composable
private fun StepDot(label: String, done: Boolean) {
    Box(
        modifier = Modifier
            .size(28.dp)
            .clip(RoundedCornerShape(14.dp))
            .background(if (done) Green.copy(alpha = 0.15f) else Glass)
            .border(1.dp, if (done) Green.copy(alpha = 0.4f) else Border, RoundedCornerShape(14.dp)),
        contentAlignment = Alignment.Center
    ) {
        Text(label, color = if (done) Green else Color.White.copy(alpha = 0.4f), fontSize = 11.sp, fontWeight = FontWeight.Bold)
    }
}

@Composable
private fun CodSection(amount: Double, collected: Boolean, onToggle: (Boolean) -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 20.dp)
            .clip(RoundedCornerShape(14.dp))
            .background(if (collected) Amber.copy(alpha = 0.08f) else Glass)
            .border(
                1.dp,
                if (collected) Amber.copy(alpha = 0.3f) else Border,
                RoundedCornerShape(14.dp)
            )
            .padding(16.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
    ) {
        Column {
            Text("Cash on Delivery", color = Amber, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
            Text("₱${"%,.2f".format(amount)}", color = Color.White, fontSize = 22.sp, fontWeight = FontWeight.Bold)
        }
        Switch(
            checked = collected,
            onCheckedChange = onToggle,
            colors = SwitchDefaults.colors(
                checkedThumbColor = Canvas,
                checkedTrackColor = Amber,
                uncheckedThumbColor = Color.White.copy(alpha = 0.4f),
                uncheckedTrackColor = Color.White.copy(alpha = 0.08f)
            )
        )
    }
}

/**
 * Photo capture section backed by the system camera (ActivityResultContracts.TakePicture).
 *
 * Tapping anywhere on the preview/placeholder area — including the camera icon —
 * immediately fires the system camera app. No intermediate "Open Camera" button
 * and no embedded CameraX preview inside the scrollable form.
 *
 * Flow:
 *   1. Tap placeholder  →  request CAMERA permission if not yet granted
 *   2. Permission granted  →  create a FileProvider URI in context.filesDir
 *   3. System camera launches full-screen (familiar UX, no re-implementation)
 *   4. On confirm in camera app  →  TakePicture returns true  →  onCaptured(path)
 *
 * FileProvider authority must match the `<provider>` declared in AndroidManifest.xml.
 */
@Composable
private fun PhotoSection(
    captured: Boolean,
    onCaptured: (String) -> Unit,
    taskId: String,
    context: Context
) {
    // Stable file path — same name on retake so old file is overwritten cleanly.
    val photoFile = remember(taskId) { File(context.filesDir, "photo_$taskId.jpg") }

    var cameraPermissionGranted by remember {
        mutableStateOf(
            ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA)
                == PackageManager.PERMISSION_GRANTED
        )
    }

    val scope = rememberCoroutineScope()

    // TakePicture writes the photo to a FileProvider URI and returns success flag.
    val photoUri = remember(taskId) {
        FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", photoFile)
    }
    val takePictureLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.TakePicture()
    ) { success ->
        if (success) {
            scope.launch(Dispatchers.IO) {
                ImageCompressor.compressToFile(photoFile)
                withContext(Dispatchers.Main) { onCaptured(photoFile.absolutePath) }
            }
        }
    }
    val cameraPermissionLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.RequestPermission()
    ) { granted ->
        cameraPermissionGranted = granted
        if (granted) takePictureLauncher.launch(photoUri)
    }

    fun launchCamera() {
        if (cameraPermissionGranted) takePictureLauncher.launch(photoUri)
        else cameraPermissionLauncher.launch(Manifest.permission.CAMERA)
    }

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 20.dp)
            .clip(RoundedCornerShape(14.dp))
            .background(Glass)
            .border(
                1.dp,
                if (captured) Green.copy(alpha = 0.3f) else Border,
                RoundedCornerShape(14.dp)
            )
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp)
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween
        ) {
            Text("Parcel Photo", color = Color.White.copy(alpha = 0.6f), fontSize = 12.sp)
            if (captured) Text("Captured ✓", color = Green, fontSize = 11.sp)
        }

        // Single tappable area — tapping anywhere (icon, text, or empty space) launches
        // the system camera immediately with no intermediate button required.
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(160.dp)
                .clip(RoundedCornerShape(10.dp))
                .background(
                    if (captured) Green.copy(alpha = 0.05f) else Color(0x08FFFFFF)
                )
                .border(
                    1.dp,
                    if (captured) Green.copy(alpha = 0.25f) else Border,
                    RoundedCornerShape(10.dp)
                )
                .clickable { launchCamera() },
            contentAlignment = Alignment.Center
        ) {
            if (captured) {
                Column(
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(4.dp)
                ) {
                    Icon(
                        Icons.Default.CameraAlt,
                        contentDescription = null,
                        tint = Green.copy(alpha = 0.7f),
                        modifier = Modifier.size(36.dp)
                    )
                    Text("Photo captured ✓", color = Green, fontSize = 13.sp, fontWeight = FontWeight.Medium)
                    Text("Tap to retake", color = Color.White.copy(alpha = 0.3f), fontSize = 11.sp)
                }
            } else {
                Column(
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(6.dp)
                ) {
                    Icon(
                        Icons.Default.CameraAlt,
                        contentDescription = "Take photo",
                        tint = Cyan.copy(alpha = 0.6f),
                        modifier = Modifier.size(40.dp)
                    )
                    Text("Tap to take photo", color = Color.White.copy(alpha = 0.5f), fontSize = 13.sp)
                    Text(
                        "System camera will open",
                        color = Color.White.copy(alpha = 0.25f),
                        fontSize = 11.sp
                    )
                }
            }
        }
    }
}

/**
 * OTP capture section with a two-step flow:
 *
 *  1. "Send Code" button  →  POST /v1/otps/generate  → SMS dispatched to recipient
 *  2. Driver asks recipient for the code; types it here
 *     → 6 digits entered  →  POST /v1/otps/verify  → backend confirms or rejects
 *  3. On success: section turns green, otpToken is set, submit button unlocks.
 *
 * Inline OTP errors (send failure, wrong/expired code) are shown inside the
 * section rather than the global error surface so the rest of the form stays usable.
 */
@Composable
private fun OtpPodSection(
    otpToken: String?,
    recipientPhone: String,
    otpSent: Boolean,
    isSendingOtp: Boolean,
    isVerifyingOtp: Boolean,
    otpError: String?,
    /** Non-null in dummy/dev mode — backend returned the code directly (no SMS sent).
     *  Pre-fills the entry field and is shown as a visible badge for QA purposes. */
    dummyOtpCode: String?,
    onSendOtp: () -> Unit,
    onConfirmOtp: (String) -> Unit,
) {
    // Local input buffer — pre-filled with dummy code when available.
    var entered by remember { mutableStateOf(dummyOtpCode ?: "") }
    // Keep in sync if dummyOtpCode arrives after initial composition (race: state update).
    LaunchedEffect(dummyOtpCode) { if (dummyOtpCode != null) entered = dummyOtpCode }

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 20.dp)
            .clip(RoundedCornerShape(14.dp))
            .background(Glass)
            .border(
                1.dp,
                if (otpToken != null) Green.copy(alpha = 0.3f) else Border,
                RoundedCornerShape(14.dp)
            )
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        // Section header
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween
        ) {
            Text("OTP Verification", color = Color.White.copy(alpha = 0.6f), fontSize = 12.sp)
            if (otpToken != null) Text("Verified ✓", color = Green, fontSize = 11.sp)
        }

        when {
            // ── Step 3: already verified ──────────────────────────────────────
            otpToken != null -> {
                Text(
                    "Code verified — recipient confirmed delivery.",
                    color = Green.copy(alpha = 0.85f),
                    fontSize = 13.sp
                )
            }

            // ── Step 1: OTP not yet sent — show "Send Code" button ────────────
            !otpSent -> {
                val phoneHint = if (recipientPhone.isNotBlank()) " to $recipientPhone" else ""
                Text(
                    "Send a one-time code$phoneHint and ask the recipient to read it back.",
                    color = Color.White.copy(alpha = 0.5f),
                    fontSize = 13.sp
                )
                Button(
                    onClick = onSendOtp,
                    enabled = !isSendingOtp,
                    modifier = Modifier.fillMaxWidth().height(44.dp),
                    shape = RoundedCornerShape(10.dp),
                    colors = ButtonDefaults.buttonColors(
                        containerColor = Cyan.copy(alpha = 0.15f),
                        disabledContainerColor = Glass
                    )
                ) {
                    if (isSendingOtp) {
                        CircularProgressIndicator(
                            color = Cyan,
                            modifier = Modifier.size(18.dp),
                            strokeWidth = 2.dp
                        )
                    } else {
                        Text(
                            "Send Code to Recipient",
                            color = Cyan,
                            fontWeight = FontWeight.SemiBold,
                            fontSize = 14.sp
                        )
                    }
                }
            }

            // ── Step 2: OTP sent — show entry field ───────────────────────────
            else -> {
                // Dev/dummy mode badge — visible when backend returned code directly.
                if (dummyOtpCode != null) {
                    Text(
                        "Dev mode — code: $dummyOtpCode",
                        color = Cyan.copy(alpha = 0.75f),
                        fontSize = 12.sp,
                        fontWeight = FontWeight.SemiBold,
                    )
                }
                Text(
                    if (dummyOtpCode != null) "Verifying automatically…"
                    else "Enter the 6-digit code shown on the recipient's phone.",
                    color = Color.White.copy(alpha = 0.5f),
                    fontSize = 13.sp
                )
                OutlinedTextField(
                    value = entered,
                    onValueChange = { input ->
                        if (input.length <= 6) {
                            entered = input
                            // Auto-submit as soon as 6 digits are entered — fires verifyOtp.
                            if (input.length == 6 && !isVerifyingOtp) onConfirmOtp(input)
                        }
                    },
                    label = { Text("6-digit OTP") },
                    singleLine = true,
                    enabled = !isVerifyingOtp,
                    modifier = Modifier.fillMaxWidth(),
                    trailingIcon = {
                        if (isVerifyingOtp) {
                            CircularProgressIndicator(
                                color = Cyan,
                                modifier = Modifier.size(18.dp),
                                strokeWidth = 2.dp
                            )
                        }
                    },
                    colors = OutlinedTextFieldDefaults.colors(
                        focusedBorderColor = Cyan,
                        unfocusedBorderColor = Border,
                        focusedTextColor = Color.White,
                        unfocusedTextColor = Color.White,
                        focusedLabelColor = Cyan,
                        unfocusedLabelColor = Color.White.copy(alpha = 0.5f),
                        cursorColor = Cyan,
                        disabledTextColor = Color.White.copy(alpha = 0.4f),
                        disabledBorderColor = Border,
                        disabledLabelColor = Color.White.copy(alpha = 0.3f)
                    )
                )
                // Resend affordance — clears otpSent so the "Send Code" button re-appears.
                TextButton(
                    onClick = onSendOtp,
                    modifier = Modifier.align(Alignment.End)
                ) {
                    Text("Resend Code", color = Cyan.copy(alpha = 0.55f), fontSize = 12.sp)
                }
            }
        }

        // Inline OTP error (wrong code, network failure, expired OTP).
        // Shown inside the section so the global error surface stays clean.
        otpError?.let { err ->
            Text(err, color = Red, fontSize = 12.sp, lineHeight = 17.sp)
        }
    }
}

@Composable
private fun FailureReasonSheet(
    onSelect: (FailureReason) -> Unit,
    onDismiss: () -> Unit
) {
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = 0.6f)),
        contentAlignment = Alignment.BottomCenter
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(topStart = 20.dp, topEnd = 20.dp))
                .background(Color(0xFF0D1220))
                .border(1.dp, Border, RoundedCornerShape(topStart = 20.dp, topEnd = 20.dp))
                .padding(horizontal = 20.dp, vertical = 24.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            Text("Failure Reason", color = Red, fontSize = 16.sp, fontWeight = FontWeight.Bold)
            Text("Select the reason this delivery could not be completed", color = Color.White.copy(alpha = 0.5f), fontSize = 13.sp)
            Spacer(Modifier.height(4.dp))

            FailureReason.entries.forEach { reason ->
                Button(
                    onClick = { onSelect(reason) },
                    modifier = Modifier.fillMaxWidth().height(48.dp),
                    shape = RoundedCornerShape(10.dp),
                    colors = ButtonDefaults.buttonColors(containerColor = Glass)
                ) {
                    Text(reason.displayName, color = Color.White, fontSize = 14.sp)
                }
            }

            Spacer(Modifier.height(8.dp))
            TextButton(onClick = onDismiss, modifier = Modifier.fillMaxWidth()) {
                Text("Cancel", color = Color.White.copy(alpha = 0.4f))
            }
            Spacer(Modifier.navigationBarsPadding())
        }
    }
}

private fun saveBitmap(context: Context, bitmap: Bitmap, filename: String): String {
    val file = File(context.filesDir, filename)
    FileOutputStream(file).use { out -> bitmap.compress(Bitmap.CompressFormat.PNG, 90, out) }
    return file.absolutePath
}
