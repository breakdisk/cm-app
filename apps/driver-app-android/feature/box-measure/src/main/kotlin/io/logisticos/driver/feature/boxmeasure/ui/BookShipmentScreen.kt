package io.logisticos.driver.feature.boxmeasure.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.logisticos.driver.feature.boxmeasure.data.BOX_SIZES
import io.logisticos.driver.feature.boxmeasure.data.formatAmount

// ── Design tokens (mirror BoxMeasureScreen palette) ───────────────────────────
private val BkCanvas  = Color(0xFF050810)
private val BkCyan    = Color(0xFF00E5FF)
private val BkPurple  = Color(0xFFA855F7)
private val BkGreen   = Color(0xFF00FF88)
private val BkAmber   = Color(0xFFFFAB00)
private val BkGlass   = Color(0x0AFFFFFF)
private val BkBorder  = Color(0x14FFFFFF)
private val BkText    = Color(0xFFE2E8F0)
private val BkMuted   = Color(0xFF64748B)

/**
 * BookShipmentScreen — collects customer details and submits a new shipment order
 * pre-filled with the box dimensions and price calculated by the Quote tool.
 *
 * Pre-fill parameters come from [BoxMeasureScreen]'s "Book This Shipment" callback
 * and are passed through [ShiftNavGraph] as route query parameters.
 *
 * The screen is intentionally minimal: driver captures name + phone + delivery address.
 * All freight parameters (size, mode, origin, price) are shown read-only for
 * confirmation and are not editable here.
 *
 * @param prefillSizeId   matched standard box size ID
 * @param prefillFreight  freight mode name ("SEA" / "AIR")
 * @param prefillOrigin   origin location string
 * @param prefillL/W/H    measured dimensions in cm
 * @param prefillTotal    estimated total in origin currency
 * @param prefillCurrency origin currency code (e.g. "USD", "AED")
 * @param onBooked        called after the driver submits the order; navigates to Home
 * @param onBack          navigation pop
 */
