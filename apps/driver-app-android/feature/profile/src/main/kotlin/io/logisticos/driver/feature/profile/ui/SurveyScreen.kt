package io.logisticos.driver.feature.profile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Remove
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import kotlinx.coroutines.launch
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.core.designsystem.*
import io.logisticos.driver.core.network.service.HomeCatalogueItemDto
import io.logisticos.driver.core.network.service.HomeMoveDto
import io.logisticos.driver.feature.profile.presentation.ExtraDraft
import io.logisticos.driver.feature.profile.presentation.SurveyUiState
import io.logisticos.driver.feature.profile.presentation.SurveyViewModel
import io.logisticos.driver.feature.profile.presentation.addendumLine
import io.logisticos.driver.feature.profile.presentation.centsFromText
import io.logisticos.driver.feature.profile.presentation.crewLine
import io.logisticos.driver.feature.profile.presentation.extrasTotal
import io.logisticos.driver.feature.profile.presentation.money
import io.logisticos.driver.feature.profile.presentation.qtyOf
import io.logisticos.driver.feature.profile.presentation.whenLabel

/**
 * The lead's survey of a reserved home move. What the lead records here can
 * only add to the booking — items found beyond it, packing materials and
 * resources — and goes to the customer to approve in their app before any
 * more is charged. An empty survey records that the home matched.
 */
@Composable
fun SurveyScreen(
    shipmentId: String,
    onBack: () -> Unit,
    viewModel: SurveyViewModel = hiltViewModel(),
) {
    val s by viewModel.uiState.collectAsState()
    val c = LocalMoveColors.current
    LaunchedEffect(shipmentId) { viewModel.open(shipmentId) }

    Column(
        Modifier
            .fillMaxSize()
            .background(c.ground)
            .verticalScroll(rememberScrollState()),
    ) {
        MoveScreenHeader(label = "Survey", title = "What's in the home", onBack = onBack)
        Column(Modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            when {
                s.loading && s.move == null -> Box(Modifier.fillMaxWidth().padding(top = 48.dp), contentAlignment = Alignment.Center) {
                    CircularProgressIndicator(color = c.accent, strokeWidth = 2.dp)
                }
                s.move == null -> {
                    MoveNotice("Couldn't open the move", s.error ?: "Try again.", tone = MoveTone.Penalty)
                    MoveBigButton("TRY AGAIN", onClick = viewModel::load, filled = false, height = 56.dp)
                }
                s.result != null -> {
                    Sent(s, onBack)
                    HardStop(s, viewModel::approveOnSite)
                }
                else -> {
                    HardStop(s, viewModel::approveOnSite)
                    Form(s, viewModel)
                }
            }
        }
        Spacer(Modifier.navigationBarsPadding().height(24.dp))
    }
}

/**
 * The hard stop: nothing the survey added is loaded until the customer
 * approves its price — in their app, or here with the code they were sent.
 * Once approved, the customer pays the addition on this phone or their own.
 */
@Composable
private fun HardStop(s: SurveyUiState, onApprove: (String) -> Unit) {
    val c = LocalMoveColors.current
    val uri = androidx.compose.ui.platform.LocalUriHandler.current
    s.checkoutUrl?.let { url ->
        MoveNotice(
            title = "Approved — load the extra items",
            body = "The customer agreed the addition. They pay it here or in their app.",
            tone = MoveTone.Accent,
        )
        if (url.isNotBlank()) {
            MoveBigButton("CUSTOMER PAYS HERE", onClick = { runCatching { uri.openUri(url) } }, filled = false, height = 56.dp)
        }
        return
    }
    val a = s.pendingAddendum ?: return
    var code by rememberSaveable(a.id) { mutableStateOf("") }
    MovePanel(tone = MoveTone.Amber) {
        Text("HARD STOP", color = c.amber, fontSize = 12.sp, fontWeight = FontWeight.Bold, letterSpacing = 1.6.sp)
        Text(
            "Don't load what the survey added (+${money(a.totalCents, a.currency)}) until the customer approves it.",
            color = c.ink, fontSize = 16.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(top = 4.dp),
        )
        Text(
            "They can approve in their app — or here, with the code they were sent.",
            color = c.muted, fontSize = 13.sp, modifier = Modifier.padding(top = 4.dp),
        )
        Spacer(Modifier.height(10.dp))
        MoveTextField(
            value = code,
            onValueChange = { code = it.filter(Char::isDigit).take(6) },
            label = "Customer's approval code",
            keyboardType = KeyboardType.NumberPassword,
            mono = true,
            large = true,
            isError = s.approveError != null,
            supportingText = s.approveError,
        )
        Spacer(Modifier.height(10.dp))
        MoveBigButton(
            label = "CUSTOMER APPROVES",
            onClick = { onApprove(code) },
            enabled = code.length == 6 && !s.approving,
            loading = s.approving,
            height = 56.dp,
        )
    }
}

