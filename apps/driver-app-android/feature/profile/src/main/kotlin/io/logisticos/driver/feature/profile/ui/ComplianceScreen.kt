package io.logisticos.driver.feature.profile.ui

import android.content.Context
import android.net.Uri
import android.util.Base64
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.AttachFile
import androidx.compose.material.icons.filled.CloudUpload
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
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.core.designsystem.*
import io.logisticos.driver.core.network.service.DocumentTypeDto
import io.logisticos.driver.core.network.service.DriverDocumentDto
import io.logisticos.driver.feature.profile.presentation.ComplianceViewModel

/**
 * Credentials (driver design, "compliance"): overall verification status and
 * each required document with its state, uploaded or replaced from here.
 *
 * @param onBack null where this is a bottom tab (no back affordance).
 */
@Composable
fun ComplianceScreen(
    onBack: (() -> Unit)?,
    viewModel: ComplianceViewModel = hiltViewModel(),
) {
    val state by viewModel.uiState.collectAsState()
    val c = LocalMoveColors.current

    LaunchedEffect(Unit) { viewModel.load() }

    var pendingType by remember { mutableStateOf<DocumentTypeDto?>(null) }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(c.ground)
            .verticalScroll(rememberScrollState())
    ) {
        MoveScreenHeader(label = "Compliance · verification", title = "Your credentials", onBack = onBack)

        Column(
            modifier = Modifier.padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            when {
                state.loading && state.profile == null -> {
                    Box(
                        Modifier.fillMaxWidth().padding(top = 64.dp),
                        contentAlignment = Alignment.Center,
                    ) { CircularProgressIndicator(color = c.accent) }
                }
                state.error != null && state.profile == null -> {
                    MoveNotice(title = "Couldn't load your credentials", body = state.error!!, tone = MoveTone.Penalty)
                    MoveBigButton("TRY AGAIN", onClick = { viewModel.load() }, filled = false, height = 56.dp)
                }
                state.profile != null -> {
                    OverallStatusCard(status = state.profile!!.overallStatus)

                    state.error?.let {
                        MoveNotice(title = "That didn't go through", body = it, tone = MoveTone.Penalty)
                        MoveBigButton("DISMISS", onClick = viewModel::clearError, filled = false, height = 56.dp)
                    }

                    if (state.requiredTypes.isEmpty()) {
                        MoveNotice(
                            title = "Nothing to upload",
                            body = "No documents are required for this jurisdiction.",
                            tone = MoveTone.Neutral,
                        )
                    } else {
                        val byType = state.latestByTypeId
                        val needAttention = state.requiredTypes.count { type ->
                            type.isRequired && byType[type.id]?.status !in setOf("approved", "submitted")
                        }
                        Row(
                            modifier = Modifier.fillMaxWidth().padding(top = 10.dp),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            MoveLabel("Credentials")
                            if (needAttention > 0) {
                                Text("$needAttention need attention", color = c.amber, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
                            }
                        }
                        val shape = RoundedCornerShape(20.dp)
                        Column(
                            Modifier
                                .fillMaxWidth()
                                .clip(shape)
                                .background(c.panel)
                                .border(1.dp, c.hairline, shape)
                        ) {
                            state.requiredTypes.forEachIndexed { i, type ->
                                DocumentRow(
                                    type = type,
                                    current = byType[type.id],
                                    isUploading = state.uploadingTypeCode == type.code,
                                    onUpload = { pendingType = type },
                                )
                                if (i < state.requiredTypes.lastIndex) MoveDivider()
                            }
                        }
                        Text(
                            "Photograph or pick a clear JPG, PNG or PDF. Ops reviews each one before it counts.",
                            color = c.muted,
                            fontSize = 13.sp,
                        )
                    }
                }
            }
        }

        Spacer(Modifier.navigationBarsPadding().height(24.dp))
    }

    pendingType?.let { type ->
        UploadDialog(
            type = type,
            onDismiss = { pendingType = null },
            onSubmit = { number, base64, contentType, issueDate, expiryDate ->
                viewModel.uploadDocument(
                    typeCode = type.code,
                    documentNumber = number,
                    fileBase64 = base64,
                    contentType = contentType,
                    issueDate = issueDate,
                    expiryDate = expiryDate,
                )
                pendingType = null
            },
        )
    }
}

