package net.cargomarket.omnideliv.courier.ui

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import net.cargomarket.omnideliv.courier.BuildConfig
import net.cargomarket.omnideliv.courier.data.CourierApi
import net.cargomarket.omnideliv.courier.data.OtpSendRequest
import net.cargomarket.omnideliv.courier.data.OtpVerifyRequest
import net.cargomarket.omnideliv.courier.data.RegisterCourierRequest
import net.cargomarket.omnideliv.courier.data.TokenStore
import net.cargomarket.omnideliv.courier.domain.NETWORK_UNREACHABLE
import net.cargomarket.omnideliv.courier.domain.SignInStep
import net.cargomarket.omnideliv.courier.domain.normalizePhone
import net.cargomarket.omnideliv.courier.domain.signInError
import javax.inject.Inject

@HiltViewModel
class SignInViewModel @Inject constructor(
    private val api: CourierApi,
    private val tokens: TokenStore,
) : ViewModel() {

    private val _step = MutableStateFlow<SignInStep>(SignInStep.EnteringPhone())
    val step: StateFlow<SignInStep> = _step.asStateFlow()

    fun onPhoneChanged(input: String) {
        val current = _step.value
        if (current is SignInStep.EnteringPhone) {
            // Clearing the error on edit rather than on the next submit: leaving
            // it up while the courier fixes the number reads as though the new
            // number is also rejected.
            _step.value = current.copy(input = input, error = null)
        }
    }

    fun onCodeChanged(input: String) {
        val current = _step.value
        if (current is SignInStep.EnteringCode) {
            _step.value = current.copy(input = input, error = null)
        }
    }

    /** Back to the number, keeping it filled in so it need not be retyped. */
    fun onEditPhone() {
        val current = _step.value
        if (current is SignInStep.EnteringCode) {
            _step.value = SignInStep.EnteringPhone(input = current.phone)
        }
    }

    fun onSendCode() {
        val current = _step.value as? SignInStep.EnteringPhone ?: return
        val phone = normalizePhone(current.input) ?: return
        _step.value = SignInStep.Working(current)

        viewModelScope.launch {
            val outcome = runCatching {
                api.sendOtp(OtpSendRequest(phone = phone, tenantSlug = BuildConfig.TENANT_SLUG))
            }
            _step.value = outcome.fold(
                onSuccess = { res ->
                    if (res.isSuccessful) {
                        SignInStep.EnteringCode(phone = phone)
                    } else {
                        current.copy(error = signInError(res.code()))
                    }
                },
                // A thrown request never reached the server. Reported as a
                // connectivity problem rather than a rejected number, because
                // telling a courier to check a correct number wastes their time.
                onFailure = { current.copy(error = signInError(NETWORK_UNREACHABLE)) },
            )
        }
    }

    fun onVerify() {
        val current = _step.value as? SignInStep.EnteringCode ?: return
        _step.value = SignInStep.Working(current)

        viewModelScope.launch {
            val outcome = runCatching {
                api.verifyOtp(
                    OtpVerifyRequest(
                        phone = current.phone,
                        otpCode = current.input,
                        tenantSlug = BuildConfig.TENANT_SLUG,
                    ),
                )
            }

            outcome.fold(
                onSuccess = { res ->
                    val body = res.body()
                    if (res.isSuccessful && body != null) {
                        // Token first, and it has to be: `registerCourier` is
                        // an authenticated route, so the interceptor needs a
                        // token before it can succeed.
                        //
                        // That ordering leaves a real window. Verify
                        // auto-creates the identity *user* but not the courier
                        // *profile*, so between these two lines the app holds a
                        // valid session for somebody field-ops has never heard
                        // of, and every job call would 404. `ensureCourierProfile`
                        // is idempotent and runs on every sign-in precisely so
                        // that state is self-healing — signing in again fixes it,
                        // with no support path required.
                        tokens.signIn(body.accessToken, body.userId)
                        ensureCourierProfile(current.phone)
                    } else {
                        _step.value = current.copy(error = signInError(res.code()))
                    }
                },
                onFailure = { _step.value = current.copy(error = signInError(NETWORK_UNREACHABLE)) },
            )
        }
    }

    /**
     * Make sure a courier profile exists for this user.
     *
     * Idempotent on the server, so calling it on every sign-in is cheap and
     * means a courier whose profile was never created is fixed by signing in
     * again. Failure is deliberately **not** fatal: the session is already
     * valid, and blocking sign-in on this would strand a courier whose profile
     * already exists behind a transient error.
     */
    private suspend fun ensureCourierProfile(phone: String) {
        runCatching {
            val res = api.registerCourier(
                // No name collected: the OTP path has none to give, and a
                // fabricated one would end up on a customer's screen.
                RegisterCourierRequest(firstName = "Courier", lastName = "", phone = phone),
            )
            if (res.isSuccessful) res.body()?.id?.let { tokens.courierId = it }
        }
        // The gate is already open — TokenStore.signIn moved it. Nothing to do
        // here on success or failure.
    }
}
