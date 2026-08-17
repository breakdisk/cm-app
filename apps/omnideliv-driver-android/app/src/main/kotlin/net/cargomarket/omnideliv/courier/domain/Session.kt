package net.cargomarket.omnideliv.courier.domain

/**
 * Signing in, as pure rules.
 *
 * The platform authenticates a courier by phone OTP and auto-registers on first
 * verify, so there is no sign-up form to build — but the phone has to reach the
 * backend in the one shape identity stores, because it mints the account's login
 * address from those exact digits. Normalising in the UI and normalising on the
 * server would be two chances to disagree about whether `0917…` and `+63917…`
 * are the same courier.
 */

/** How many digits an OTP has. Six, and the backend will not accept another length. */
const val OTP_LENGTH = 6

/**
 * The Philippine country calling code, without a `+`.
 *
 * OmniDeliv launches in PH and identity stores bare digits. This is a constant
 * rather than a parameter because a second country is a product decision with
 * consequences beyond a prefix — a courier in another country needs a different
 * tenant, tariff and payout rail long before they need a different dial code.
 */
const val PH_COUNTRY_CODE = "63"

/**
 * Reduce whatever a courier typed to the digits identity stores, or `null` if it
 * cannot be one.
 *
 * Accepts the four forms a Filipino courier actually types — `09171234567`,
 * `9171234567`, `+639171234567`, `639171234567` — and rejects everything else
 * rather than guessing. A wrong number here is worse than a rejected one: the
 * OTP goes to somebody else's handset and the courier is told to check a phone
 * that will never ring.
 */
fun normalizePhone(raw: String): String? {
    // Strip anything a human might use as separators, including the leading +.
    val digits = raw.filter(Char::isDigit)

    val national = when {
        // 639171234567 — already national, 63 followed by a 10-digit mobile.
        digits.length == 12 && digits.startsWith(PH_COUNTRY_CODE) ->
            digits.removePrefix(PH_COUNTRY_CODE)
        // 09171234567 — the form printed on every Philippine SIM.
        digits.length == 11 && digits.startsWith("0") -> digits.removePrefix("0")
        // 9171234567 — typed without the trunk zero.
        digits.length == 10 -> digits
        else -> return null
    }

    // Every PH mobile number is ten digits beginning 9. Checked after stripping
    // so `00917…` and `63917…` fail here rather than being silently accepted.
    if (national.length != 10 || !national.startsWith("9")) return null
    return PH_COUNTRY_CODE + national
}

/**
 * Is this a plausible OTP?
 *
 * Length and digits only — whether it is *correct* is the server's to say, and
 * an app that tried to be cleverer would reject a valid code.
 */
fun isPlausibleOtp(code: String): Boolean =
    code.length == OTP_LENGTH && code.all(Char::isDigit)

/** Where a courier is in signing in. */
sealed interface SignInStep {
    /** Entering a phone number. */
    data class EnteringPhone(val input: String = "", val error: String? = null) : SignInStep

    /**
     * Waiting for the code.
     *
     * Carries the normalised phone rather than what was typed: the verify call
     * must send the same digits the send call did, and re-normalising a second
     * time is a second chance to differ.
     */
    data class EnteringCode(
        val phone: String,
        val input: String = "",
        val error: String? = null,
    ) : SignInStep

    /** A request is in flight. Carries the step to fall back to on failure. */
    data class Working(val previous: SignInStep) : SignInStep
}

/** Can the courier submit what is on screen? */
fun canSubmit(step: SignInStep): Boolean = when (step) {
    is SignInStep.EnteringPhone -> normalizePhone(step.input) != null
    is SignInStep.EnteringCode -> isPlausibleOtp(step.input)
    is SignInStep.Working -> false
}

/**
 * Not an HTTP status: the request threw and never reached the server.
 *
 * Distinct from the `else` branch so a courier in a dead zone is told their
 * signal is weak rather than being sent to re-check a number that was correct.
 */
const val NETWORK_UNREACHABLE = -1

/**
 * What the user is told when a request fails.
 *
 * Mapped from the status rather than shown raw, because a courier standing in
 * the street can act on "check the number" and cannot act on a status code. An
 * unknown status says the attempt failed and nothing more — inventing a cause
 * would send them chasing the wrong thing.
 */
fun signInError(httpStatus: Int): String = when (httpStatus) {
    NETWORK_UNREACHABLE -> "We could not reach the server. Check your signal and try again."
    400, 422 -> "That number does not look right. Check it and try again."
    401, 403 -> "That code is not right, or it has expired. Send a new one."
    404 -> "We could not find that number. Ask your dispatcher to register you."
    429 -> "Too many attempts. Wait a minute before trying again."
    in 500..599 -> "We cannot reach the server. Your signal may be weak."
    else -> "That did not work. Try again."
}
