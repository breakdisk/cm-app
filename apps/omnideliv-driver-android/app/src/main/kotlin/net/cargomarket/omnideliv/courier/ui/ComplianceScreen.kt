package net.cargomarket.omnideliv.courier.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import net.cargomarket.omnideliv.courier.domain.ChecklistItem
import net.cargomarket.omnideliv.courier.domain.DocState
import net.cargomarket.omnideliv.courier.domain.SubmitValidation
import net.cargomarket.omnideliv.courier.domain.canSubmit
import net.cargomarket.omnideliv.courier.domain.summaryLine
import net.cargomarket.omnideliv.courier.domain.validateSubmission
import java.io.File
import java.time.LocalDate

/**
 * The courier's documents.
 *
 * Until this screen existed there was no way for a courier to submit anything:
 * the compliance service's `/me` routes worked with their token, the admin
 * console could only review, and no client called either. So a profile opened
 * at `pending_submission` and stayed there permanently — which is why
 * compliance gating could not be switched on.
 *
 * **Submission is folded in rather than being its own destination.** The app
 * already made that choice for the manifest's stop detail: a navigation between
 * a courier and the button they came to press is a navigation they do not want.
 * The phases below are local state, so a back gesture returns to the checklist
 * instead of leaving the feature.
 */
private sealed interface Phase {
    data object Checklist : Phase
    data class Form(val item: ChecklistItem) : Phase
    data class Capture(
        val item: ChecklistItem,
        val documentNumber: String,
        val expiryDate: String?,
    ) : Phase
}

@Composable
fun ComplianceScreen(
    onBack: () -> Unit = {},
    vm: ComplianceViewModel = hiltViewModel(),
) {
    val state by vm.state.collectAsState()
    var phase by remember { mutableStateOf<Phase>(Phase.Checklist) }

    LaunchedEffect(Unit) { vm.load() }

    when (val p = phase) {
        is Phase.Checklist -> ChecklistView(
            state = state,
            onBack = onBack,
            onPick = { item ->
                vm.clearSubmitFeedback()
                phase = Phase.Form(item)
            },
        )

        is Phase.Form -> DocumentForm(
            item = p.item,
            submitting = state.submitting,
            error = state.submitError,
            // A retry re-sends the photo already on disk rather than sending
            // the courier back to the camera for a second picture of the same
            // licence.
            retryPhoto = state.retryPhoto,
            onRetry = { number, expiry, photo ->
                vm.submit(
                    typeCode = p.item.code,
                    documentName = p.item.name,
                    documentNumber = number,
                    expiryDate = expiry,
                    photo = photo,
                    onDone = { phase = Phase.Checklist },
                )
            },
            onContinue = { number, expiry -> phase = Phase.Capture(p.item, number, expiry) },
            onCancel = {
                vm.clearSubmitFeedback()
                phase = Phase.Checklist
            },
        )

        is Phase.Capture -> ProofScreen(
            label = "Photograph your ${p.item.name}",
            rationaleTitle = "Camera access is needed for your documents",
            rationaleBody = "A clear photo of the document is what a reviewer approves. " +
                "Nothing is uploaded until you press send.",
            // No way past the camera here. A delivery proof may be skipped —
            // evidence missing beats a delivery that cannot be completed — but
            // a document submission with no document is not a thing to record.
            skipLabel = "Go back",
            onSkip = { phase = Phase.Form(p.item) },
            onCaptured = { file ->
                vm.submit(
                    typeCode = p.item.code,
                    documentName = p.item.name,
                    documentNumber = p.documentNumber,
                    expiryDate = p.expiryDate,
                    photo = file,
                    onDone = { phase = Phase.Checklist },
                )
                // Back to the form immediately: it is where the spinner, any
                // error and the retry button live. Sitting on a camera preview
                // while an upload runs shows the courier nothing.
                phase = Phase.Form(p.item)
            },
        )
    }
}

// ── Checklist ────────────────────────────────────────────────────────────────

