package net.cargomarket.omnideliv.courier.domain

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNotNull
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

class SessionTest {

    /**
     * The shapes a courier types when they have not named a country. All are the
     * same person and identity stores one of them, so all must reduce to it or
     * the same courier ends up with two accounts.
     */
    @Test
    fun `national forms reduce to the default country`() {
        val expected = "639171234567"
        for (typed in listOf(
            "09171234567",
            "9171234567",
            "639171234567",
            "0917 123 4567",
            "(0917) 123 4567",
        )) {
            assertEquals(expected, normalizePhone(typed, "63"), "failed for `$typed`")
        }
    }

    /**
     * The bug this test exists for. The validator required exactly ten national
     * digits beginning `9` — a Philippine mobile pattern — so every UAE prefix
     * was refused and a courier in Dubai could not sign in at all.
     */
    @Test
    fun `gulf prefixes are accepted, not just philippine ones`() {
        assertEquals("971551234567", normalizePhone("0551234567", "971"), "du 055")
        assertEquals("971581234567", normalizePhone("0581234567", "971"), "du 058")
        assertEquals("971501234567", normalizePhone("0501234567", "971"), "Etisalat 050")
        assertEquals("971521234567", normalizePhone("052 123 4567", "971"), "Etisalat 052")
    }

    /**
     * The worst part of the old rule: a courier who typed their number
     * *completely correctly*, country code and all, was still refused. An
     * explicit `+` names the country and must win over any default.
     */
    @Test
    fun `an explicit country code wins over the build default`() {
        assertEquals("971551234567", normalizePhone("+971 55 123 4567", "63"))
        assertEquals("639171234567", normalizePhone("+63 917 123 4567", "971"))
        assertEquals("14155552671", normalizePhone("+1 415 555 2671", "63"))
        assertEquals("442071838750", normalizePhone("+44 20 7183 8750", "63"))
    }

    /** `00` is how the same thing is written across the Gulf and much of Europe. */
    @Test
    fun `a double-zero prefix is international too`() {
        assertEquals("971551234567", normalizePhone("00971551234567", "63"))
        assertEquals("639171234567", normalizePhone("0063 917 123 4567", "971"))
    }

    /**
     * Still refuses rather than guesses. A wrong number sends the OTP to
     * somebody else's handset while the courier waits for a phone that will
     * never ring.
     */
    @Test
    fun `anything that cannot be a phone number is refused`() {
        for (bad in listOf(
            "",              // nothing
            "abcdef",        // no digits
            "12345",         // too short even with a country code
            "+1234567",      // explicitly international and still too short
            "0",             // a lone trunk zero
            "+9715512345678901234", // past the E.164 ceiling
        )) {
            assertNull(normalizePhone(bad, "971"), "should have refused `$bad`")
        }
    }

    /** E.164 permits at most fifteen digits, country code included. */
    @Test
    fun `the e164 bounds are enforced at both ends`() {
        assertNotNull(normalizePhone("+" + "1".repeat(E164_MIN_DIGITS), "63"))
        assertNull(normalizePhone("+" + "1".repeat(E164_MIN_DIGITS - 1), "63"))
        assertNotNull(normalizePhone("+" + "1".repeat(E164_MAX_DIGITS), "63"))
        assertNull(normalizePhone("+" + "1".repeat(E164_MAX_DIGITS + 1), "63"))
    }

    /** Never carries a leading `+` or a trunk zero — identity stores bare digits. */
    @Test
    fun `output is always bare digits`() {
        for (typed in listOf("+971 55 123 4567", "0551234567", "00971551234567")) {
            val n = normalizePhone(typed, "971")!!
            assertTrue(n.all(Char::isDigit), "`$typed` produced `$n`")
            assertTrue(n.startsWith("971"))
        }
    }

