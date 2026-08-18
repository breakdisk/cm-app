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
 * The country assumed when a courier types a national number.
 *
 * Overridden per build (`DEFAULT_COUNTRY_CODE` in `build.gradle.kts`) because it
 * belongs to the tenant, not to this file: `cargomarket-ph` is 63, a Gulf tenant
 * is 971. Only ever a *default* — a courier who types `+971…` gets 971 on any
 * build, which is what makes one APK usable outside its launch market.
 */
const val DEFAULT_COUNTRY_CODE = "63"

/**
 * Shortest and longest an E.164 subscriber number can be, country code included.
 *
 * From the ITU recommendation. Checked instead of a per-country pattern because
 * this app has no business owning a table of the world's numbering plans — and a
 * pattern that is merely out of date rejects real couriers, which is the failure
 * that matters here.
 */
const val E164_MIN_DIGITS = 8
const val E164_MAX_DIGITS = 15

/**
 * Shortest national part accepted when a country code has to be supplied.
 *
 * Without this, a three-digit country code plus five typed digits clears the
 * E.164 floor and `12345` is accepted as a phone number. Six is below every real
 * mobile plan and above obvious nonsense — deliberately loose, because refusing
 * a real courier is the costlier mistake.
 */
const val MIN_NATIONAL_DIGITS = 6

/**
 * Reduce whatever a courier typed to the digits identity stores, or `null` if it
 * cannot be a phone number.
 *
 * Two shapes are accepted, and the distinction is the whole design:
 *
 * - **Explicitly international** — `+971 55 123 4567`, or `00971…`. The courier
 *   has stated their country, so it is used, whatever [defaultCountryCode] says.
 * - **National** — `0551234567`, or `551234567`. No country was stated, so
 *   [defaultCountryCode] supplies one.
 *
 * The previous version was Philippines-only: it required exactly ten national
 * digits beginning `9`, which is a PH mobile pattern. That rejected every UAE
 * prefix (`050`, `055`, `058`), every other country, **and full `+971…` E.164
 * input** — a courier could not sign in even by typing their number completely
 * correctly. A hardcoded numbering plan is a launch decision leaking into an
 * identity check.
 *
 * Still refuses rather than guesses when the result cannot be a phone number.
 * A wrong number sends the OTP to somebody else's handset while the courier
 * waits for a phone that will never ring.
 */
fun normalizePhone(
    raw: String,
    defaultCountryCode: String = DEFAULT_COUNTRY_CODE,
): String? {
    val trimmed = raw.trim()
    val digits = trimmed.filter(Char::isDigit)
    if (digits.isEmpty()) return null

    val e164 = when {
        // "+971…" — the courier named their country.
        trimmed.startsWith("+") -> digits

        // "00971…" — the other way to write it, common across the Gulf and EU.
        digits.startsWith("00") -> digits.removePrefix("00")

        // "0551234567" — national with a trunk zero, which is never part of the
        // international form and is always dropped.
        digits.startsWith("0") -> {
            val national = digits.trimStart('0')
            if (national.length < MIN_NATIONAL_DIGITS) return null
            defaultCountryCode + national
        }

        // Already carries the default country code, typed without a plus. Length
        // is checked so a national number that merely starts with those digits
        // is not mistaken for one that includes them.
        digits.startsWith(defaultCountryCode) &&
            digits.length >= defaultCountryCode.length + E164_MIN_DIGITS - 2 -> digits

        // "551234567" — national, no trunk zero.
        else -> {
            if (digits.length < MIN_NATIONAL_DIGITS) return null
            defaultCountryCode + digits
        }
    }

    return if (e164.length in E164_MIN_DIGITS..E164_MAX_DIGITS) e164 else null
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

/**
 * Can the courier submit what is on screen?
 *
 * Takes the same country default the send call will use. If these two disagreed
 * the button would enable on a number the request then refuses, or stay dead on
 * one that would have worked.
 */
fun canSubmit(
    step: SignInStep,
    defaultCountryCode: String = DEFAULT_COUNTRY_CODE,
): Boolean = when (step) {
    is SignInStep.EnteringPhone -> normalizePhone(step.input, defaultCountryCode) != null
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
