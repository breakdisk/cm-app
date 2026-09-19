package io.logisticos.driver.move

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowBack
import androidx.compose.material.icons.filled.Navigation
import androidx.compose.material.icons.filled.Notifications
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.PowerSettingsNew
import androidx.compose.material.icons.filled.Public
import androidx.compose.material.icons.filled.WbSunny
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.core.common.AssignmentPayload
import io.logisticos.driver.core.designsystem.Condensed
import io.logisticos.driver.core.designsystem.LocalMoveColors
import io.logisticos.driver.core.designsystem.MoveTheme
import io.logisticos.driver.core.network.service.TaskItem
import io.logisticos.driver.feature.home.presentation.HomeUiState
import io.logisticos.driver.feature.home.presentation.HomeViewModel
import io.logisticos.driver.feature.home.presentation.reservationLine
import io.logisticos.driver.feature.profile.presentation.EarningsViewModel
import io.logisticos.driver.feature.profile.presentation.HosUiState
import io.logisticos.driver.feature.profile.presentation.HosViewModel
import io.logisticos.driver.feature.profile.ui.HosSummaryLine
import java.util.Locale

/**
 * The Move build's Loads board: duty card, today's earnings, the open load and
 * the driver's stops, per the driver design (screenshot 01-driver).
 *
 * Behaviour is the existing home screen's — the same [HomeViewModel] owns going
 * on and off duty, the location service, the offer poll, claim and pass — so
 * only the presentation is new. Glove-operated: 56–68 dp controls throughout.
 */
@Composable
fun LoadsBoardScreen(
    sun: Boolean,
    onToggleSun: () -> Unit,
    onNavigateToTask: (taskId: String) -> Unit,
    onOpenNotifications: () -> Unit,
    onOpenProfile: () -> Unit,
    viewModel: HomeViewModel = hiltViewModel(),
    earningsViewModel: EarningsViewModel = hiltViewModel(),
    hosViewModel: HosViewModel = hiltViewModel(),
) {
    val state by viewModel.uiState.collectAsState()
    val earnings by earningsViewModel.uiState.collectAsState()
    val hos by hosViewModel.uiState.collectAsState()
    // The hours-of-service clock moves when duty does.
    LaunchedEffect(state.isOnline) { hosViewModel.refresh() }
    RequestDriverPermissions(
        onGranted = viewModel::onLocationPermissionGranted,
        onDenied = viewModel::onLocationPermissionDenied,
    )

    val offer = state.pendingOffer?.takeIf { it.isGrabOffer }
    var reviewing by rememberSaveable { mutableStateOf(false) }
    LaunchedEffect(offer?.offerId) { if (offer == null) reviewing = false }

    MoveTheme(sun, onToggleSun) {
        val c = LocalMoveColors.current
        Column(Modifier.fillMaxSize().background(c.ground)) {
            BoardHeader(
                label = if (reviewing && offer != null) "OFFER · ${offer.trackingNumber.ifBlank { "NEW LOAD" }}" else "LOAD BOARD",
                title = if (reviewing && offer != null) "Review the load" else "Loads near you",
                showBack = reviewing && offer != null,
                onBack = { reviewing = false },
                sun = sun,
                onToggleSun = onToggleSun,
                onOpenNotifications = onOpenNotifications,
                onOpenProfile = onOpenProfile,
            )
            if (reviewing && offer != null) {
                OfferDetail(
                    offer = offer,
                    secondsLeft = state.offerSecondsLeft,
                    taken = state.offerTaken,
                    acting = state.isActingOnOffer,
                    onClaim = viewModel::claimOffer,
                    onPass = viewModel::passOffer,
                )
            } else {
                Board(
                    state = state,
                    offer = offer,
                    todayCents = earnings.earnings?.todayCents,
                    weekCents = earnings.earnings?.weekCents,
                    isGig = earnings.isGigWorker,
                    onToggleDuty = viewModel::toggleOnlineStatus,
                    onOpenOffer = { reviewing = true },
                    onNavigateToTask = onNavigateToTask,
                    hos = hos,
                    onDismissReservation = viewModel::dismissReservation,
                )
            }
        }
    }
}

