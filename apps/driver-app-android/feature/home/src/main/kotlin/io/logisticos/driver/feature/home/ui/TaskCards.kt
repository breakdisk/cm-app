package io.logisticos.driver.feature.home.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.logisticos.driver.core.common.AssignmentPayload
import io.logisticos.driver.core.network.service.TaskItem
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.sin
import kotlin.math.sqrt

// ─── Palette (matches HomeScreen dark glassmorphism theme) ────────────────────
private val Canvas0 = Color(0xFF050810)
private val Cyan    = Color(0xFF00E5FF)
private val Green   = Color(0xFF00FF88)
private val Amber   = Color(0xFFFFAB00)
private val Purple  = Color(0xFFA855F7)
private val Glass   = Color(0x0AFFFFFF)
private val Border  = Color(0x14FFFFFF)
private val Red     = Color(0xFFFF4D4D)

/** Average urban driving speed used for card ETAs (km/h). */
private const val AVG_SPEED_KMH = 22.0

internal val DECLINE_REASONS = listOf(
    "ALREADY_BUSY"       to "Already on another delivery",
    "TOO_FAR"            to "Pickup too far away",
    "VEHICLE_ISSUE"      to "Vehicle issue",
    "PERSONAL_EMERGENCY" to "Personal emergency",
    "OTHER"              to "Other reason",
)

// ─── Delivery-category icon mapping ───────────────────────────────────────────

internal fun categoryEmoji(category: String): String = when (category) {
    "food"     -> "🍔"
    "grocery"  -> "🛒"
    "medicine" -> "💊"
    "heavy"    -> "⚖️"
    "large"    -> "🚛"
    else       -> "📦"   // parcel (default)
}

internal fun categoryLabel(category: String): String = when (category) {
    "food"     -> "Food"
    "grocery"  -> "Grocery"
    "medicine" -> "Medicine"
    "heavy"    -> "Heavy"
    "large"    -> "Big Shipment"
    else       -> "Parcel"
}

// ─── Distance / ETA helpers ───────────────────────────────────────────────────

internal fun haversineKm(lat1: Double, lng1: Double, lat2: Double, lng2: Double): Double {
    val r = 6371.0
    val dLat = Math.toRadians(lat2 - lat1)
    val dLng = Math.toRadians(lng2 - lng1)
    val a = sin(dLat / 2) * sin(dLat / 2) +
        cos(Math.toRadians(lat1)) * cos(Math.toRadians(lat2)) *
        sin(dLng / 2) * sin(dLng / 2)
    return r * 2 * atan2(sqrt(a), sqrt(1 - a))
}

internal fun formatKm(km: Double): String =
    if (km < 1.0) "${(km * 1000).toInt()} m" else "%.1f km".format(km)

internal fun etaLabel(km: Double): String {
    val minutes = (km / AVG_SPEED_KMH * 60).toInt().coerceAtLeast(1)
    return if (minutes >= 60) "${minutes / 60}h ${minutes % 60}m" else "$minutes min"
}

// ─── Route sketch background ──────────────────────────────────────────────────

/**
 * Stylized pickup→delivery route drawn from real coordinates, normalized into
 * the card bounds. Offline-safe (no map SDK): a dashed neon curve from the
 * pickup pin (purple) to the destination pin (green) over a faint dot grid.
 * Falls back to a generic diagonal when either end has no coordinates.
 */
