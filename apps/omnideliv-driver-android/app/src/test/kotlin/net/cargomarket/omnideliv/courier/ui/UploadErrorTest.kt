package net.cargomarket.omnideliv.courier.ui

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * What a courier is told when an upload fails, and whether they are offered a
 * retry.
 *
 * Both are pure functions of the status code precisely so they can be pinned
 * here — the alternative is discovering on a doorstep that the app renders
 * "HTTP 413" at somebody.
 */
class UploadErrorTest {

    private val codes = listOf(400, 401, 403, 404, 408, 413, 422, 429, 500, 502, 503, 599, 418)

    /** A status code tells a courier nothing about what to do next. */
    @Test
    fun no_message_shows_a_raw_status_code() {
        codes.forEach { code ->
            val msg = uploadError(code)
            assertFalse("$code leaked its number: $msg", msg.contains(code.toString()))
            assertFalse("$code said HTTP: $msg", msg.contains("HTTP", ignoreCase = true))
        }
    }

    /** Every message is something a person can act on, not a category name. */
    @Test
    fun every_message_is_a_sentence() {
        codes.forEach { code ->
            val msg = uploadError(code)
            assertTrue("$code was empty", msg.isNotBlank())
            assertTrue("$code did not end a sentence: $msg", msg.trimEnd().endsWith("."))
        }
    }

    @Test
    fun an_expired_session_tells_them_to_sign_in_again() {
        listOf(401, 403).forEach {
            assertTrue(uploadError(it).contains("sign", ignoreCase = true))
        }
    }

    /**
     * Retrying identical bytes against a refusal on the merits would fail
     * identically, and a button that does nothing twice teaches couriers to
     * distrust every button in the app.
     */
    @Test
    fun only_failures_that_could_plausibly_clear_offer_a_retry() {
        listOf(500, 502, 503, 599, 408, 429).forEach {
            assertTrue("$it should be retryable", retryable(it))
        }
        listOf(400, 401, 403, 404, 413, 422).forEach {
            assertFalse("$it must not offer a retry", retryable(it))
        }
    }

    /** A 5xx is the server's problem, and the same bytes are worth resending. */
    @Test
    fun the_whole_5xx_range_is_retryable() {
        for (code in 500..599) {
            assertTrue("$code", retryable(code))
        }
    }
}