@Composable
private fun Sent(s: SurveyUiState, onDone: () -> Unit) {
    val addendum = s.result?.addendum
    if (addendum == null) {
        MoveNotice(
            title = "Survey recorded",
            body = "Nothing beyond the booking. The move stands as booked.",
            tone = MoveTone.Accent,
        )
    } else {
        MoveNotice(
            title = "Sent to the customer",
            body = "They see +${money(addendum.totalCents, addendum.currency)} to approve in their app. " +
                "Nothing more is charged until they do. With it, the job is ${crewLine(addendum.trucks, addendum.crewTotal)}.",
            tone = MoveTone.Accent,
        )
    }
    MoveBigButton("DONE", onClick = onDone, height = 56.dp)
}

@Composable
private fun Form(s: SurveyUiState, vm: SurveyViewModel) {
    val c = LocalMoveColors.current
    val move = s.move ?: return

    MoveSummary(move, s.leadName)

    s.pay?.takeIf { it.surveyPayoutCents > 0 && move.surveySubmittedAt == null }?.let { p ->
        MoveNotice(
            title = "Signing off pays you ${money(p.surveyPayoutCents, move.currency)}",
            body = "The survey fee, less the platform's commission, goes to your earnings when you send this — whether or not the move goes ahead.",
            tone = MoveTone.Accent,
        )
    }

    s.addendum?.let { a ->
        MoveNotice(
            title = if (a.status == "pending") "Earlier survey waiting" else "Earlier survey",
            body = addendumLine(a),
            tone = if (a.status == "pending") MoveTone.Amber else MoveTone.Neutral,
        )
    }
    move.surveySubmittedAt?.let {
        Text("Surveyed ${whenLabel(it)}. A new survey adds to that one.", color = c.muted, fontSize = 13.sp)
    }

    BookedItems(move)

    MoveLabel("Found beyond the booking")
    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        s.rooms.forEach { r ->
            val added = s.lines.filter { it.room == r.key }.sumOf { it.qty }
            MoveChip(
                label = if (added > 0) "${r.name} · $added" else r.name,
                selected = r.key == s.room?.key,
                onClick = { vm.selectRoom(r.key) },
            )
        }
    }
    s.room?.let { room ->
        Column(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(20.dp))
                .background(c.panel)
                .border(1.dp, c.hairline, RoundedCornerShape(20.dp)),
        ) {
            if (room.items.isEmpty()) {
                Text("No catalogue items for this room.", color = c.muted, fontSize = 14.sp, modifier = Modifier.padding(16.dp))
            }
            room.items.forEachIndexed { i, item ->
                ItemRow(
                    item = item,
                    qty = qtyOf(s.lines, room.key, item.key),
                    line = s.lines.firstOrNull { it.room == room.key && it.itemKey == item.key },
                    onAdd = { vm.add(room.key, item) },
                    onRemove = { vm.remove(room.key, item.key) },
                    onDismantle = { vm.toggleDismantle(room.key, item.key) },
                    onPacking = { vm.togglePacking(room.key, item.key) },
                )
                if (i < room.items.lastIndex) MoveDivider()
            }
        }
    }

    MoveLabel("Photos")
    Photos(s, vm)

    MoveLabel("Materials & resources")
    Extras(s, vm, move.currency)

    MoveTextField(
        value = s.note,
        onValueChange = vm::setNote,
        label = "Note for the customer (optional)",
        singleLine = false,
        supportingText = "${s.note.length}/500",
    )

    val count = s.lines.sumOf { it.qty }
    val anything = count > 0 || s.extras.isNotEmpty()
    Text(
        if (anything) {
            "$count item${if (count == 1) "" else "s"} and ${s.extras.size} extra${if (s.extras.size == 1) "" else "s"} to add. " +
                "The server prices them; the agreed ${money(move.totalCents, move.currency)} never goes down."
        } else {
            "Nothing added. Sending records that the home matched the booking."
        },
        color = c.muted, fontSize = 13.sp,
    )
    if (s.waitingOnCustomer) {
        Text("Survey again once the customer has answered the earlier addition.", color = c.muted, fontSize = 13.sp)
    }
    s.problem?.let { MoveNotice("Check the survey", it, tone = MoveTone.Amber) }
    s.submitError?.let { MoveNotice("The survey didn't send", it, tone = MoveTone.Penalty) }
    MoveBigButton(
        label = if (anything) "SEND TO THE CUSTOMER" else "RECORD: NOTHING MORE",
        onClick = vm::submit,
        enabled = s.canSubmit,
        loading = s.submitting,
    )
}

