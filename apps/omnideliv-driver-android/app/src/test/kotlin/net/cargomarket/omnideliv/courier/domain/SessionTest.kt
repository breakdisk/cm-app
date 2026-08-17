package net.cargomarket.omnideliv.courier.domain

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

class SessionTest {

    /**
     * The four shapes a Filipino courier actually types. All are the same
     * person, and identity stores one of them — so all four must reduce to it or
     * the same courier gets two accounts.
     */
    @Test
    fun `every form a courier types reduces to the same digits`() {
        val expected = "639171234567"
        for (typed in listOf(
            "09171234567",
            "9171234567",
            "+639171234567",
            "639171234567",
            "0917 123 4567",
            "+63 917-123-4567",
            "(0917) 123 4567",
        )) {
            assertEquals(expected, normalizePhone(typed), "failed for `$typed`")
        }
    }

    /**
     * A wrong number is worse than a rejected one: the OTP reaches somebody
     * else's handset and the courier waits for a phone that will never ring. So
     * anything ambiguous is refused rather than guessed at.
     */
    @Test
    fun `anything that cannot be a PH mobile is refused`() {
        for (bad in listOf(
            "",                 // nothing
            "0917123456",       // one digit short
            "091712345678",     // one digit long
            "08171234567",      // landline trunk, not a mobile
            "8171234567",       // does not start 9
            "639871234567890",  // far too long
            "63917123456",      // 63 + only 9 digits
            "abcdefghijk",      // no digits at all
            "00639171234567",   // double trunk prefix
            "1234567890",       // ten digits, wrong first digit
        )) {
            assertNull(normalizePhone(bad), "should have refused `$bad`")
        }
    }

    /** Every accepted number is exactly 63 followed by ten digits starting 9. */
    @Test
    fun `an accepted number is always twelve digits starting 639`() {
        val n = normalizePhone("0917 123 4567")!!
        assertEquals(12, n.length)
        assertTrue(n.startsWith("639"))
        assertTrue(n.all(Char::isDigit))
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
        assertFalse(canSubmit(SignInStep.EnteringPhone("0917")))
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