// ── Board ────────────────────────────────────────────────────────────────────

@Composable
private fun Board(
    state: HomeUiState,
    offer: AssignmentPayload?,
    todayCents: Long?,
    weekCents: Long?,
    isGig: Boolean,
    onToggleDuty: () -> Unit,
    onOpenOffer: () -> Unit,
    onNavigateToTask: (String) -> Unit,
    hos: HosUiState,
    onDismissReservation: () -> Unit,
) {
    val c = LocalMoveColors.current
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(start = 16.dp, end = 16.dp, top = 6.dp, bottom = 26.dp),
    ) {
        state.reservedMoveDate?.let { day ->
            Box(Modifier.clickable(onClickLabel = "Dismiss", onClick = onDismissReservation)) {
                Notice("Home move reserved", reservationLine(day), amber = false)
            }
            Spacer(Modifier.height(12.dp))
        }
        if (state.locationDenied) {
            Notice("Location is off", "Dispatch can't find you without it. Allow location for this app in Settings.", amber = true)
            Spacer(Modifier.height(12.dp))
        }

        // Duty card
        val onDuty = state.isOnline
        Panel(
            background = if (onDuty) c.accentPanel else c.amberPanel,
            border = if (onDuty) c.accentBorder else c.amberBorder,
        ) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(13.dp)) {
                Box(Modifier.size(12.dp).clip(CircleShape).background(if (onDuty) c.accent else c.amber))
                Column(Modifier.weight(1f)) {
                    Text(
                        if (onDuty) "On duty" else "Off duty",
                        color = if (onDuty) c.accent else c.amber,
                        fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 24.sp,
                    )
                    Text(
                        if (onDuty) "Loads come to you while you're on duty" else "Board paused — dispatch routes around you",
                        color = c.muted, fontSize = 14.sp,
                    )
                    hos.clock?.let { HosSummaryLine(it, hos.fetchedAtMillis, Modifier.padding(top = 4.dp)) }
                }
            }
            Spacer(Modifier.height(16.dp))
            BigButton(
                label = if (onDuty) "GO OFF DUTY" else "GO ON DUTY",
                icon = Icons.Filled.PowerSettingsNew,
                filled = !onDuty,
                loading = state.isTogglingStatus,
                onClick = onToggleDuty,
            )
            state.error?.let {
                Spacer(Modifier.height(10.dp))
                Text(it, color = c.penalty, fontSize = 13.sp)
            }
        }

        Spacer(Modifier.height(16.dp))

        // Earnings
        Panel {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
                Text(if (isGig) "TODAY'S EARNINGS" else "TODAY", color = c.accent, fontSize = 12.sp, letterSpacing = 2.4.sp)
                if (isGig && weekCents != null) Text("This week ${money(weekCents)}", color = c.muted, fontSize = 13.sp)
            }
            Text(
                if (isGig) todayCents?.let(::money) ?: "—" else "${state.tasks.size} stops",
                color = c.ink, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 60.sp, lineHeight = 64.sp,
                maxLines = 1,
            )
            Spacer(Modifier.height(10.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(24.dp)) {
                Stat("STOPS", state.tasks.size.toString())
                Stat(
                    "ACCEPTANCE",
                    state.acceptancePct?.let { "$it%" }
                        ?: if (state.offersSeen > 0) "${state.offersClaimed * 100 / state.offersSeen}%" else "—",
                )
                Stat("RATING", state.ratingAvg?.let { String.format(Locale.US, "%.1f", it) } ?: "—")
            }
        }

        // Loads
        SectionHeading(
            dot = if (onDuty) c.accent else c.amber,
            text = when {
                !onDuty -> "Load board paused"
                offer != null -> "1 load open"
                else -> "No loads right now"
            },
        )
        when {
            !onDuty -> DashedPanel(
                title = "Nothing will be offered",
                body = "Go on duty to see loads again.",
            )
            offer != null -> LoadCard(offer, state.offerSecondsLeft, state.offerTaken, onOpenOffer)
            else -> DashedPanel(
                title = "Waiting for dispatch",
                body = "A load appears here the moment one is offered to you.",
            )
        }

        // Stops
        if (state.tasks.isNotEmpty()) {
            SectionHeading(dot = c.accent, text = "Your stops")
            Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
                state.tasks.sortedBy { it.sequence }.forEach { task ->
                    StopCard(task) { onNavigateToTask(task.taskId) }
                }
            }
        }
    }
}

