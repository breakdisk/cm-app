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
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Backspace
import androidx.compose.material.icons.filled.CameraAlt
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import androidx.core.content.FileProvider
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.core.common.ImageCompressor
import io.logisticos.driver.core.designsystem.*
import io.logisticos.driver.feature.pod.presentation.FailureReason
import io.logisticos.driver.feature.pod.presentation.PodViewModel
import java.io.File
import java.io.FileOutputStream
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** pod issues 6-digit delivery PINs. */
private const val PIN_LENGTH = 6

/**
 * Proof of delivery (driver design, "pin"): cash, the recipient's PIN on a
 * glove-sized keypad, the doorstep photo and a signature — whichever this stop
 * requires — then submit. Behaviour is [PodViewModel]'s; only the presentation
 * is the design's.
 */
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
    val c = LocalMoveColors.current

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
        val collected = state.partialCodAmountInput.toDoubleOrNull()?.takeIf { it > 0 } ?: codAmount
        SubmittedPane(
            codLine = if (isCod && state.codCollected) "COD ₱${"%,.2f".format(collected)} collected" else null,
            onContinue = onCompleted,
        )
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
            .background(c.ground)
            .verticalScroll(rememberScrollState())
    ) {
        MoveScreenHeader(label = "Proof of delivery", title = "Hand it over", onBack = onBack)

        // What this handover needs, ticked off as each is done.
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .horizontalScroll(rememberScrollState())
                .padding(horizontal = 16.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            if (isCod)             StepPill("Cash", state.codCollected)
            if (requiresOtp)       StepPill("PIN", state.otpToken != null)
            if (requiresPhoto)     StepPill("Photo", state.photoPath != null)
            if (requiresSignature) StepPill("Signature", state.signaturePath != null)
        }

        Column(
            modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 14.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            if (isCod) {
                CodSection(
                    amount = codAmount,
                    collected = state.codCollected,
                    onToggle = viewModel::onCodToggled,
                    partialAmountInput = state.partialCodAmountInput,
                    onPartialAmountChanged = viewModel::onPartialCodAmountChanged,
                )
            }

            // Delivery PIN — pod refuses a POD without it
            if (requiresOtp) {
                PinSection(
                    otpToken = state.otpToken,
                    otpSent = state.otpSent,
                    pinAlreadyIssued = state.pinAlreadyIssued,
                    isSendingOtp = state.isSendingOtp,
                    isVerifyingOtp = state.isVerifyingOtp,
                    otpError = state.otpError,
                    onSendPin = { viewModel.sendOtpToRecipient() },
                    onSendNewPin = { viewModel.sendOtpToRecipient(reissue = true) },
                    onConfirmOtp = viewModel::confirmOtp
                )
            }

            if (requiresPhoto) {
                PhotoSection(
                    captured = state.photoPath != null,
                    onCaptured = { path -> viewModel.onPhotoCaptured(path) },
                    taskId = taskId,
                    context = context
                )
            }

            if (requiresSignature) {
                MovePanel(tone = if (state.signaturePath != null) MoveTone.Accent else MoveTone.Neutral) {
                    Row(
                        modifier = Modifier.fillMaxWidth().padding(bottom = 10.dp),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        MoveLabel("Signature")
                        if (state.signaturePath != null) Text("Captured ✓", color = c.success, fontSize = 13.sp, fontWeight = FontWeight.Bold)
                    }
                    SignatureCanvas(
                        onSigned = { bitmap ->
                            val path = saveBitmap(context, bitmap, "sig_$taskId.png")
                            viewModel.onSignatureSaved(path)
                        },
                        modifier = Modifier.fillMaxWidth()
                    )
                }
            }

            // Surface submit-time failures so the user sees the real error instead
            // of the old silent-enqueue-to-sync-queue behaviour.
            state.error?.let { err ->
                MoveNotice(title = "Proof not submitted", body = err, tone = MoveTone.Penalty)
            }

            MoveBigButton(
                label = "SUBMIT PROOF",
                onClick = { viewModel.submit(taskId) },
                enabled = state.canSubmit,
                loading = state.isSubmitting,
            )

            // Hint listing what's still required when the submit button is disabled
            if (!state.canSubmit) {
                val missing = buildList {
                    if (requiresPhoto && state.photoPath == null)         add("parcel photo")
                    if (requiresSignature && state.signaturePath == null) add("signature")
                    if (requiresOtp && state.otpToken == null)            add("delivery PIN")
                }
                if (missing.isNotEmpty()) {
                    Text(
                        "Still needed: ${missing.joinToString(", ")}",
                        color = c.muted,
                        fontSize = 14.sp,
                        textAlign = TextAlign.Center,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            }

            MoveBigButton(
                label = "COULDN'T DELIVER",
                onClick = { viewModel.showFailureSheet() },
                filled = false,
                tone = MoveTone.Penalty,
                height = 56.dp,
            )
        }

        Spacer(Modifier.navigationBarsPadding().height(24.dp))
    }
}

@Composable
private fun StepPill(label: String, done: Boolean) {
    val c = LocalMoveColors.current
    MovePill(text = if (done) "✓ $label" else label, fg = if (done) c.success else c.muted, bg = c.chip)
}

@Composable
private fun SubmittedPane(codLine: String?, onContinue: () -> Unit) {
    val c = LocalMoveColors.current
    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(c.ground)
            .padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Box(
            modifier = Modifier
                .size(112.dp)
                .clip(CircleShape)
                .background(c.chip)
                .border(2.dp, c.success, CircleShape),
            contentAlignment = Alignment.Center
        ) {
            Icon(Icons.Filled.Check, contentDescription = null, tint = c.success, modifier = Modifier.size(56.dp))
        }
        Text("Proof submitted", color = c.ink, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 40.sp, modifier = Modifier.padding(top = 24.dp))
        Text("This stop is done.", color = c.muted, fontSize = 16.sp, textAlign = TextAlign.Center, modifier = Modifier.padding(top = 6.dp))
        codLine?.let {
            Text(it, color = c.amber, fontSize = 16.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(top = 10.dp))
        }
        Spacer(Modifier.height(32.dp))
        MoveBigButton(label = "CONTINUE", onClick = onContinue)
    }
}