@Composable
private fun OverallStatusCard(status: String) {
    val c = LocalMoveColors.current
    val (label, kind) = statusKind(status)
    MovePanel(tone = kind.panelTone) {
        Text("OVERALL STATUS", color = c.muted, fontSize = 12.sp, letterSpacing = 2.4.sp)
        Text(label, color = kind.color(c), fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 30.sp)
        statusHelp(status).takeIf { it.isNotBlank() }?.let {
            Text(it, color = c.muted, fontSize = 15.sp, modifier = Modifier.padding(top = 2.dp))
        }
    }
}

@Composable
private fun DocumentRow(
    type: DocumentTypeDto,
    current: DriverDocumentDto?,
    isUploading: Boolean,
    onUpload: () -> Unit,
) {
    val c = LocalMoveColors.current
    val (label, kind) = statusKind(current?.status ?: "missing")
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 17.dp, vertical = 16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(type.name, color = c.ink, fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
                Text(
                    listOfNotNull(
                        if (type.isRequired) "Required" else "Optional",
                        current?.expiryDate?.let { "expires $it" },
                    ).joinToString(" · "),
                    color = if (type.isRequired && current == null) c.amber else c.muted,
                    fontSize = 13.sp,
                )
            }
            MoveStatePill(text = label, color = kind.color(c))
        }

        current?.rejectionReason?.takeIf { current.status == "rejected" }?.let { reason ->
            Text("Rejected: $reason", color = c.penalty, fontSize = 13.sp)
        }

        MoveBigButton(
            label = if (current == null) "UPLOAD" else "REPLACE",
            onClick = onUpload,
            filled = false,
            icon = Icons.Filled.CloudUpload,
            loading = isUploading,
            height = 56.dp,
        )
    }
}

@Composable
private fun UploadDialog(
    type: DocumentTypeDto,
    onDismiss: () -> Unit,
    onSubmit: (
        documentNumber: String,
        fileBase64: String,
        contentType: String,
        issueDate: String?,
        expiryDate: String?,
    ) -> Unit,
) {
    val c = LocalMoveColors.current
    val context = LocalContext.current
    var documentNumber by remember { mutableStateOf("") }
    var issueDate by remember { mutableStateOf("") }
    var expiryDate by remember { mutableStateOf("") }
    var pickedFileBase64 by remember { mutableStateOf<String?>(null) }
    var pickedContentType by remember { mutableStateOf<String?>(null) }
    var pickError by remember { mutableStateOf<String?>(null) }

    val launcher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.OpenDocument(),
    ) { uri: Uri? ->
        if (uri != null) {
            val (b64, mime, err) = readUriAsBase64(context, uri)
            pickedFileBase64 = b64
            pickedContentType = mime
            pickError = err
        }
    }

    val canSubmit = documentNumber.isNotBlank() &&
        pickedFileBase64 != null &&
        pickedContentType != null

    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = c.surface,
        titleContentColor = c.ink,
        textContentColor = c.muted,
        title = { Text("Upload ${type.name}", fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 24.sp) },
        text = {
            Column(
                modifier = Modifier.verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                MoveTextField(
                    value = documentNumber,
                    onValueChange = { documentNumber = it },
                    label = "Document number",
                )
                if (type.requiresExpiry) {
                    MoveTextField(
                        value = issueDate,
                        onValueChange = { issueDate = it },
                        label = "Issue date (YYYY-MM-DD, optional)",
                    )
                    MoveTextField(
                        value = expiryDate,
                        onValueChange = { expiryDate = it },
                        label = "Expiry date (YYYY-MM-DD)",
                    )
                }
                MoveBigButton(
                    label = if (pickedFileBase64 != null) "FILE SELECTED · ${pickedContentType}" else "PICK A FILE",
                    onClick = {
                        // Image and PDF MIME types — server enforces actual allow-list.
                        launcher.launch(arrayOf("image/jpeg", "image/png", "application/pdf"))
                    },
                    filled = false,
                    icon = Icons.Filled.AttachFile,
                    height = 56.dp,
                )
                pickError?.let { Text(it, color = c.penalty, fontSize = 13.sp) }
            }
        },
        confirmButton = {
            TextButton(
                onClick = {
                    onSubmit(
                        documentNumber,
                        pickedFileBase64!!,
                        pickedContentType!!,
                        issueDate.takeIf { it.isNotBlank() },
                        expiryDate.takeIf { it.isNotBlank() },
                    )
                },
                enabled = canSubmit,
                modifier = Modifier.heightIn(min = 56.dp),
            ) { Text("SUBMIT", color = if (canSubmit) c.accent else c.muted, fontWeight = FontWeight.Bold, letterSpacing = 1.2.sp) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss, modifier = Modifier.heightIn(min = 56.dp)) {
                Text("CANCEL", color = c.muted, fontWeight = FontWeight.Bold, letterSpacing = 1.2.sp)
            }
        },
    )
}