@Composable
private fun LoadCard(offer: AssignmentPayload, secondsLeft: Int?, taken: Boolean, onOpen: () -> Unit) {
    val c = LocalMoveColors.current
    val shape = RoundedCornerShape(22.dp)
    Column(
        Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(c.accentPanel)
            .border(1.dp, c.accentBorder, shape)
            .clickable(enabled = !taken, role = Role.Button, onClickLabel = "Review the load", onClick = onOpen)
            .padding(20.dp),
    ) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
            Text(
                "${offer.trackingNumber.ifBlank { "NEW LOAD" }} · 1 STOP",
                color = c.muted, fontSize = 12.sp, letterSpacing = 2.sp, maxLines = 1, overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            if (taken) {
                Pill("TAKEN", c.penalty, c.chip)
            } else if (secondsLeft != null) {
                Pill("${secondsLeft}s", c.accentInk, c.accent)
            }
        }
        Row(Modifier.fillMaxWidth().padding(top = 12.dp), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.Bottom) {
            Text(
                offer.payoutCents?.let(::money) ?: "Rate hidden",
                color = c.ink, fontFamily = Condensed, fontWeight = FontWeight.Bold,
                fontSize = if (offer.payoutCents != null) 46.sp else 28.sp,
            )
            Column(horizontalAlignment = Alignment.End) {
                Text(offer.deliveryCategory.uppercase(Locale.US), color = c.accent, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 22.sp)
                if (offer.weightGrams > 0) Text(kg(offer.weightGrams), color = c.muted, fontSize = 12.sp)
            }
        }
        Text(
            "${offer.merchantName.ifBlank { "Pickup" }} → ${offer.customerName.ifBlank { "Drop-off" }}",
            color = c.ink, fontSize = 17.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(top = 14.dp),
        )
        if (offer.address.isNotBlank()) Text(offer.address, color = c.muted, fontSize = 14.sp, modifier = Modifier.padding(top = 4.dp))
    }
}

@Composable
private fun StopCard(task: TaskItem, onNavigate: () -> Unit) {
    val c = LocalMoveColors.current
    Panel {
        Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            Box(
                Modifier.size(44.dp).clip(RoundedCornerShape(14.dp)).background(c.accentPanel),
                contentAlignment = Alignment.Center,
            ) {
                Text(task.sequence.toString(), color = c.accent, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 22.sp)
            }
            Column(Modifier.weight(1f)) {
                Text(
                    task.customerName.ifBlank { task.merchantName.ifBlank { "Stop ${task.sequence}" } },
                    color = c.ink, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 24.sp,
                    maxLines = 1, overflow = TextOverflow.Ellipsis,
                )
                Text(task.address, color = c.ink, fontSize = 15.sp, modifier = Modifier.padding(top = 4.dp))
                Text(
                    listOfNotNull(task.taskType.uppercase(Locale.US), task.trackingNumber).joinToString(" · "),
                    color = c.muted, fontSize = 13.sp, modifier = Modifier.padding(top = 3.dp),
                )
                Spacer(Modifier.height(14.dp))
                BigButton(label = "NAVIGATE", icon = Icons.Filled.Navigation, filled = false, height = 56, onClick = onNavigate)
            }
        }
    }
}

// ── Offer detail ─────────────────────────────────────────────────────────────