@Composable
private fun MoveSummary(move: HomeMoveDto, leadName: String?) {
    val p = move.property
    MovePanel {
        Text(
            "${p.type.replaceFirstChar { it.uppercase() }} · ${p.size}",
            color = LocalMoveColors.current.ink, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 22.sp,
        )
        Spacer(Modifier.height(10.dp))
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            leadName?.let { MoveDetailRow("Team", it) }
            MoveDetailRow("Crew", crewLine(move.trucks, move.crewTotal))
            MoveDetailRow("Pickup", floorLine(p.pickupFloor, p.pickupHasLift))
            MoveDetailRow("Drop-off", floorLine(p.dropoffFloor, p.dropoffHasLift) + if (p.longCarry) " · long carry" else "")
            MoveDetailRow("Move", whenLabel(move.moveAt))
            MoveDetailRow("Agreed", money(move.totalCents, move.currency))
        }
    }
}

private fun floorLine(floor: Int, lift: Boolean): String =
    (if (floor == 0) "Ground" else if (floor >= 5) "5th floor +" else "Floor $floor") + if (lift) ", lift" else ", no lift"

@Composable
private fun BookedItems(move: HomeMoveDto) {
    val c = LocalMoveColors.current
    var open by rememberSaveable { mutableStateOf(false) }
    val total = move.items.sumOf { it.qty }
    MovePanel(padding = 14.dp) {
        Row(
            Modifier.fillMaxWidth().heightIn(min = 44.dp).clickable(role = Role.Button, onClickLabel = if (open) "Hide" else "Show") { open = !open },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Booked: $total item${if (total == 1) "" else "s"}", color = c.ink, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
            Text(if (open) "HIDE" else "SHOW", color = c.accent, fontSize = 13.sp, fontWeight = FontWeight.Bold, letterSpacing = 1.sp)
        }
        if (open) {
            move.items.groupBy { it.room }.forEach { (room, items) ->
                Text(room.uppercase(), color = c.muted, fontSize = 11.sp, letterSpacing = 1.4.sp, modifier = Modifier.padding(top = 10.dp))
                items.forEach { i ->
                    Text(
                        "${i.name.ifBlank { i.itemKey }} × ${i.qty}" + listOfNotNull(if (i.dismantle) "dismantle" else null, if (i.packing) "packing" else null)
                            .joinToString(prefix = " · ").takeIf { i.dismantle || i.packing }.orEmpty(),
                        color = c.ink, fontSize = 14.sp,
                    )
                }
            }
        }
    }
}

@Composable
private fun ItemRow(
    item: HomeCatalogueItemDto,
    qty: Int,
    line: io.logisticos.driver.feature.profile.presentation.SurveyLine?,
    onAdd: () -> Unit,
    onRemove: () -> Unit,
    onDismantle: () -> Unit,
    onPacking: () -> Unit,
) {
    val c = LocalMoveColors.current
    Column(Modifier.padding(horizontal = 14.dp, vertical = 10.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(item.name, color = c.ink, fontSize = 15.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
                Text("${"%.1f".format(item.volumeL / 1000.0)} m³ · ${item.weightKg} kg", color = c.muted, fontSize = 12.sp)
            }
            Stepper(Icons.Filled.Remove, "One fewer ${item.name}", enabled = qty > 0, onClick = onRemove)
            Text(
                "$qty",
                color = if (qty > 0) c.accent else c.muted, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 22.sp,
                modifier = Modifier.width(40.dp), textAlign = androidx.compose.ui.text.style.TextAlign.Center,
            )
            Stepper(Icons.Filled.Add, "One more ${item.name}", enabled = true, onClick = onAdd)
        }
        if (line != null && (item.assembly || item.packing)) {
            Row(Modifier.padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                if (item.assembly) MoveChip("Dismantle & rebuild", selected = line.dismantle, onClick = onDismantle)
                MoveChip("Special packing", selected = line.packing, onClick = onPacking)
            }
        }
    }
}