@Composable
private fun CodSection(
    amount: Double,
    collected: Boolean,
    onToggle: (Boolean) -> Unit,
    partialAmountInput: String,
    onPartialAmountChanged: (String) -> Unit,
) {
    val c = LocalMoveColors.current
    MovePanel(tone = if (collected) MoveTone.Amber else MoveTone.Neutral) {
        // The whole row toggles — a switch alone is too small for gloves.
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = 56.dp)
                .toggleable(value = collected, onValueChange = onToggle, role = Role.Switch),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Column(Modifier.weight(1f)) {
                Text("CASH ON DELIVERY", color = c.amber, fontSize = 12.sp, letterSpacing = 2.4.sp)
                Text("₱${"%,.2f".format(amount)}", color = c.ink, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 34.sp)
                Text(if (collected) "Collected" else "Switch on once the cash is in hand", color = c.muted, fontSize = 13.sp)
            }
            Switch(
                checked = collected,
                onCheckedChange = null,
                colors = SwitchDefaults.colors(
                    checkedThumbColor = c.amberInk,
                    checkedTrackColor = c.amber,
                    checkedBorderColor = c.amber,
                    uncheckedThumbColor = c.muted,
                    uncheckedTrackColor = c.chip,
                    uncheckedBorderColor = c.hairline,
                )
            )
        }
        // Partial amount input — shown when the toggle is ON so the driver can
        // record a partial collection (customer only had partial cash).
        // Blank = full amount collected; any positive value overrides the total.
        if (collected) {
            Spacer(Modifier.height(12.dp))
            MoveTextField(
                value = partialAmountInput,
                onValueChange = onPartialAmountChanged,
                label = "Amount collected (₱)",
                placeholder = "%,.2f".format(amount),
                keyboardType = KeyboardType.Decimal,
            )
            if (partialAmountInput.isNotEmpty()) {
                val partial = partialAmountInput.toDoubleOrNull()
                if (partial != null && partial < amount) {
                    Text(
                        "Partial collection — shortfall ₱${"%,.2f".format(amount - partial)}",
                        color = c.amber,
                        fontSize = 13.sp,
                        modifier = Modifier.padding(top = 8.dp),
                    )
                }
            }
        }
    }
}

