package net.cargomarket.omnideliv.courier.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.text.KeyboardOptions
import androidx.hilt.navigation.compose.hiltViewModel
import net.cargomarket.omnideliv.courier.domain.OTP_LENGTH
import net.cargomarket.omnideliv.courier.domain.SignInStep
import net.cargomarket.omnideliv.courier.domain.canSubmit

/**
 * The only way into the app.
 *
 * Phone OTP, because the platform already authenticates couriers that way and
 * auto-registers on first verify — so there is no sign-up form here, and the
 * whole app has exactly two text inputs, both numeric.
 */
@Composable
fun SignInScreen(vm: SignInViewModel = hiltViewModel()) {
    val step by vm.step.collectAsState()

    Column(
        Modifier
            .fillMaxSize()
            .padding(horizontal = 24.dp),
        verticalArrangement = Arrangement.Center,
    ) {
        Text(
            text = "OmniDeliv",
            color = Tokens.Signal,
            fontWeight = FontWeight.Bold,
            fontSize = 30.sp,
        )
        Text(
            text = "Courier",
            color = Tokens.TextMuted,
            fontSize = 15.sp,
        )
        Spacer(Modifier.height(36.dp))

        // `Working` renders the step underneath it, disabled, so the screen does
        // not jump between layouts while a request is in flight.
        when (val s = if (step is SignInStep.Working) {
            (step as SignInStep.Working).previous
        } else {
            step
        }) {
            is SignInStep.EnteringPhone -> PhoneStep(
                step = s,
                busy = step is SignInStep.Working,
                onChange = vm::onPhoneChanged,
                onSubmit = vm::onSendCode,
            )

            is SignInStep.EnteringCode -> CodeStep(
                step = s,
                busy = step is SignInStep.Working,
                onChange = vm::onCodeChanged,
                onSubmit = vm::onVerify,
                onEditPhone = vm::onEditPhone,
            )

            // Unreachable: `Working` was unwrapped above. Rendering nothing is
            // still better than a crash if that ever stops being true.
            is SignInStep.Working -> Unit
        }
    }
}

@Composable
private fun PhoneStep(
    step: SignInStep.EnteringPhone,
    busy: Boolean,
    onChange: (String) -> Unit,
    onSubmit: () -> Unit,
) {
    Text("Your mobile number", color = Tokens.Text, fontSize = 17.sp, fontWeight = FontWeight.Bold)
    Spacer(Modifier.height(4.dp))
    Text(
        "We will text you a $OTP_LENGTH-digit code.",
        color = Tokens.TextMuted,
        fontSize = 13.sp,
    )
    Spacer(Modifier.height(14.dp))

    NumericField(
        value = step.input,
        onChange = onChange,
        placeholder = "0917 123 4567",
        keyboard = KeyboardType.Phone,
        enabled = !busy,
    )

    ErrorLine(step.error)
    Spacer(Modifier.height(18.dp))
    PrimaryButton(
        label = "Send code",
        enabled = canSubmit(step) && !busy,
        busy = busy,
        onClick = onSubmit,
    )
}

@Composable
private fun CodeStep(
    step: SignInStep.EnteringCode,
    busy: Boolean,
    onChange: (String) -> Unit,
    onSubmit: () -> Unit,
    onEditPhone: () -> Unit,
) {
    Text("Enter the code", color = Tokens.Text, fontSize = 17.sp, fontWeight = FontWeight.Bold)
    Spacer(Modifier.height(4.dp))
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(
            // Shown so a courier who mistyped can see it before waiting for a
            // code that will never arrive.
            text = "+${step.phone}",
            color = Tokens.TextMuted,
            fontSize = 13.sp,
            fontFamily = FontFamily.Monospace,
        )
        TextButton(onClick = onEditPhone, enabled = !busy) {
            Text("Change", color = Tokens.Cyan, fontSize = 13.sp)
        }
    }
    Spacer(Modifier.height(10.dp))

    NumericField(
        value = step.input,
        // Hard-capped at the known length: a seventh digit cannot be right, and
        // silently dropping it beats letting the field grow past what fits.
        onChange = { if (it.length <= OTP_LENGTH) onChange(it) },
        placeholder = "123456",
        keyboard = KeyboardType.NumberPassword,
        enabled = !busy,
        monospace = true,
    )

    ErrorLine(step.error)
    Spacer(Modifier.height(18.dp))
    PrimaryButton(
        label = "Sign in",
        enabled = canSubmit(step) && !busy,
        busy = busy,
        onClick = onSubmit,
    )
}

@Composable
private fun NumericField(
    value: String,
    onChange: (String) -> Unit,
    placeholder: String,
    keyboard: KeyboardType,
    enabled: Boolean,
    monospace: Boolean = false,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        enabled = enabled,
        singleLine = true,
        placeholder = { Text(placeholder, color = Tokens.TextMuted, fontSize = 18.sp) },
        keyboardOptions = KeyboardOptions(keyboardType = keyboard),
        textStyle = androidx.compose.ui.text.TextStyle(
            color = Tokens.Text,
            fontSize = 22.sp,
            fontFamily = if (monospace) FontFamily.Monospace else FontFamily.Default,
            // Tabular-ish: digits should not shift as they are typed.
            letterSpacing = if (monospace) 6.sp else 0.sp,
        ),
        colors = OutlinedTextFieldDefaults.colors(
            focusedBorderColor = Tokens.Signal,
            unfocusedBorderColor = Tokens.Border,
            disabledBorderColor = Tokens.Border,
            cursorColor = Tokens.Signal,
        ),
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = Tokens.MinTarget),
    )
}

@Composable
private fun ErrorLine(error: String?) {
    if (error == null) return
    Spacer(Modifier.height(8.dp))
    Text(
        text = error,
        color = Tokens.Amber,
        fontSize = 13.sp,
        lineHeight = 18.sp,
    )
}

@Composable
private fun PrimaryButton(
    label: String,
    enabled: Boolean,
    busy: Boolean,
    onClick: () -> Unit,
) {
    Button(
        onClick = onClick,
        enabled = enabled,
        colors = ButtonDefaults.buttonColors(
            containerColor = Tokens.Signal,
            contentColor = Tokens.SignalInk,
            disabledContainerColor = Tokens.SurfaceRaised,
            disabledContentColor = Tokens.TextMuted,
        ),
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = Tokens.MinTarget),
    ) {
        if (busy) {
            CircularProgressIndicator(
                color = Tokens.SignalInk,
                strokeWidth = 2.dp,
                modifier = Modifier.height(20.dp),
            )
        } else {
            Text(
                text = label,
                fontSize = 16.sp,
                fontWeight = FontWeight.Bold,
                textAlign = TextAlign.Center,
            )
        }
    }
}