@Composable
private fun ChecklistView(
    state: ComplianceUiState,
    onBack: () -> Unit,
    onPick: (ChecklistItem) -> Unit,
) {
    Column(
        Modifier
            .fillMaxSize()
            .background(Tokens.Base)
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
    ) {
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Documents", color = Tokens.Text, fontWeight = FontWeight.Bold, fontSize = 22.sp)
            TextButton(onClick = onBack) {
                Text("Done", color = Tokens.Cyan, fontSize = 13.sp)
            }
        }

        Text(
            summaryLine(state.state, state.outstanding),
            color = if (state.outstanding > 0) Tokens.Amber else Tokens.TextMuted,
            fontSize = 13.sp,
            lineHeight = 18.sp,
        )

        state.justSubmitted?.let {
            Spacer(Modifier.height(8.dp))
            Text("$it sent for review.", color = Tokens.Signal, fontSize = 13.sp)
        }

        if (state.failed) {
            Spacer(Modifier.height(8.dp))
            Text(
                "Could not load your documents. Pull down or try again shortly.",
                color = Tokens.Amber,
                fontSize = 12.sp,
            )
        }

        Spacer(Modifier.height(18.dp))

        if (state.loading && state.items.isEmpty()) {
            Text("Loading…", color = Tokens.TextMuted, fontSize = 14.sp)
            return@Column
        }

        if (state.items.isEmpty()) {
            Text(
                "No documents are required in your area.",
                color = Tokens.TextMuted,
                fontSize = 14.sp,
            )
            return@Column
        }

        state.items.forEach { item ->
            DocumentRow(item, onPick)
            Spacer(Modifier.height(10.dp))
        }

        Spacer(Modifier.height(8.dp))
        Text(
            // Says plainly what the platform is doing, because a courier with
            // four outstanding documents who is still getting jobs would
            // otherwise conclude this screen is broken.
            "Keeping these current is what stops your account being held up later.",
            color = Tokens.TextMuted,
            fontSize = 11.sp,
            lineHeight = 16.sp,
        )
    }
}

@Composable
private fun DocumentRow(item: ChecklistItem, onPick: (ChecklistItem) -> Unit) {
    val actionable = canSubmit(item.state)
    val (label, color) = stateLabel(item.state)

    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(Tokens.Surface)
            .border(1.dp, if (actionable) color else Tokens.Border, RoundedCornerShape(12.dp))
            .let { if (actionable) it.clickable { onPick(item) } else it }
            .padding(14.dp),
    ) {
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                item.name,
                color = Tokens.Text,
                fontSize = 15.sp,
                fontWeight = FontWeight.Bold,
                modifier = Modifier.padding(end = 10.dp),
            )
            // Text as well as colour, always. Direct sunlight and WCAG 2.1 AA
            // are both reasons; either alone would be enough.
            Badge(label, color)
        }

        (item.state as? DocState.Rejected)?.reason?.let {
            Spacer(Modifier.height(6.dp))
            Text("Reason: $it", color = Tokens.Amber, fontSize = 12.sp, lineHeight = 17.sp)
        }

        Spacer(Modifier.height(6.dp))
        Text(explain(item.state), color = Tokens.TextMuted, fontSize = 12.sp, lineHeight = 17.sp)

        if (actionable) {
            Spacer(Modifier.height(8.dp))
            Text(
                if (item.state is DocState.Missing) "Tap to submit" else "Tap to replace",
                color = Tokens.Cyan,
                fontSize = 12.sp,
                fontWeight = FontWeight.Bold,
            )
        }
    }
}

private fun stateLabel(state: DocState): Pair<String, Color> = when (state) {
    is DocState.Missing -> "not submitted" to Tokens.Amber
    is DocState.UnderReview -> "in review" to Tokens.Cyan
    is DocState.Approved -> "approved" to Tokens.Signal
    is DocState.ExpiringSoon -> "expiring" to Tokens.Amber
    is DocState.Expired -> "expired" to Tokens.Amber
    is DocState.Rejected -> "rejected" to Tokens.Amber
}

private fun explain(state: DocState): String = when (state) {
    is DocState.Missing -> "We do not have this one yet."
    is DocState.UnderReview -> "Someone is looking at it. Nothing for you to do."
    is DocState.Approved ->
        if (state.daysLeft == null) "Approved." else "Approved — valid for ${state.daysLeft} more days."
    is DocState.ExpiringSoon -> when (state.daysLeft) {
        0L -> "Expires today. Send a new one."
        1L -> "Expires tomorrow. Send a new one."
        else -> "Expires in ${state.daysLeft} days. Send a new one."
    }
    is DocState.Expired -> "This has run out. Send a current one."
    is DocState.Rejected -> "A reviewer could not accept it. Send another."
}

