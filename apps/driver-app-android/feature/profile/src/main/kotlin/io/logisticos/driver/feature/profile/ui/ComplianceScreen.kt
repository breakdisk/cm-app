package io.logisticos.driver.feature.profile.ui

import android.content.Context
import android.net.Uri
import android.util.Base64
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
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
import io.logisticos.driver.core.network.service.DocumentTypeDto
import io.logisticos.driver.core.network.service.DriverDocumentDto
import io.logisticos.driver.feature.profile.presentation.ComplianceViewModel

private val Canvas = Color(0xFF050810)
private val Cyan   = Color(0xFF00E5FF)
private val Green  = Color(0xFF00FF88)
private val Amber  = Color(0xFFFFAB00)
private val Red    = Color(0xFFFF3B5C)
private val Glass  = Color(0x0AFFFFFF)
private val Border = Color(0x14FFFFFF)

@Composable
fun ComplianceScreen(
    onBack: () -> Unit,
    viewModel: ComplianceViewModel = hiltViewModel(),
) {
    val state by viewModel.uiState.collectAsState()

    LaunchedEffect(Unit) { viewModel.load() }

    var pendingType by remember { mutableStateOf<DocumentTypeDto?>(null) }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(Canvas)
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 16.dp)
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            IconButton(onClick = onBack, modifier = Modifier.size(36.dp)) {
                Icon(
                    Icons.AutoMirrored.Filled.ArrowBack,
                    contentDescription = "Back",
                    tint = Color.White.copy(alpha = 0.7f),
                )
            }
            Column {
                Text(
                    "VERIFICATION",
                    color = Cyan,
                    fontSize = 11.sp,
                    fontWeight = FontWeight.Bold,
                    letterSpacing = 1.sp,
                )
                Text(
                    "Compliance Documents",
                    color = Color.White,
                    fontSize = 20.sp,
                    fontWeight = FontWeight.Bold,
                )
            }
        }

        when {
            state.loading && state.profile == null -> {
                Box(
                    Modifier.fillMaxWidth().padding(top = 64.dp),
                    contentAlignment = Alignment.Center,
                ) { CircularProgressIndicator(color = Cyan) }
            }
            state.error != null && state.profile == null -> {
                ErrorBanner(text = state.error!!, onRetry = { viewModel.load() })
            }
            state.profile != null -> {
                OverallStatusCard(status = state.profile!!.overallStatus)
                Spacer(Modifier.height(12.dp))

                state.error?.let {
                    ErrorBanner(text = it, onDismiss = viewModel::clearError)
                    Spacer(Modifier.height(8.dp))
                }

                if (state.requiredTypes.isEmpty()) {
                    Card(
                        modifier = Modifier.fillMaxWidth(),
                        colors = CardDefaults.cardColors(containerColor = Glass),
                        border = BorderStroke(1.dp, Border),
                    ) {
                        Text(
                            "No documents required for this jurisdiction.",
                            color = Color.White.copy(alpha = 0.6f),
                            fontSize = 13.sp,
                            modifier = Modifier.padding(20.dp),
                        )
                    }
                } else {
                    val byType = state.latestByTypeId
                    state.requiredTypes.forEach { type ->
                        val current = byType[type.id]
                        DocumentRow(
                            type = type,
                            current = current,
                            isUploading = state.uploadingTypeCode == type.code,
                            onUpload = { pendingType = type },
                        )
                        Spacer(Modifier.height(10.dp))
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
    val (label, color) = statusPalette(status)
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = color.copy(alpha = 0.08f)),
        border = BorderStroke(1.dp, color.copy(alpha = 0.3f)),
    ) {
        Column(
            modifier = Modifier.padding(20.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text("Overall Status", color = Color.White.copy(alpha = 0.5f), fontSize = 12.sp)
            Text(label, color = color, fontSize = 18.sp, fontWeight = FontWeight.Bold)
            Text(
                statusHelp(status),
                color = Color.White.copy(alpha = 0.6f),
                fontSize = 12.sp,
            )
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
    val (label, color) = statusPalette(current?.status ?: "missing")
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(Glass)
            .border(1.dp, Border, RoundedCornerShape(14.dp))
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(type.name, color = Color.White, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
                Text(
                    if (type.isRequired) "Required" else "Optional",
                    color = if (type.isRequired) Amber else Color.White.copy(alpha = 0.4f),
                    fontSize = 11.sp,
                )
            }
            StatusBadge(label = label, color = color)
        }

        current?.rejectionReason?.takeIf { current.status == "rejected" }?.let { reason ->
            Text(
                "Rejected: $reason",
                color = Red,
                fontSize = 12.sp,
            )
        }
        current?.expiryDate?.let { expiry ->
            Text(
                "Expires $expiry",
                color = Color.White.copy(alpha = 0.5f),
                fontSize = 11.sp,
            )
        }

        Button(
            onClick = onUpload,
            enabled = !isUploading,
            modifier = Modifier.fillMaxWidth().height(44.dp),
            colors = ButtonDefaults.buttonColors(containerColor = Cyan.copy(alpha = 0.12f)),
            border = BorderStroke(1.dp, Cyan.copy(alpha = 0.4f)),
            shape = RoundedCornerShape(10.dp),
        ) {
            if (isUploading) {
                CircularProgressIndicator(
                    color = Cyan,
                    strokeWidth = 2.dp,
                    modifier = Modifier.size(18.dp),
                )
            } else {
                Icon(
                    Icons.Default.CloudUpload,
                    contentDescription = null,
                    tint = Cyan,
                    modifier = Modifier.size(16.dp),
                )
                Spacer(Modifier.width(8.dp))
                Text(
                    if (current == null) "Upload" else "Replace",
                    color = Cyan,
                    fontSize = 14.sp,
                )
            }
        }
    }
}

@Composable
private fun StatusBadge(label: String, color: Color) {
    Box(
        modifier = Modifier
            .clip(RoundedCornerShape(6.dp))
            .background(color.copy(alpha = 0.15f))
            .border(1.dp, color.copy(alpha = 0.4f), RoundedCornerShape(6.dp))
            .padding(horizontal = 8.dp, vertical = 4.dp),
    ) {
        Text(label, color = color, fontSize = 10.sp, fontWeight = FontWeight.Bold)
    }
}

@Composable
private fun ErrorBanner(text: String, onDismiss: (() -> Unit)? = null, onRetry: (() -> Unit)? = null) {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        color = Red.copy(alpha = 0.1f),
        border = BorderStroke(1.dp, Red.copy(alpha = 0.4f)),
        shape = RoundedCornerShape(12.dp),
    ) {
        Column(modifier = Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(text, color = Red, fontSize = 12.sp)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                onRetry?.let {
                    TextButton(onClick = it) { Text("Retry", color = Cyan) }
                }
                onDismiss?.let {
                    TextButton(onClick = it) { Text("Dismiss", color = Color.White.copy(alpha = 0.6f)) }
                }
            }
        }
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
        containerColor = Color(0xFF0D1220),
        titleContentColor = Color.White,
        textContentColor = Color.White.copy(alpha = 0.7f),
        title = { Text("Upload ${type.name}", fontWeight = FontWeight.Bold) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                OutlinedTextField(
                    value = documentNumber,
                    onValueChange = { documentNumber = it },
                    label = { Text("Document number") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                    colors = textFieldColors(),
                )
                if (type.requiresExpiry) {
                    OutlinedTextField(
                        value = issueDate,
                        onValueChange = { issueDate = it },
                        label = { Text("Issue date (YYYY-MM-DD, optional)") },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth(),
                        colors = textFieldColors(),
                    )
                    OutlinedTextField(
                        value = expiryDate,
                        onValueChange = { expiryDate = it },
                        label = { Text("Expiry date (YYYY-MM-DD)") },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth(),
                        colors = textFieldColors(),
                    )
                }
                Button(
                    onClick = {
                        // Image and PDF MIME types — server enforces actual allow-list.
                        launcher.launch(arrayOf("image/jpeg", "image/png", "application/pdf"))
                    },
                    modifier = Modifier.fillMaxWidth().height(44.dp),
                    colors = ButtonDefaults.buttonColors(containerColor = Cyan.copy(alpha = 0.12f)),
                    border = BorderStroke(1.dp, Cyan.copy(alpha = 0.4f)),
                    shape = RoundedCornerShape(10.dp),
                ) {
                    Text(
                        if (pickedFileBase64 != null) "File selected (${pickedContentType})" else "Pick file (JPG / PNG / PDF)",
                        color = Cyan,
                        fontSize = 13.sp,
                    )
                }
                pickError?.let { Text(it, color = Red, fontSize = 11.sp) }
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
            ) { Text("Submit", color = if (canSubmit) Green else Color.White.copy(alpha = 0.3f)) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text("Cancel", color = Color.White.copy(alpha = 0.5f)) }
        },
    )
}

@Composable
private fun textFieldColors() = OutlinedTextFieldDefaults.colors(
    focusedBorderColor = Cyan,
    unfocusedBorderColor = Border,
    focusedTextColor = Color.White,
    unfocusedTextColor = Color.White,
    focusedLabelColor = Cyan,
    unfocusedLabelColor = Color.White.copy(alpha = 0.5f),
    cursorColor = Cyan,
)

private fun statusPalette(status: String): Pair<String, Color> = when (status) {
    "approved"            -> "Approved"  to Green
    "submitted"           -> "Submitted" to Cyan
    "pending_submission"  -> "Pending"   to Amber
    "rejected"            -> "Rejected"  to Red
    "expired"             -> "Expired"   to Red
    "suspended"           -> "Suspended" to Red
    "superseded"          -> "Replaced"  to Color.White.copy(alpha = 0.4f)
    "missing"             -> "Missing"   to Amber
    else                  -> status      to Color.White.copy(alpha = 0.5f)
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