@Composable
private fun OfferDetail(
    offer: AssignmentPayload,
    secondsLeft: Int?,
    taken: Boolean,
    acting: Boolean,
    onClaim: () -> Unit,
    onPass: () -> Unit,
) {
    val c = LocalMoveColors.current
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(start = 16.dp, end = 16.dp, top = 6.dp, bottom = 26.dp),
    ) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            Text("YOUR RATE", color = c.muted, fontSize = 12.sp, letterSpacing = 2.4.sp)
            if (secondsLeft != null && !taken) Text("Expires in ${secondsLeft}s", color = c.accent, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
        }
        Text(
            offer.payoutCents?.let(::money) ?: "Rate hidden",
            color = c.ink, fontFamily = Condensed, fontWeight = FontWeight.Bold,
            fontSize = if (offer.payoutCents != null) 72.sp else 36.sp, lineHeight = 76.sp,
        )
        Row(Modifier.padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(11.dp)) {
            Icon(Icons.Filled.Public, contentDescription = null, tint = c.accent, modifier = Modifier.size(18.dp))
            Text(
                "Your contractual rate for this load. Passing never counts against you.",
                color = c.muted, fontSize = 13.sp,
            )
        }

        Spacer(Modifier.height(22.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            StatTile("WEIGHT", if (offer.weightGrams > 0) kg(offer.weightGrams) else "—", "declared", Modifier.weight(1f))
            StatTile("LOAD", offer.deliveryCategory.replaceFirstChar { it.titlecase(Locale.US) }, "type", Modifier.weight(1f))
        }
        Spacer(Modifier.height(16.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            StatTile("COLLECT", if (offer.codAmountCents > 0) money(offer.codAmountCents) else "None", "cash at door", Modifier.weight(1f))
            StatTile("FOR", offer.customerName.ifBlank { "—" }, "drop-off", Modifier.weight(1f))
        }

        Spacer(Modifier.height(28.dp))
        Text("MANIFEST", color = c.muted, fontSize = 12.sp, letterSpacing = 2.4.sp)
        Spacer(Modifier.height(14.dp))
        Panel(padding = 0) {
            ManifestRow("Pick up from", offer.merchantName.ifBlank { "Sender" })
            ManifestRow("Deliver to", offer.address.ifBlank { "—" })
            ManifestRow("Tracking", offer.trackingNumber.ifBlank { "—" }, last = true)
        }

        Spacer(Modifier.height(20.dp))
        BigButton(
            // payoutCents is declared in core:common, so it cannot be smart-cast here.
            label = if (taken) "TAKEN BY ANOTHER DRIVER"
                else offer.payoutCents?.let { "CLAIM AT ${money(it)}" } ?: "CLAIM THIS LOAD",
            filled = true,
            enabled = !taken,
            loading = acting,
            onClick = onClaim,
        )
        Spacer(Modifier.height(12.dp))
        val passShape = RoundedCornerShape(18.dp)
        Box(
            Modifier
                .fillMaxWidth()
                .height(60.dp)
                .clip(passShape)
                .border(1.dp, c.penaltyBorder, passShape)
                .clickable(enabled = !acting, role = Role.Button, onClick = onPass),
            contentAlignment = Alignment.Center,
        ) {
            Text("PASS — NO PENALTY", color = c.penalty, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, letterSpacing = 1.1.sp)
        }
        Text(
            "This load won't be offered to you again.",
            color = c.muted, fontSize = 12.sp, modifier = Modifier.padding(top = 10.dp),
        )
    }
}

// ── Pieces ───────────────────────────────────────────────────────────────────

