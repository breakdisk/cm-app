package net.cargomarket.omnideliv.courier.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.RadioButton
import androidx.compose.material3.RadioButtonDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import net.cargomarket.omnideliv.courier.domain.EXCEPTION_TEXT_MAX
import net.cargomarket.omnideliv.courier.domain.ExceptionDraft
import net.cargomarket.omnideliv.courier.domain.ExceptionReason

/**
 * "Report an issue": the reason sheet behind the manifest's button.
 *
 * A full screen in place of the manifest, the same way [ProofScreen] is, rather
 * than a sheet over it. The courier is choosing between six things with one
 * hand, often outside, and a full-height list gives every row a full-width
 * target.
 *
 * Submitting records the report and returns to the manifest. It does not end
 * the job. Ops decides what happens next (decision D2), so the courier keeps
 * the goods and the assignment until they hear back.
 */
@Composable
fun IssueScreen(
    onSubmit: (ExceptionDraft) -> Unit,
    onCancel: () -> Unit,
) {
    // Saveable so a rotation or a trip to the dialler mid-report keeps what the
    // courier already typed.
    var reason by rememberSaveable { mutableStateOf<ExceptionReason?>(null) }
    var note by rememberSaveable { mutableStateOf("") }
    var goods by rememberSaveable { mutableStateOf("") }

    BackHandler(onBack = onCancel)

    Column(
        Modifier
            .fillMaxSize()
            .background(Tokens.Base)
            .imePadding()
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
    ) {
        Text("What's stopping this delivery?", color = Tokens.Text, fontSize = 20.sp, fontWeight = FontWeight.Bold)
        Spacer(Modifier.height(4.dp))
        Text(
            "Ops will see this straight away. Keep the order with you until they get back to you.",
            color = Tokens.TextMuted,
            fontSize = 13.sp,
        )
        Spacer(Modifier.height(16.dp))

        Column(Modifier.selectableGroup()) {
            ExceptionReason.entries.forEach { r ->
                ReasonRow(r, selected = r == reason, onSelect = { reason = r })
                Spacer(Modifier.height(8.dp))
            }
        }

        val chosen = reason
        if (chosen != null && chosen.asksForGoods) {
            Spacer(Modifier.height(8.dp))
            IssueField(
                value = goods,
                onValueChange = { goods = it.take(EXCEPTION_TEXT_MAX) },
                label = "Where are the goods now? (optional)",
                placeholder = "e.g. still with me, left with the guard",
            )
        }

        Spacer(Modifier.height(12.dp))
        IssueField(
            value = note,
            onValueChange = { note = it.take(EXCEPTION_TEXT_MAX) },
            label = "Anything ops should know? (optional)",
            placeholder = "e.g. called twice, gate locked",
        )

        Spacer(Modifier.height(20.dp))
        Button(
            onClick = { chosen?.let { onSubmit(ExceptionDraft.of(it, note, goods)) } },
            enabled = chosen != null,
            modifier = Modifier.fillMaxWidth().heightIn(min = Tokens.MinTarget),
            shape = RoundedCornerShape(12.dp),
            colors = ButtonDefaults.buttonColors(
                containerColor = Tokens.Amber,
                contentColor = Tokens.Base,
                disabledContainerColor = Tokens.SurfaceRaised,
                disabledContentColor = Tokens.TextMuted,
            ),
        ) {
            Text("Send report", fontSize = 16.sp, fontWeight = FontWeight.Bold)
        }
        Spacer(Modifier.height(8.dp))
        Button(
            onClick = onCancel,
            modifier = Modifier.fillMaxWidth().heightIn(min = 44.dp),
            shape = RoundedCornerShape(12.dp),
            colors = ButtonDefaults.buttonColors(
                containerColor = Tokens.SurfaceRaised,
                contentColor = Tokens.Text,
            ),
        ) {
            Text("Back to the order", fontSize = 13.sp)
        }
    }
}

@Composable
private fun ReasonRow(reason: ExceptionReason, selected: Boolean, onSelect: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = Tokens.MinTarget)
            .background(Tokens.Surface, RoundedCornerShape(12.dp))
            .border(1.dp, if (selected) Tokens.Amber else Tokens.Border, RoundedCornerShape(12.dp))
            .selectable(selected = selected, onClick = onSelect, role = Role.RadioButton)
            .padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // The row owns the click, so the radio itself must not take a second one.
        RadioButton(
            selected = selected,
            onClick = null,
            colors = RadioButtonDefaults.colors(selectedColor = Tokens.Amber, unselectedColor = Tokens.TextMuted),
        )
        Spacer(Modifier.width(10.dp))
        Text(reason.label, color = Tokens.Text, fontSize = 15.sp)
    }
}

@Composable
private fun IssueField(
    value: String,
    onValueChange: (String) -> Unit,
    label: String,
    placeholder: String,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        label = { Text(label) },
        placeholder = { Text(placeholder, color = Tokens.TextMuted) },
        minLines = 2,
        modifier = Modifier.fillMaxWidth(),
        colors = OutlinedTextFieldDefaults.colors(
            focusedTextColor = Tokens.Text,
            unfocusedTextColor = Tokens.Text,
            focusedBorderColor = Tokens.Amber,
            unfocusedBorderColor = Tokens.Border,
            focusedLabelColor = Tokens.Amber,
            unfocusedLabelColor = Tokens.TextMuted,
            cursorColor = Tokens.Amber,
        ),
    )
}