/** How a document or profile state reads: good, in review, needs action, blocked. */
private enum class StatusKind(val panelTone: MoveTone) {
    Good(MoveTone.Neutral), Review(MoveTone.Accent), Warn(MoveTone.Amber), Bad(MoveTone.Penalty), Muted(MoveTone.Neutral);

    fun color(c: MoveColors): Color = when (this) {
        Good -> c.success
        Review -> c.accent
        Warn -> c.amber
        Bad -> c.penalty
        Muted -> c.muted
    }
}

private fun statusKind(status: String): Pair<String, StatusKind> = when (status) {
    "approved"            -> "Approved"  to StatusKind.Good
    "submitted"           -> "Submitted" to StatusKind.Review
    "pending_submission"  -> "Pending"   to StatusKind.Warn
    "rejected"            -> "Rejected"  to StatusKind.Bad
    "expired"             -> "Expired"   to StatusKind.Bad
    "suspended"           -> "Suspended" to StatusKind.Bad
    "superseded"          -> "Replaced"  to StatusKind.Muted
    "missing"             -> "Missing"   to StatusKind.Warn
    else                  -> status      to StatusKind.Muted
}

private fun statusHelp(status: String): String = when (status) {
    "approved"           -> "All documents verified. You're cleared to dispatch."
    "submitted"          -> "Documents under review by ops team."
    "pending_submission" -> "Some required documents are still missing."
    "rejected"           -> "One or more documents were rejected. Re-upload below."
    "suspended"          -> "Account suspended. Contact ops."
    else                 -> ""
}

/**
 * Reads a content:// URI and returns Base64 (no-wrap) + MIME type. Returns
 * (base64, mime, null) on success or (null, null, error) on failure. Caller
 * displays the error in the dialog instead of crashing.
 */
private fun readUriAsBase64(
    context: Context,
    uri: Uri,
): Triple<String?, String?, String?> {
    return try {
        val mime = context.contentResolver.getType(uri) ?: "application/octet-stream"
        val bytes = context.contentResolver.openInputStream(uri)?.use { it.readBytes() }
            ?: return Triple(null, null, "Could not read file")
        // 8 MB hard cap mirrors the storage adapter's max payload.
        if (bytes.size > 8 * 1024 * 1024) {
            return Triple(null, null, "File too large (max 8 MB)")
        }
        val b64 = Base64.encodeToString(bytes, Base64.NO_WRAP)
        Triple(b64, mime, null)
    } catch (e: Exception) {
        Triple(null, null, e.message ?: "Failed to read file")
    }
}