@Composable
private fun Stepper(icon: ImageVector, label: String, enabled: Boolean, onClick: () -> Unit) {
    val c = LocalMoveColors.current
    val shape = RoundedCornerShape(14.dp)
    Box(
        Modifier
            .size(48.dp)
            .clip(shape)
            .background(c.chip)
            .border(1.dp, c.hairline, shape)
            .clickable(enabled = enabled, role = Role.Button, onClickLabel = label, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, contentDescription = label, tint = if (enabled) c.ink else c.muted.copy(alpha = 0.4f))
    }
}

/**
 * Photos: condition evidence (what's already scratched or cracked) and access
 * constraints (the narrow lift, the tight stair turn). The typed inventory
 * prices the move; these are its context, and protect everyone from a false
 * damage claim.
 */
@Composable
private fun Photos(s: SurveyUiState, vm: SurveyViewModel) {
    val c = LocalMoveColors.current
    val context = androidx.compose.ui.platform.LocalContext.current
    var kind by rememberSaveable { mutableIntStateOf(0) }
    var caption by rememberSaveable { mutableStateOf("") }
    var pending by rememberSaveable { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    val camera = androidx.activity.compose.rememberLauncherForActivityResult(
        androidx.activity.result.contract.ActivityResultContracts.TakePicture(),
    ) { taken ->
        // The shutter: read here, at the physical event.
        val shutterAt = System.currentTimeMillis()
        val path = pending ?: return@rememberLauncherForActivityResult
        pending = null
        if (!taken) return@rememberLauncherForActivityResult
        val k = if (kind == 0) "condition" else "access"
        val room = s.room?.key.takeIf { kind == 0 }
        val words = caption
        scope.launch {
            kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) {
                io.logisticos.driver.core.common.ImageCompressor.compressToFile(java.io.File(path))
            }
            vm.photoTaken(path, k, room, words, shutterAt)
            caption = ""
        }
    }
    fun shoot() {
        val file = java.io.File(context.filesDir, "survey_${System.currentTimeMillis()}.jpg")
        pending = file.absolutePath
        val uri = androidx.core.content.FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", file)
        camera.launch(uri)
    }
    val permission = androidx.activity.compose.rememberLauncherForActivityResult(
        androidx.activity.result.contract.ActivityResultContracts.RequestPermission(),
    ) { granted -> if (granted) shoot() }

    MovePanel {
        MoveSegmented(listOf("Condition", "Access"), selected = kind, onSelect = { kind = it })
        Text(
            if (kind == 0) "What's already damaged — scratches, chips, cracks. In ${s.room?.name ?: "this room"}."
            else "What will slow the move — a narrow lift, a tight stair turn, a long carry.",
            color = c.muted, fontSize = 13.sp, modifier = Modifier.padding(top = 8.dp),
        )
        Spacer(Modifier.height(8.dp))
        MoveTextField(value = caption, onValueChange = { caption = it.take(200) }, label = "Caption (what it shows)")
        Spacer(Modifier.height(10.dp))
        MoveBigButton(
            label = "TAKE A PHOTO",
            filled = false,
            height = 56.dp,
            onClick = {
                val granted = androidx.core.content.ContextCompat.checkSelfPermission(context, android.Manifest.permission.CAMERA) ==
                    android.content.pm.PackageManager.PERMISSION_GRANTED
                if (granted) shoot() else permission.launch(android.Manifest.permission.CAMERA)
            },
        )
    }
    s.photos.forEach { p -> PhotoRow(p, onRetry = { p.localPath?.let(vm::retryPhoto) }) }
}