// ── Form ─────────────────────────────────────────────────────────────────────

@Composable
private fun DocumentForm(
    item: ChecklistItem,
    submitting: Boolean,
    error: String?,
    retryPhoto: File?,
    onRetry: (String, String?, File) -> Unit,
    onContinue: (String, String?) -> Unit,
    onCancel: () -> Unit,
) {
    // Prefilled from the last submission of this type, so a courier resubmitting
    // a rejected licence does not retype a number that was never the problem.
    var number by remember(item.typeId) { mutableStateOf(item.lastDocumentNumber.orEmpty()) }
    var expiry by remember(item.typeId) { mutableStateOf("") }
    var localError by remember(item.typeId) { mutableStateOf<String?>(null) }

    Column(
        Modifier
            .fillMaxSize()
            .background(Tokens.Base)
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
    ) {
        Text(item.name, color = Tokens.Text, fontWeight = FontWeight.Bold, fontSize = 20.sp)
        Spacer(Modifier.height(4.dp))
        Text(
            "Type what is printed on the document, then take a photo of it.",
            color = Tokens.TextMuted,
            fontSize = 13.sp,
            lineHeight = 18.sp,
        )

        Spacer(Modifier.height(20.dp))

        Field(
            value = number,
            onChange = { number = it; localError = null },
            label = "Document number",
            keyboard = KeyboardType.Text,
        )

        if (item.hasExpiry) {
            Spacer(Modifier.height(12.dp))
            Field(
                value = expiry,
                onChange = { expiry = it; localError = null },
                label = "Expiry date (YYYY-MM-DD)",
                keyboard = KeyboardType.Number,
            )
        }

        val message = localError ?: error
        if (message != null) {
            Spacer(Modifier.height(12.dp))
            Text(message, color = Tokens.Amber, fontSize = 13.sp, lineHeight = 18.sp)
        }

        Spacer(Modifier.height(22.dp))

        if (submitting) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                CircularProgressIndicator(
                    color = Tokens.Cyan,
                    strokeWidth = 2.dp,
                    modifier = Modifier.height(18.dp),
                )
                Spacer(Modifier.height(0.dp))
                Text("  Sending…", color = Tokens.TextMuted, fontSize = 13.sp)
            }
            return@Column
        }

        // Only when the failure was one that sending the same bytes again could
        // actually fix — see `retryable`.
        if (retryPhoto != null) {
            Primary("Send that photo again") {
                onRetry(number, expiry.takeIf { item.hasExpiry }, retryPhoto)
            }
            Spacer(Modifier.height(10.dp))
        }

        Primary(if (retryPhoto != null) "Take a new photo" else "Take photo") {
            when (
                val v = validateSubmission(
                    documentNumber = number,
                    expiryDate = expiry,
                    requiresExpiry = item.hasExpiry,
                    today = LocalDate.now(),
                )
            ) {
                is SubmitValidation.Invalid -> localError = v.message
                is SubmitValidation.Valid ->
                    onContinue(number.trim(), expiry.takeIf { item.hasExpiry })
            }
        }

        Spacer(Modifier.height(10.dp))
        TextButton(onClick = onCancel, modifier = Modifier.fillMaxWidth()) {
            Text("Cancel", color = Tokens.TextMuted, fontSize = 13.sp)
        }
    }
}

@Composable
private fun Primary(text: String, onClick: () -> Unit) {
    Button(
        onClick = onClick,
        colors = ButtonDefaults.buttonColors(
            containerColor = Tokens.Signal,
            contentColor = Tokens.SignalInk,
        ),
        modifier = Modifier.fillMaxWidth().heightIn(min = Tokens.MinTarget),
    ) {
        Text(text, fontSize = 16.sp, fontWeight = FontWeight.Bold)
    }
}

@Composable
private fun Field(
    value: String,
    onChange: (String) -> Unit,
    label: String,
    keyboard: KeyboardType,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(label, color = Tokens.TextMuted, fontSize = 12.sp) },
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = keyboard),
        colors = OutlinedTextFieldDefaults.colors(
            focusedTextColor = Tokens.Text,
            unfocusedTextColor = Tokens.Text,
            focusedBorderColor = Tokens.Cyan,
            unfocusedBorderColor = Tokens.Border,
            cursorColor = Tokens.Cyan,
        ),
        modifier = Modifier.fillMaxWidth(),
    )
}