/**
 * Photo capture section backed by the system camera (ActivityResultContracts.TakePicture).
 *
 * Tapping the viewfinder or the capture button fires the system camera app.
 * No embedded CameraX preview inside the scrollable form.
 *
 * Flow:
 *   1. Tap  →  request CAMERA permission if not yet granted
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
    val c = LocalMoveColors.current
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

    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            MoveLabel("Parcel photo")
            if (captured) Text("Captured ✓", color = c.success, fontSize = 13.sp, fontWeight = FontWeight.Bold)
        }
        MoveViewfinder(
            Modifier
                .height(220.dp)
                .clickable(role = Role.Button, onClickLabel = "Take photo") { launchCamera() }
        ) {
            Column(
                modifier = Modifier.fillMaxSize().padding(28.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center,
            ) {
                Icon(
                    Icons.Filled.CameraAlt,
                    contentDescription = null,
                    tint = if (captured) c.success else c.accent,
                    modifier = Modifier.size(44.dp)
                )
                Text(
                    if (captured) "Photo taken" else "Frame the parcel at the door",
                    color = c.ink,
                    fontSize = 17.sp,
                    fontWeight = FontWeight.SemiBold,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.padding(top = 10.dp),
                )
                Text(
                    if (captured) "Tap to retake" else "The system camera opens",
                    color = c.muted,
                    fontSize = 13.sp,
                )
            }
        }
        MoveBigButton(
            label = if (captured) "RETAKE PHOTO" else "TAKE THE PHOTO",
            onClick = { launchCamera() },
            filled = !captured,
            icon = Icons.Filled.CameraAlt,
        )
    }
}

/**
 * Delivery PIN section.
 *
 * The recipient normally has the PIN already — the customer app shows it from
 * booking — so the keypad is shown straight away and the sixth digit verifies
 * automatically (POST /v1/otps/verify). "Text one" asks pod to send a PIN; pod
 * keeps a live PIN rather than replacing it, and says so. "Send a new PIN"
 * replaces it, which is why it is its own button.
 *
 * pod's reasons (wrong with attempts left, locked, none issued) are shown
 * inline so the rest of the form stays usable.
 */
