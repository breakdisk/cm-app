package io.logisticos.driver.feature.delivery.ui

import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.Phone
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.core.designsystem.*
import io.logisticos.driver.core.network.service.JobMessageItem
import io.logisticos.driver.feature.delivery.presentation.ChatViewModel

/** What a driver most often needs to say, one tap each. */
private val QUICK_REPLIES = listOf(
    "On my way",
    "I'm outside",
    "Running about 10 minutes late",
    "Where should I park?",
)

private fun timeOf(iso: String): String = runCatching {
    java.time.Instant.parse(iso)
        .atZone(java.time.ZoneId.systemDefault())
        .format(java.time.format.DateTimeFormatter.ofPattern("h:mm a"))
}.getOrDefault("")

/**
 * The thread with the customer on this job (driver design, "chat").
 *
 * It polls while it is on screen and marks itself read; there is no push yet,
 * so a closed app learns nothing until the driver opens it again.
 */
@Composable
fun ChatScreen(
    shipmentId: String,
    customerName: String,
    customerPhone: String,
    onBack: () -> Unit,
    viewModel: ChatViewModel = hiltViewModel(),
) {
    val state by viewModel.uiState.collectAsState()
    val c = LocalMoveColors.current
    val context = LocalContext.current
    val listState = rememberLazyListState()
    var draft by rememberSaveable { mutableStateOf("") }

    DisposableEffect(shipmentId) {
        viewModel.startPolling(shipmentId)
        onDispose { viewModel.stopPolling() }
    }

    LaunchedEffect(state.messages.size) {
        if (state.messages.isNotEmpty()) listState.animateScrollToItem(state.messages.lastIndex)
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(c.ground)
            .imePadding()
    ) {
        MoveScreenHeader(
            label = "Message · this stop",
            title = customerName.ifBlank { "The customer" },
            onBack = onBack,
            actions = {
                if (customerPhone.isNotBlank()) {
                    MoveSquareButton(Icons.Filled.Phone, "Call $customerPhone", {
                        context.startActivity(Intent(Intent.ACTION_DIAL, Uri.parse("tel:$customerPhone")))
                    })
                }
            },
        )

        LazyColumn(
            state = listState,
            modifier = Modifier.weight(1f).fillMaxWidth(),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 12.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item(key = "context") {
                Text(
                    "KEPT WITH THIS JOB",
                    color = c.muted,
                    fontSize = 12.sp,
                    letterSpacing = 2.4.sp,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.fillMaxWidth(),
                )
            }

            if (state.loading && state.messages.isEmpty()) {
                item(key = "loading") {
                    Box(Modifier.fillMaxWidth().padding(top = 24.dp), contentAlignment = Alignment.Center) {
                        CircularProgressIndicator(color = c.accent)
                    }
                }
            }

            if (!state.loading && state.messages.isEmpty() && state.error == null) {
                item(key = "empty") {
                    MoveNotice(
                        title = "Nothing yet",
                        body = "Tell them you're close, or ask where to park. They see it in their app.",
                        tone = MoveTone.Neutral,
                    )
                }
            }

            items(state.messages, key = { it.id }) { message -> Bubble(message) }

            state.error?.let { error ->
                item(key = "error") {
                    Text(error, color = c.penalty, fontSize = 14.sp, modifier = Modifier.fillMaxWidth())
                }
            }
        }

        if (state.canSend) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .horizontalScroll(rememberScrollState())
                    .padding(horizontal = 16.dp),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                QUICK_REPLIES.forEach { reply ->
                    MoveChip(
                        label = reply,
                        selected = false,
                        onClick = { viewModel.send(shipmentId, reply) },
                    )
                }
            }

            Row(
                modifier = Modifier.fillMaxWidth().padding(16.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
                verticalAlignment = Alignment.Bottom,
            ) {
                MoveTextField(
                    value = draft,
                    onValueChange = { draft = it },
                    label = "Message",
                    modifier = Modifier.weight(1f),
                    singleLine = false,
                )
                MoveSquareButton(
                    icon = Icons.AutoMirrored.Filled.Send,
                    label = "Send",
                    onClick = {
                        viewModel.send(shipmentId, draft)
                        draft = ""
                    },
                    active = draft.isNotBlank(),
                    modifier = Modifier.padding(bottom = 4.dp),
                )
            }
        } else {
            MoveNotice(
                title = "This job is done",
                body = "The thread stays here to read. Anything new goes through support.",
                tone = MoveTone.Neutral,
                modifier = Modifier.padding(16.dp),
            )
        }

        Spacer(Modifier.navigationBarsPadding())
    }
}

@Composable
private fun Bubble(message: JobMessageItem) {
    val c = LocalMoveColors.current
    val mine = message.senderRole == "driver"
    val shape = if (mine) {
        RoundedCornerShape(topStart = 18.dp, topEnd = 18.dp, bottomStart = 18.dp, bottomEnd = 5.dp)
    } else {
        RoundedCornerShape(topStart = 18.dp, topEnd = 18.dp, bottomStart = 5.dp, bottomEnd = 18.dp)
    }
    Column(
        modifier = Modifier.fillMaxWidth(),
        horizontalAlignment = if (mine) Alignment.End else Alignment.Start,
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth(0.84f)
                .wrapContentWidth(if (mine) Alignment.End else Alignment.Start)
                .clip(shape)
                .background(if (mine) c.accent else c.chip)
                .border(1.dp, if (mine) c.accent else c.hairline, shape)
                .padding(horizontal = 16.dp, vertical = 13.dp)
        ) {
            Text(
                message.body,
                color = if (mine) c.accentInk else c.ink,
                fontSize = 17.sp,
                lineHeight = 24.sp,
            )
        }
        Text(timeOf(message.createdAt), color = c.muted, fontSize = 12.sp, modifier = Modifier.padding(top = 5.dp))
    }
}