@Composable
internal fun RouteSketch(
    pickupLat: Double?, pickupLng: Double?,
    deliveryLat: Double?, deliveryLng: Double?,
    modifier: Modifier = Modifier,
) {
    Canvas(modifier = modifier) {
        val w = size.width
        val h = size.height

        // Faint dot-matrix backdrop
        val step = 28.dp.toPx()
        var y = step / 2
        while (y < h) {
            var x = step / 2
            while (x < w) {
                drawCircle(Color.White.copy(alpha = 0.04f), radius = 1.2f, center = Offset(x, y))
                x += step
            }
            y += step
        }

        // Normalize the two route ends into padded card space. With only two
        // points, the mapping just decides which corners they occupy — enough
        // to convey direction without a map tile.
        val pad = 0.18f
        val (start, end) = if (pickupLat != null && pickupLng != null &&
            deliveryLat != null && deliveryLng != null &&
            (pickupLat != deliveryLat || pickupLng != deliveryLng)
        ) {
            val dxEast  = deliveryLng >= pickupLng
            val dyNorth = deliveryLat >= pickupLat
            val sx = if (dxEast) pad else 1f - pad
            val ex = if (dxEast) 1f - pad else pad
            val sy = if (dyNorth) 1f - pad else pad   // screen y grows downward
            val ey = if (dyNorth) pad else 1f - pad
            Offset(w * sx, h * sy) to Offset(w * ex, h * ey)
        } else {
            Offset(w * pad, h * (1f - pad)) to Offset(w * (1f - pad), h * pad)
        }

        // Curved dashed path with a soft glow pass underneath
        val mid = Offset((start.x + end.x) / 2, minOf(start.y, end.y) - h * 0.18f)
        val path = Path().apply {
            moveTo(start.x, start.y)
            quadraticBezierTo(mid.x, mid.y, end.x, end.y)
        }
        drawPath(
            path, Cyan.copy(alpha = 0.10f),
            style = Stroke(width = 8.dp.toPx())
        )
        drawPath(
            path,
            brush = Brush.linearGradient(listOf(Purple, Cyan, Green), start, end),
            style = Stroke(
                width = 2.dp.toPx(),
                pathEffect = PathEffect.dashPathEffect(floatArrayOf(12f, 10f)),
            ),
            alpha = 0.65f,
        )

        // Pickup pin (purple ring) and destination pin (green glow)
        drawCircle(Purple.copy(alpha = 0.25f), radius = 14.dp.toPx() / 2, center = start)
        drawCircle(Purple, radius = 4.dp.toPx() / 2, center = start)
        drawCircle(Green.copy(alpha = 0.25f), radius = 16.dp.toPx() / 2, center = end)
        drawCircle(Green, radius = 5.dp.toPx() / 2, center = end)
    }
}

// ─── Incoming offer card (Accept / Decline) ───────────────────────────────────

/**
 * Rich offer card for an incoming assignment. The driver sees merchant,
 * category icon, distances, ETA and — for gig workers only — the payout,
 * and must Accept or Decline.
 *
 * [gigPayoutCents] is non-null only for part-time drivers; full-time drivers
 * never see a price (requirement).
 */