    /**
     * The button must agree with the request. If these disagreed it would enable
     * on a number the send call then refuses, or stay dead on one that works.
     */
    /**
     * The default is load bearing: the same digits become a different courier
     * depending on which country is assumed.
     *
     * Note what is *not* claimed here. Without a table of the world's numbering
     * plans, `0551234567` under `63` is still a structurally valid E.164 — it is
     * simply the wrong person. That is why the default is build config rather
     * than something this function could validate its way out of.
     */
    @Test
    fun `the default country changes who the number belongs to`() {
        val typed = "0551234567"
        assertEquals("971551234567", normalizePhone(typed, "971"))
        assertEquals("63551234567", normalizePhone(typed, "63"))
        assertTrue(canSubmit(SignInStep.EnteringPhone(typed), "971"))
    }

    @Test
    fun `an otp is six digits and nothing else`() {
        assertTrue(isPlausibleOtp("123456"))
        assertFalse(isPlausibleOtp("12345"), "five digits")
        assertFalse(isPlausibleOtp("1234567"), "seven digits")
        assertFalse(isPlausibleOtp("12345a"), "not all digits")
        assertFalse(isPlausibleOtp(""), "empty")
        assertFalse(isPlausibleOtp("12 34 56"), "spaces are not digits")
    }

    /**
     * Plausible is not correct. Whether a code is right is the server's to say,
     * and an app trying to be cleverer would reject a valid one.
     */
    @Test
    fun `the leading zero of an otp is preserved`() {
        assertTrue(isPlausibleOtp("000123"))
    }

    @Test
    fun `submit is blocked until the input could work`() {
        assertFalse(canSubmit(SignInStep.EnteringPhone("091")))
        assertTrue(canSubmit(SignInStep.EnteringPhone("09171234567")))

        assertFalse(canSubmit(SignInStep.EnteringCode("639171234567", "123")))
        assertTrue(canSubmit(SignInStep.EnteringCode("639171234567", "123456")))
    }

    /**
     * A second tap while a request is in flight would send a second OTP, and the
     * first code the courier reads would then be the stale one.
     */
    @Test
    fun `submit is blocked while a request is in flight`() {
        val working = SignInStep.Working(SignInStep.EnteringPhone("09171234567"))
        assertFalse(canSubmit(working))
    }

    /**
     * The code step carries the *normalised* phone, not what was typed. Verify
     * has to send the digits that send sent; re-normalising later is a second
     * chance to differ.
     */
    @Test
    fun `the code step carries normalised digits`() {
        val typed = "0917 123 4567"
        val step = SignInStep.EnteringCode(phone = normalizePhone(typed)!!)
        assertEquals("639171234567", step.phone)
    }

    /**
     * A courier standing in the street can act on "check the number" and cannot
     * act on a status code. Each message has to name something they can do.
     */
    @Test
    fun `every error message is actionable`() {
        val cases = mapOf(
            400 to "number",
            401 to "code",
            404 to "dispatcher",
            429 to "Wait",
            500 to "signal",
        )
        for ((status, cue) in cases) {
            val msg = signInError(status)
            assertTrue(msg.contains(cue, ignoreCase = true), "status $status said: $msg")
        }
    }

    /**
     * An unknown status must not invent a cause — that sends the courier
     * chasing the wrong thing.
     */
    @Test
    fun `an unknown status says only that it failed`() {
        val msg = signInError(418)
        assertEquals("That did not work. Try again.", msg)
        for (invented in listOf("number", "code", "signal", "dispatcher")) {
            assertFalse(msg.contains(invented, ignoreCase = true))
        }
    }

    /**
     * A request that never left the device is not a rejected number. Collapsing
     * the two would send a courier in a lift lobby to re-check digits that were
     * right.
     */
    @Test
    fun `a request that never reached the server blames the signal`() {
        val msg = signInError(NETWORK_UNREACHABLE)
        assertTrue(msg.contains("signal", ignoreCase = true))
        assertFalse(msg.contains("number", ignoreCase = true))
        assertFalse(msg.contains("code", ignoreCase = true))
        assertEquals(-1, NETWORK_UNREACHABLE, "must not collide with any HTTP status")
    }

    @Test
    fun `the declared otp length matches what the check enforces`() {
        assertEquals(6, OTP_LENGTH)
        assertTrue(isPlausibleOtp("1".repeat(OTP_LENGTH)))
        assertFalse(isPlausibleOtp("1".repeat(OTP_LENGTH + 1)))
    }
}