@Composable
private fun PhotoRow(p: io.logisticos.driver.feature.profile.presentation.PhotoItem, onRetry: () -> Unit) {
    val c = LocalMoveColors.current
    val thumb by produceState<androidx.compose.ui.graphics.ImageBitmap?>(null, p.localPath) {
        value = p.localPath?.let { path ->
            kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) {
                runCatching {
                    val opts = android.graphics.BitmapFactory.Options().apply { inSampleSize = 8 }
                    android.graphics.BitmapFactory.decodeFile(path, opts)?.asImageBitmap()
                }.getOrNull()
            }
        }
    }
    MovePanel(padding = 12.dp) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Box(Modifier.size(56.dp).clip(RoundedCornerShape(10.dp)).background(c.chip), contentAlignment = Alignment.Center) {
                thumb?.let { androidx.compose.foundation.Image(it, contentDescription = null, contentScale = androidx.compose.ui.layout.ContentScale.Crop, modifier = Modifier.matchParentSize()) }
                    ?: Text(if (p.kind == "access") "↕" else "◎", color = c.muted, fontSize = 20.sp)
            }
            Column(Modifier.weight(1f)) {
                Text(if (p.kind == "access") "Access" else "Condition", color = c.ink, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
                if (p.caption.isNotBlank()) Text(p.caption, color = c.muted, fontSize = 13.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
                Text(
                    when (p.status) { "done" -> "Recorded"; "uploading" -> "Uploading…"; else -> p.error ?: "Didn't upload" },
                    color = when (p.status) { "done" -> c.success; "uploading" -> c.muted; else -> c.penalty },
                    fontSize = 12.sp,
                )
            }
            if (p.status == "failed") MoveChip("Retry", selected = false, onClick = onRetry)
        }
    }
}

@Composable
private fun Extras(s: SurveyUiState, vm: SurveyViewModel, currency: String) {
    val c = LocalMoveColors.current
    s.extras.forEachIndexed { i, e ->
        MovePanel(padding = 14.dp) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(e.name, color = c.ink, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
                    Text(
                        "${if (e.kind == "material") "Material" else "Resource"} · ${e.qty} × ${money(e.unitCents, currency)} = ${money(e.qty * e.unitCents, currency)}",
                        color = c.muted, fontSize = 13.sp,
                    )
                }
                Stepper(Icons.Filled.Close, "Remove ${e.name}", enabled = true, onClick = { vm.removeExtra(i) })
            }
        }
    }
    if (s.extras.isNotEmpty()) {
        Text("Extras: ${money(extrasTotal(s.extras), currency)}", color = c.ink, fontSize = 14.sp, fontWeight = FontWeight.SemiBold)
    }

    var kind by rememberSaveable { mutableIntStateOf(0) }
    var name by rememberSaveable { mutableStateOf("") }
    var qty by rememberSaveable { mutableStateOf("1") }
    var price by rememberSaveable { mutableStateOf("") }
    var problem by remember { mutableStateOf<String?>(null) }
    MovePanel {
        MoveSegmented(listOf("Material", "Resource"), selected = kind, onSelect = { kind = it })
        Spacer(Modifier.height(10.dp))
        MoveTextField(
            value = name, onValueChange = { name = it.take(80) },
            label = if (kind == 0) "Material (e.g. wardrobe boxes)" else "Resource (e.g. extra helper, hoist)",
        )
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            MoveTextField(
                value = qty, onValueChange = { qty = it.filter(Char::isDigit).take(3) },
                label = "Qty", keyboardType = KeyboardType.Number, modifier = Modifier.weight(1f),
            )
            MoveTextField(
                value = price, onValueChange = { price = it.take(12) },
                label = "Each ($currency)", keyboardType = KeyboardType.Decimal, modifier = Modifier.weight(2f),
            )
        }
        problem?.let { Text(it, color = c.amber, fontSize = 13.sp, modifier = Modifier.padding(top = 6.dp)) }
        Spacer(Modifier.height(10.dp))
        MoveBigButton(
            label = "ADD EXTRA",
            filled = false,
            height = 56.dp,
            onClick = {
                val cents = centsFromText(price)
                problem = if (cents == null) {
                    "Enter the price for one, like 250 or 250.50."
                } else {
                    vm.addExtra(ExtraDraft(if (kind == 0) "material" else "resource", name, qty.toIntOrNull() ?: 0, cents))
                }
                if (problem == null) { name = ""; qty = "1"; price = "" }
            },
        )
    }
}