@Composable
internal fun TaskOfferCard(
    offer: AssignmentPayload,
    gigPayoutCents: Long?,
    driverLat: Double?,
    driverLng: Double?,
    isActing: Boolean,
    onAccept: () -> Unit,
    onDecline: (reason: String) -> Unit,
) {
    var showReasonSheet by remember { mutableStateOf(false) }
    if (showReasonSheet) {
        DeclineReasonSheet(
            onDismiss = { showReasonSheet = false },
            onSelect  = { reason ->
                showReasonSheet = false
                onDecline(reason)
            },
        )
    }

    Box(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(20.dp))
            .background(Color(0xFF0A0F1E))
            .border(1.dp, Cyan.copy(alpha = 0.45f), RoundedCornerShape(20.dp)),
    ) {
        // Full-card route background
        RouteSketch(
            pickupLat = offer.pickupLat, pickupLng = offer.pickupLng,
            deliveryLat = offer.deliveryLat, deliveryLng = offer.deliveryLng,
            modifier = Modifier.matchParentSize(),
        )

        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            // Header: pulsing label + category chip
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    "INCOMING TASK",
                    color = Cyan, fontSize = 11.sp,
                    fontWeight = FontWeight.Bold, letterSpacing = 2.sp,
                )
                CategoryChip(offer.deliveryCategory, offer.weightGrams)
            }

            // Merchant (sender) — headline of the card
            Column {
                Text(
                    text = offer.merchantName.ifBlank { offer.customerName },
                    color = Color.White, fontSize = 20.sp, fontWeight = FontWeight.Bold,
                )
                Text(
                    text = if (offer.taskType == "pickup") "Pickup • ${offer.address}"
                           else "Deliver to • ${offer.address}",
                    color = Color.White.copy(alpha = 0.6f), fontSize = 12.sp,
                )
                if (offer.trackingNumber.isNotBlank()) {
                    Text(
                        offer.trackingNumber,
                        color = Cyan.copy(alpha = 0.7f), fontSize = 11.sp,
                        fontFamily = FontFamily.Monospace,
                    )
                }
            }

            // Metrics row: distance to pickup, route distance, ETA, payout (gig only)
            val toPickupKm = if (driverLat != null && driverLng != null &&
                offer.pickupLat != null && offer.pickupLng != null
            ) haversineKm(driverLat, driverLng, offer.pickupLat!!, offer.pickupLng!!) else null
            val routeKm = if (offer.pickupLat != null && offer.pickupLng != null &&
                offer.deliveryLat != null && offer.deliveryLng != null
            ) haversineKm(offer.pickupLat!!, offer.pickupLng!!, offer.deliveryLat!!, offer.deliveryLng!!) else null
            val totalKm = if (toPickupKm != null || routeKm != null)
                (toPickupKm ?: 0.0) + (routeKm ?: 0.0) else null

            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Metric("To pickup", toPickupKm?.let(::formatKm) ?: "—", Purple)
                Metric("Trip", routeKm?.let(::formatKm) ?: "—", Cyan)
                Metric("ETA", totalKm?.let(::etaLabel) ?: "—", Color.White)
                if (gigPayoutCents != null) {
                    Metric("Payout", "₱${"%.2f".format(gigPayoutCents / 100.0)}", Green)
                }
            }

            // COD badge — delivery legs with cash to collect
            if (offer.codAmountCents > 0 && offer.taskType == "delivery") {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(10.dp))
                        .background(Amber.copy(alpha = 0.12f))
                        .padding(horizontal = 12.dp, vertical = 8.dp),
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Text("💰 COD to collect", color = Amber, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
                    Text(
                        "₱${"%.2f".format(offer.codAmountCents / 100.0)}",
                        color = Amber, fontSize = 13.sp,
                        fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace,
                    )
                }
            }

            // Accept / Decline
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                OutlinedButton(
                    onClick = { showReasonSheet = true },
                    enabled = !isActing,
                    modifier = Modifier.weight(1f).height(50.dp),
                    shape = RoundedCornerShape(14.dp),
                    border = androidx.compose.foundation.BorderStroke(1.dp, Red.copy(alpha = 0.5f)),
                    colors = ButtonDefaults.outlinedButtonColors(contentColor = Red),
                ) {
                    Text("Decline", fontWeight = FontWeight.Bold, fontSize = 14.sp)
                }
                Button(
                    onClick = onAccept,
                    enabled = !isActing,
                    modifier = Modifier.weight(1f).height(50.dp),
                    shape = RoundedCornerShape(14.dp),
                    colors = ButtonDefaults.buttonColors(containerColor = Green),
                ) {
                    if (isActing) {
                        CircularProgressIndicator(
                            color = Canvas0, modifier = Modifier.size(18.dp), strokeWidth = 2.dp,
                        )
                    } else {
                        Text("Accept", color = Canvas0, fontWeight = FontWeight.Bold, fontSize = 14.sp)
                    }
                }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun DeclineReasonSheet(
    onDismiss: () -> Unit,
    onSelect: (reason: String) -> Unit,
) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        containerColor   = Color(0xFF0D1220),
        tonalElevation   = 0.dp,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 20.dp)
                .padding(bottom = 32.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                "Reason for declining",
                color = Color.White, fontSize = 17.sp, fontWeight = FontWeight.SemiBold,
                modifier = Modifier.padding(bottom = 8.dp),
            )
            DECLINE_REASONS.forEach { (code, label) ->
                OutlinedButton(
                    onClick = { onSelect(code) },
                    modifier = Modifier.fillMaxWidth().height(52.dp),
                    shape = RoundedCornerShape(12.dp),
                    border = androidx.compose.foundation.BorderStroke(1.dp, Border),
                    colors = ButtonDefaults.outlinedButtonColors(contentColor = Color.White),
                ) {
                    Text(label, fontSize = 14.sp)
                }
            }
        }
    }
}

// ─── Active task card ─────────────────────────────────────────────────────────

/**
 * Compact card for an accepted (pending / in-progress) task in the driver's
 * queue. Same visual language as the offer card, no Accept/Decline.
 * [payout] is shown only when non-null (gig workers).
 */