@Composable
fun BookShipmentScreen(
    prefillSizeId:   String,
    prefillFreight:  String,
    prefillOrigin:   String,
    prefillL:        Double,
    prefillW:        Double,
    prefillH:        Double,
    prefillTotal:    Double,
    prefillCurrency: String,
    onBooked:        () -> Unit,
    onBack:          () -> Unit,
) {
    var customerName    by remember { mutableStateOf("") }
    var customerPhone   by remember { mutableStateOf("") }
    var deliveryAddress by remember { mutableStateOf("") }
    var notes           by remember { mutableStateOf("") }
    var submitting      by remember { mutableStateOf(false) }

    val size = BOX_SIZES.find { it.id == prefillSizeId }

    val canSubmit = customerName.isNotBlank() && customerPhone.isNotBlank() &&
                    deliveryAddress.isNotBlank() && !submitting

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(BkCanvas)
    ) {
        // ── Top bar ─────────────────────────────────────────────────────────
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onBack) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back", tint = BkText)
            }
            Column(modifier = Modifier.weight(1f).padding(start = 4.dp)) {
                Text("Book Shipment", color = BkText, fontWeight = FontWeight.Bold, fontSize = 16.sp)
                Text("Confirm details to create order", color = BkMuted, fontSize = 11.sp)
            }
        }

        Column(
            modifier = Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {

            // ── Quote summary (read-only) ──────────────────────────────────
            Surface(
                color   = BkCyan.copy(alpha = 0.05f),
                shape   = RoundedCornerShape(16.dp),
                border  = BorderStroke(1.dp, BkCyan.copy(alpha = 0.2f)),
                modifier = Modifier.fillMaxWidth(),
            ) {
                Column(
                    modifier = Modifier.padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    Row(verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Icon(Icons.Default.ViewInAr, null, tint = BkCyan, modifier = Modifier.size(16.dp))
                        Text("Quote Summary", color = BkCyan, fontWeight = FontWeight.Bold, fontSize = 13.sp)
                    }

                    QuoteSummaryRow("Box size",
                        size?.name ?: prefillSizeId)
                    if (prefillL > 0) QuoteSummaryRow("Dimensions",
                        "${prefillL} × ${prefillW} × ${prefillH} cm")
                    QuoteSummaryRow("Freight", prefillFreight.lowercase()
                        .replaceFirstChar { it.uppercase() })
                    QuoteSummaryRow("Origin", prefillOrigin)

                    Divider(color = BkBorder)

                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text("Estimated Total", color = BkMuted, fontSize = 12.sp)
                        Text(
                            formatAmount(prefillTotal, prefillCurrency),
                            color = BkCyan, fontWeight = FontWeight.ExtraBold,
                            fontSize = 20.sp, fontFamily = FontFamily.Monospace,
                        )
                    }
                }
            }

            // ── Customer details ────────────────────────────────────────────
            Text("Customer Details", color = BkMuted, fontSize = 11.sp, fontWeight = FontWeight.SemiBold)

            BookingTextField(
                value    = customerName,
                onChange = { customerName = it },
                label    = "Customer Name *",
                icon     = Icons.Default.Person,
            )
            BookingTextField(
                value         = customerPhone,
                onChange      = { customerPhone = it },
                label         = "Phone Number *",
                icon          = Icons.Default.Phone,
                keyboardType  = KeyboardType.Phone,
            )

            Text("Delivery Details", color = BkMuted, fontSize = 11.sp,
                fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(top = 4.dp))

            BookingTextField(
                value    = deliveryAddress,
                onChange = { deliveryAddress = it },
                label    = "Delivery Address *",
                icon     = Icons.Default.Place,
                maxLines = 3,
            )
            BookingTextField(
                value    = notes,
                onChange = { notes = it },
                label    = "Special Instructions (optional)",
                icon     = Icons.Default.Notes,
                maxLines = 2,
            )

            // ── Disclaimer ─────────────────────────────────────────────────
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(10.dp))
                    .background(BkAmber.copy(alpha = 0.06f))
                    .border(1.dp, BkAmber.copy(alpha = 0.2f), RoundedCornerShape(10.dp))
                    .padding(10.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.Top,
            ) {
                Icon(Icons.Default.Info, null, tint = BkAmber, modifier = Modifier.size(14.dp).padding(top = 1.dp))
                Text(
                    "This creates a draft order. Final pricing is confirmed at booking.\n" +
                    "The customer will receive a confirmation via SMS.",
                    color = BkAmber.copy(alpha = 0.8f), fontSize = 11.sp, lineHeight = 16.sp,
                )
            }

            // ── Submit ─────────────────────────────────────────────────────
            Button(
                onClick = {
                    // TODO(booking-service): POST to order-intake service with:
                    //   customerName, customerPhone, deliveryAddress, notes,
                    //   prefillSizeId, prefillFreight, prefillOrigin,
                    //   prefillL, prefillW, prefillH, prefillTotal, prefillCurrency
                    // For now, call onBooked() immediately so the driver is returned
                    // to Home. Wire to actual API when CreateShipmentUseCase is ready.
                    submitting = true
                    onBooked()
                },
                enabled  = canSubmit,
                modifier = Modifier.fillMaxWidth().height(54.dp),
                shape    = RoundedCornerShape(14.dp),
                colors   = ButtonDefaults.buttonColors(containerColor = Color.Transparent),
                contentPadding = PaddingValues(0.dp),
            ) {
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .background(
                            if (canSubmit) Brush.horizontalGradient(listOf(BkCyan, BkPurple))
                            else Brush.horizontalGradient(listOf(BkBorder, BkBorder)),
                            RoundedCornerShape(14.dp),
                        ),
                    contentAlignment = Alignment.Center,
                ) {
                    if (submitting) {
                        CircularProgressIndicator(color = BkCanvas, modifier = Modifier.size(22.dp), strokeWidth = 2.dp)
                    } else {
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp),
                            verticalAlignment = Alignment.CenterVertically) {
                            Icon(Icons.Default.CheckCircle, null, tint = BkCanvas)
                            Text("Confirm & Create Order", color = BkCanvas,
                                fontWeight = FontWeight.Bold, fontSize = 15.sp)
                        }
                    }
                }
            }

            Spacer(Modifier.height(24.dp))
        }
    }
}

// ── Sub-composables ────────────────────────────────────────────────────────────

@Composable
private fun QuoteSummaryRow(label: String, value: String) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(label, color = BkMuted, fontSize = 12.sp, modifier = Modifier.weight(1f))
        Text(value, color = BkText, fontSize = 12.sp, fontWeight = FontWeight.Medium,
            modifier = Modifier.padding(start = 8.dp))
    }
}

@Composable
private fun BookingTextField(
    value:        String,
    onChange:     (String) -> Unit,
    label:        String,
    icon:         androidx.compose.ui.graphics.vector.ImageVector,
    keyboardType: KeyboardType = KeyboardType.Text,
    maxLines:     Int = 1,
) {
    OutlinedTextField(
        value         = value,
        onValueChange = onChange,
        modifier      = Modifier.fillMaxWidth(),
        label         = { Text(label, fontSize = 12.sp) },
        leadingIcon   = { Icon(icon, null, tint = BkMuted, modifier = Modifier.size(18.dp)) },
        keyboardOptions = KeyboardOptions(keyboardType = keyboardType),
        maxLines      = maxLines,
        colors = OutlinedTextFieldDefaults.colors(
            focusedBorderColor   = BkCyan,
            unfocusedBorderColor = BkBorder,
            focusedLabelColor    = BkCyan,
            unfocusedLabelColor  = BkMuted,
            focusedTextColor     = BkText,
            unfocusedTextColor   = BkText,
            cursorColor          = BkCyan,
            focusedLeadingIconColor   = BkCyan,
            unfocusedLeadingIconColor = BkMuted,
        ),
    )
}