@Composable
private fun PinSection(
    otpToken: String?,
    otpSent: Boolean,
    pinAlreadyIssued: Boolean,
    isSendingOtp: Boolean,
    isVerifyingOtp: Boolean,
    otpError: String?,
    onSendPin: () -> Unit,
    onSendNewPin: () -> Unit,
    onConfirmOtp: (String) -> Unit,
) {
    val c = LocalMoveColors.current
    var entered by remember { mutableStateOf("") }

    // A PIN pod turned down is cleared once the check finishes, so the next
    // attempt starts from empty slots rather than six stale digits.
    LaunchedEffect(isVerifyingOtp) {
        if (!isVerifyingOtp && otpToken == null && entered.length == PIN_LENGTH) entered = ""
    }

    MovePanel(tone = if (otpToken != null) MoveTone.Accent else MoveTone.Neutral) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            MoveLabel("Delivery PIN")
            if (otpToken != null) MovePill("Verified", fg = c.success, bg = c.chip)
        }

        if (otpToken != null) {
            Text("PIN verified", color = c.ink, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 30.sp, modifier = Modifier.padding(top = 8.dp))
            Text("The recipient confirmed the handover.", color = c.muted, fontSize = 15.sp)
        } else {
            Text("Ask for their PIN", color = c.ink, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 30.sp, modifier = Modifier.padding(top = 8.dp))
            Text(
                when {
                    pinAlreadyIssued ->
                        "The recipient already has a PIN — from their booking or an earlier text. Ask them for it."
                    otpSent ->
                        "A new PIN was texted to the recipient. Earlier PINs no longer work."
                    else ->
                        "Ask the recipient for the 6-digit delivery PIN from their booking or text message."
                },
                color = c.muted,
                fontSize = 15.sp,
                lineHeight = 21.sp,
                modifier = Modifier.padding(top = 4.dp),
            )

            // Slots
            Row(
                modifier = Modifier.fillMaxWidth().padding(top = 18.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                repeat(PIN_LENGTH) { i ->
                    val digit = entered.getOrNull(i)
                    val isCursor = i == entered.length && !isVerifyingOtp
                    val shape = RoundedCornerShape(16.dp)
                    Box(
                        modifier = Modifier
                            .weight(1f)
                            .height(72.dp)
                            .clip(shape)
                            .background(if (digit != null) c.accentPanel else c.chip)
                            .border(
                                2.dp,
                                when {
                                    digit != null -> c.accentBorder
                                    isCursor -> c.accent
                                    else -> c.hairline
                                },
                                shape
                            ),
                        contentAlignment = Alignment.Center,
                    ) {
                        Text(digit?.toString() ?: "", color = c.ink, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 34.sp)
                    }
                }
            }

            // Keypad — 64 dp keys, no soft keyboard to fight with gloves on.
            Column(
                modifier = Modifier.fillMaxWidth().padding(top = 16.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                listOf("1", "2", "3", "4", "5", "6", "7", "8", "9", KEY_CLEAR, "0", KEY_DELETE).chunked(3).forEach { row ->
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        row.forEach { key ->
                            PinKey(key = key, enabled = !isVerifyingOtp, modifier = Modifier.weight(1f)) {
                                when (key) {
                                    KEY_CLEAR -> entered = ""
                                    KEY_DELETE -> entered = entered.dropLast(1)
                                    else -> if (entered.length < PIN_LENGTH) {
                                        entered += key
                                        // Verifies as soon as the sixth digit is in.
                                        if (entered.length == PIN_LENGTH) onConfirmOtp(entered)
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if (isVerifyingOtp) {
                Row(
                    modifier = Modifier.padding(top = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    CircularProgressIndicator(color = c.accent, modifier = Modifier.size(18.dp), strokeWidth = 2.dp)
                    Text("Checking the PIN…", color = c.muted, fontSize = 14.sp)
                }
            }

            // Inline PIN error (wrong code, network failure, expired or locked).
            // Shown inside the section so the global error surface stays clean.
            otpError?.let { err ->
                Text(err, color = c.penalty, fontSize = 14.sp, lineHeight = 19.sp, modifier = Modifier.padding(top = 12.dp))
            }

            Spacer(Modifier.height(14.dp))
            // "Text one" keeps a live PIN; once pod has answered, the only send
            // left is a replacement.
            MoveBigButton(
                label = if (otpSent) "SEND A NEW PIN" else "NO PIN? TEXT THEM ONE",
                onClick = if (otpSent) onSendNewPin else onSendPin,
                filled = false,
                loading = isSendingOtp,
                height = 56.dp,
            )
        }
    }
}

private const val KEY_CLEAR = "CLEAR"
private const val KEY_DELETE = "DELETE"

@Composable
private fun PinKey(key: String, enabled: Boolean, modifier: Modifier, onPress: () -> Unit) {
    val c = LocalMoveColors.current
    val shape = RoundedCornerShape(18.dp)
    val fg = if (enabled) c.ink else c.muted
    Box(
        modifier = modifier
            .height(64.dp)
            .clip(shape)
            .background(c.chip)
            .border(1.dp, c.hairline, shape)
            .clickable(enabled = enabled, role = Role.Button, onClick = onPress),
        contentAlignment = Alignment.Center,
    ) {
        when (key) {
            KEY_DELETE -> Icon(Icons.AutoMirrored.Filled.Backspace, contentDescription = "Delete digit", tint = fg, modifier = Modifier.size(26.dp))
            KEY_CLEAR -> Text("CLEAR", color = c.muted, fontSize = 13.sp, fontWeight = FontWeight.Bold, letterSpacing = 1.2.sp)
            else -> Text(key, color = fg, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 30.sp)
        }
    }
}

@Composable
private fun FailureReasonSheet(
    onSelect: (FailureReason) -> Unit,
    onDismiss: () -> Unit
) {
    val c = LocalMoveColors.current
    val shape = RoundedCornerShape(topStart = 24.dp, topEnd = 24.dp)
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(c.scrim),
        contentAlignment = Alignment.BottomCenter
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .clip(shape)
                .background(c.surface)
                .border(1.dp, c.hairline, shape)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 20.dp, vertical = 24.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp)
        ) {
            Text("COULDN'T DELIVER", color = c.penalty, fontSize = 12.sp, fontWeight = FontWeight.Bold, letterSpacing = 2.4.sp)
            Text("What happened?", color = c.ink, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 30.sp)
            Text("Pick the reason this delivery could not be completed.", color = c.muted, fontSize = 15.sp)
            Spacer(Modifier.height(4.dp))

            FailureReason.entries.forEach { reason ->
                MoveBigButton(label = reason.displayName, onClick = { onSelect(reason) }, filled = false, tone = MoveTone.Neutral, height = 60.dp)
            }

            Spacer(Modifier.height(6.dp))
            MoveBigButton(label = "CANCEL", onClick = onDismiss, filled = false, tone = MoveTone.Neutral, height = 56.dp)
            Spacer(Modifier.navigationBarsPadding())
        }
    }
}

private fun saveBitmap(context: Context, bitmap: Bitmap, filename: String): String {
    val file = File(context.filesDir, filename)
    FileOutputStream(file).use { out -> bitmap.compress(Bitmap.CompressFormat.PNG, 90, out) }
    return file.absolutePath
}