@Composable
internal fun TaskCard(
    task: TaskItem,
    driverLat: Double?,
    driverLng: Double?,
    onClick: () -> Unit,
) {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(16.dp))
            .background(Glass)
            .border(
                1.dp,
                if (task.status == "in_progress" || task.status == "inprogress")
                    Cyan.copy(alpha = 0.4f) else Border,
                RoundedCornerShape(16.dp),
            )
            .clickable(onClick = onClick),
    ) {
        RouteSketch(
            pickupLat = task.pickupLat, pickupLng = task.pickupLng,
            deliveryLat = task.deliveryLat, deliveryLng = task.deliveryLng,
            modifier = Modifier.matchParentSize(),
        )

        Column(
            modifier = Modifier.padding(14.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = task.merchantName.ifBlank { task.customerName },
                        color = Color.White, fontSize = 15.sp, fontWeight = FontWeight.SemiBold,
                    )
                    Text(
                        text = "${if (task.taskType == "pickup") "Pickup" else "Deliver"} • ${task.address}",
                        color = Color.White.copy(alpha = 0.55f), fontSize = 11.sp,
                        maxLines = 1,
                    )
                }
                CategoryChip(task.deliveryCategory, task.weightGrams)
            }

            // Distance to this task's stop + ETA + payout
            val targetLat = task.lat
            val targetLng = task.lng
            val distKm = if (driverLat != null && driverLng != null &&
                targetLat != null && targetLng != null
            ) haversineKm(driverLat, driverLng, targetLat, targetLng) else null

            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    "📍 ${distKm?.let(::formatKm) ?: "—"}",
                    color = Cyan, fontSize = 12.sp, fontWeight = FontWeight.Medium,
                )
                Text(
                    "⏱ ${distKm?.let(::etaLabel) ?: "—"}",
                    color = Color.White.copy(alpha = 0.7f), fontSize = 12.sp,
                )
                Spacer(Modifier.weight(1f))
                task.payoutCents?.let { payout ->
                    Text(
                        "₱${"%.2f".format(payout / 100.0)}",
                        color = Green, fontSize = 13.sp,
                        fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace,
                    )
                }
            }
        }
    }
}

// ─── Shared chips / fragments ─────────────────────────────────────────────────

@Composable
private fun CategoryChip(category: String, weightGrams: Long) {
    Row(
        modifier = Modifier
            .clip(RoundedCornerShape(8.dp))
            .background(Purple.copy(alpha = 0.12f))
            .padding(horizontal = 8.dp, vertical = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(categoryEmoji(category), fontSize = 12.sp)
        Text(
            text = if (weightGrams > 0)
                "${categoryLabel(category)} • ${"%.1f".format(weightGrams / 1000.0)} kg"
            else categoryLabel(category),
            color = Purple, fontSize = 10.sp,
            fontWeight = FontWeight.Bold, letterSpacing = 0.5.sp,
        )
    }
}

@Composable
private fun Metric(label: String, value: String, color: Color) {
    Column {
        Text(value, color = color, fontSize = 15.sp, fontWeight = FontWeight.Bold)
        Text(label, color = Color.White.copy(alpha = 0.45f), fontSize = 10.sp)
    }
}

// ─── Performance strip ────────────────────────────────────────────────────────

/**
 * Driver accountability strip: customer rating + (gig only) decline counter.
 * Declines turn amber at 75% of the ban limit so the driver sees the cliff
 * coming before the automatic deactivation at [banLimit].
 */
@Composable
internal fun PerformanceStrip(
    ratingAvg: Float?,
    ratingCount: Int,
    declineCount: Int,
    banLimit: Int,
    isGigWorker: Boolean,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(Glass)
            .border(1.dp, Border, RoundedCornerShape(14.dp))
            .padding(horizontal = 16.dp, vertical = 10.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Text("⭐", fontSize = 14.sp)
            Text(
                text = ratingAvg?.let { "%.1f".format(it) } ?: "New",
                color = Color.White, fontSize = 14.sp, fontWeight = FontWeight.Bold,
            )
            if (ratingCount > 0) {
                Text("($ratingCount)", color = Color.White.copy(alpha = 0.4f), fontSize = 11.sp)
            }
        }
        if (isGigWorker) {
            val nearBan = declineCount >= (banLimit * 3) / 4
            Text(
                text = "Declines $declineCount/$banLimit",
                color = if (nearBan) Amber else Color.White.copy(alpha = 0.55f),
                fontSize = 12.sp,
                fontWeight = if (nearBan) FontWeight.Bold else FontWeight.Medium,
            )
        }
    }
}