@Composable
private fun BoardHeader(
    label: String,
    title: String,
    showBack: Boolean,
    onBack: () -> Unit,
    sun: Boolean,
    onToggleSun: () -> Unit,
    onOpenNotifications: () -> Unit,
    onOpenProfile: () -> Unit,
) {
    val c = LocalMoveColors.current
    Row(
        Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        if (showBack) SquareButton(Icons.Filled.ArrowBack, "Back", onBack)
        Column(Modifier.weight(1f)) {
            Text(label, color = c.muted, fontSize = 12.sp, letterSpacing = 2.6.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(title, color = c.ink, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 22.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        if (!showBack) {
            SquareButton(Icons.Filled.Notifications, "Notifications", onOpenNotifications)
            SquareButton(Icons.Filled.Person, "Profile", onOpenProfile)
        }
        SquareButton(Icons.Filled.WbSunny, if (sun) "Sun mode on" else "Sun mode off", onToggleSun, active = sun)
    }
}

@Composable
private fun SquareButton(icon: ImageVector, label: String, onClick: () -> Unit, active: Boolean = false) {
    val c = LocalMoveColors.current
    val shape = RoundedCornerShape(16.dp)
    Box(
        Modifier
            .size(56.dp)
            .clip(shape)
            .background(if (active) c.accentPanel else c.chip)
            .border(1.dp, if (active) c.accentBorder else c.hairline, shape)
            .clickable(role = Role.Button, onClickLabel = label, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, contentDescription = label, tint = if (active) c.accent else c.ink, modifier = Modifier.size(24.dp))
    }
}

@Composable
private fun BigButton(
    label: String,
    onClick: () -> Unit,
    filled: Boolean,
    icon: ImageVector? = null,
    enabled: Boolean = true,
    loading: Boolean = false,
    height: Int = 68,
) {
    val c = LocalMoveColors.current
    val shape = RoundedCornerShape(18.dp)
    Row(
        Modifier
            .fillMaxWidth()
            .height(height.dp)
            .clip(shape)
            .background(if (filled) c.accent else c.chip)
            .border(1.dp, if (filled) c.accent else c.hairline, shape)
            .clickable(enabled = enabled && !loading, role = Role.Button, onClick = onClick),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val fg = if (filled) c.accentInk else c.ink
        if (loading) {
            CircularProgressIndicator(color = fg, strokeWidth = 2.5.dp, modifier = Modifier.size(24.dp))
        } else {
            if (icon != null) {
                Icon(icon, contentDescription = null, tint = if (filled) fg else c.accent, modifier = Modifier.size(24.dp))
                Spacer(Modifier.size(12.dp))
            }
            Text(label, color = if (enabled) fg else c.muted, fontSize = 16.sp, fontWeight = FontWeight.Bold, letterSpacing = 1.6.sp)
        }
    }
}

@Composable
private fun Panel(
    background: Color = LocalMoveColors.current.panel,
    border: Color = LocalMoveColors.current.hairline,
    padding: Int = 18,
    content: @Composable () -> Unit,
) {
    val shape = RoundedCornerShape(22.dp)
    Column(
        Modifier.fillMaxWidth().clip(shape).background(background).border(1.dp, border, shape).padding(padding.dp),
    ) { content() }
}

@Composable
private fun DashedPanel(title: String, body: String) {
    val c = LocalMoveColors.current
    Panel {
        Text(title, color = c.ink, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 22.sp)
        Text(body, color = c.muted, fontSize = 15.sp, modifier = Modifier.padding(top = 8.dp))
    }
}

@Composable
private fun Notice(title: String, body: String, amber: Boolean) {
    val c = LocalMoveColors.current
    Panel(background = if (amber) c.amberPanel else c.panel, border = if (amber) c.amberBorder else c.hairline) {
        Text(title, color = if (amber) c.amber else c.ink, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 17.sp)
        Text(body, color = c.muted, fontSize = 13.sp, modifier = Modifier.padding(top = 2.dp))
    }
}

@Composable
private fun SectionHeading(dot: Color, text: String) {
    val c = LocalMoveColors.current
    Row(Modifier.padding(top = 26.dp, bottom = 14.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Box(Modifier.size(8.dp).clip(CircleShape).background(dot))
        Text(text.uppercase(Locale.US), color = c.muted, fontSize = 12.sp, letterSpacing = 2.4.sp)
    }
}

@Composable
private fun Stat(label: String, value: String) {
    val c = LocalMoveColors.current
    Column {
        Text(label, color = c.muted, fontSize = 11.sp, letterSpacing = 2.sp)
        Text(value, color = c.ink, fontFamily = Condensed, fontWeight = FontWeight.SemiBold, fontSize = 24.sp)
    }
}

@Composable
private fun StatTile(label: String, value: String, meta: String, modifier: Modifier) {
    val c = LocalMoveColors.current
    val shape = RoundedCornerShape(18.dp)
    Column(modifier.clip(shape).background(c.panel).border(1.dp, c.hairline, shape).padding(16.dp)) {
        Text(label, color = c.muted, fontSize = 11.sp, letterSpacing = 2.sp)
        Text(value, color = c.ink, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 26.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(meta, color = c.muted, fontSize = 12.sp)
    }
}

@Composable
private fun ManifestRow(label: String, value: String, last: Boolean = false) {
    val c = LocalMoveColors.current
    Column(Modifier.fillMaxWidth().padding(horizontal = 17.dp, vertical = 16.dp)) {
        Text(label, color = c.muted, fontSize = 13.sp)
        Text(value, color = c.ink, fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
    }
    if (!last) Box(Modifier.fillMaxWidth().height(1.dp).background(c.hairline))
}

@Composable
private fun Pill(text: String, fg: Color, bg: Color) {
    Text(
        text,
        color = fg,
        fontSize = 11.sp,
        fontWeight = FontWeight.Bold,
        letterSpacing = 1.4.sp,
        modifier = Modifier.clip(RoundedCornerShape(999.dp)).background(bg).padding(horizontal = 11.dp, vertical = 6.dp),
    )
}

/**
 * Same checklist as the classic home screen: foreground location (dispatch
 * proximity + GPS heartbeat), notifications on API 33+ (FCM offers), then
 * background location as a separate follow-up request (Android 10+ rule).
 */
@Composable
private fun RequestDriverPermissions(onGranted: () -> Unit, onDenied: () -> Unit) {
    val context = LocalContext.current
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { grants ->
        val fine = grants[Manifest.permission.ACCESS_FINE_LOCATION] == true
        val coarse = grants[Manifest.permission.ACCESS_COARSE_LOCATION] == true
        if (fine || coarse) {
            onGranted()
        } else if (grants.containsKey(Manifest.permission.ACCESS_FINE_LOCATION) ||
            grants.containsKey(Manifest.permission.ACCESS_COARSE_LOCATION)
        ) {
            onDenied()
        }
    }
    val backgroundLauncher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { }

    LaunchedEffect(Unit) {
        fun granted(p: String) = ContextCompat.checkSelfPermission(context, p) == PackageManager.PERMISSION_GRANTED
        val needed = mutableListOf<String>()
        if (!granted(Manifest.permission.ACCESS_FINE_LOCATION) && !granted(Manifest.permission.ACCESS_COARSE_LOCATION)) {
            needed += Manifest.permission.ACCESS_FINE_LOCATION
            needed += Manifest.permission.ACCESS_COARSE_LOCATION
        }
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.TIRAMISU &&
            !granted(Manifest.permission.POST_NOTIFICATIONS)
        ) {
            needed += Manifest.permission.POST_NOTIFICATIONS
        }
        if (needed.isNotEmpty()) launcher.launch(needed.toTypedArray()) else onGranted()

        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.Q &&
            granted(Manifest.permission.ACCESS_FINE_LOCATION) &&
            !granted(Manifest.permission.ACCESS_BACKGROUND_LOCATION)
        ) {
            backgroundLauncher.launch(Manifest.permission.ACCESS_BACKGROUND_LOCATION)
        }
    }
}

/** Driver payouts are pesos across the existing app (TaskCards); kept consistent here. */
private fun money(cents: Long): String =
    if (cents % 100 == 0L) "₱" + String.format(Locale.US, "%,d", cents / 100)
    else "₱" + String.format(Locale.US, "%,.2f", cents / 100.0)

private fun kg(grams: Long): String {
    val kg = grams / 1000.0
    return if (kg >= 100) String.format(Locale.US, "%,d kg", kg.toLong()) else String.format(Locale.US, "%.1f kg", kg)
}
