package net.cargomarket.omnideliv.courier.data

import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNotNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/**
 * The DTO layer, pinned against bodies captured from production.
 *
 * This file exists because sign-in shipped broken twice, and both times the
 * cause was here rather than in any logic the other tests cover:
 *
 *  1. the app sent `phone` where identity declares `phone_number`, so every
 *     number was refused with a message blaming the courier;
 *  2. `AuthDto` read the body directly while identity wraps auth responses in
 *     `{"data": ...}` — with `ignoreUnknownKeys` the envelope was discarded,
 *     `access_token` went missing, and a **200 with a valid token** surfaced as
 *     "we could not reach the server".
 *
 * Both were invisible to a compiler and to every existing test. The payloads
 * below are real responses, so drift on either side fails in CI rather than on
 * a courier's phone.
 */
class WireContractTest {

    /**
     * The configuration the app actually ships, not a copy of it.
     *
     * A test that built its own `Json` could pass while the app failed — which
     * is how the auth envelope survived, since `ignoreUnknownKeys` is what
     * silently discarded `data`.
     */
    private val json = CourierJson

    @Test
    fun `otp verify parses the enveloped response from identity`() {
        // Captured from POST /v1/auth/otp/verify, tokens redacted.
        val body = """
            {"data":{"access_token":"eyJ0eXAiOiJKV1Qi.redacted",
            "driver_id":"761d071d-81e8-414b-88b8-c2c02caad198",
            "expires_in":3600,"refresh_token":"rt.redacted",
            "tenant_id":"00000000-0000-0000-0000-000000000001",
            "token_type":"Bearer"}}
        """.trimIndent()

        val parsed = json.decodeFromString<AuthEnvelope>(body)

        assertTrue(parsed.data.accessToken.isNotBlank())
        // The field is driver_id. There is no user_id in this response, and
        // reading one stored null where the position route needs a courier id.
        assertEquals("761d071d-81e8-414b-88b8-c2c02caad198", parsed.data.driverId)
        assertNotNull(parsed.data.refreshToken)
    }

    /**
     * The precise shape of bug 2: without the envelope this throws, and the
     * throw was being reported to couriers as a network failure.
     */
    @Test
    fun `the envelope is required — a flat body must not silently parse`() {
        val flat = """{"access_token":"x","driver_id":"y"}"""
        val threw = runCatching { json.decodeFromString<AuthEnvelope>(flat) }.isFailure
        assertTrue(threw, "AuthEnvelope must require `data`, or the bug can return unnoticed")
    }

    /** Requests must serialise to the field names identity actually reads. */
    @Test
    fun `otp requests use phone_number, not phone`() {
        val send = json.encodeToString(
            OtpSendRequest(phone = "971581206817", tenantSlug = "demo"),
        )
        assertTrue(send.contains("\"phone_number\""), "identity ignores `phone`: $send")
        assertTrue(send.contains("\"tenant_slug\""), "omitting tenant_slug fails deserialisation")
        // Sent explicitly. kotlinx omits defaults unless encodeDefaults is on,
        // so without it `role` never left the device and sign-in depended on
        // identity happening to default the same way.
        assertTrue(send.contains("\"role\""), "role must be explicit: $send")

        val verify = json.encodeToString(
            OtpVerifyRequest(phone = "971581206817", otpCode = "123456", tenantSlug = "demo"),
        )
        assertTrue(verify.contains("\"phone_number\""), verify)
        assertTrue(verify.contains("\"otp_code\""), "the field is otp_code, not code: $verify")
    }

    /**
     * field-ops and omnideliv are **flat** — verified against production. If a
     * future change enveloped them, the app would break exactly as auth did, so
     * the distinction is pinned rather than remembered.
     */
    @Test
    fun `courier endpoints are flat, not enveloped`() {
        val offers = """
            {"offers":[{"assignment_id":"a1","product":"omnideliv","external_ref":"o1",
            "trip_cents":3500,"tip_cents":0,"cod_amount_cents":38900,
            "offer_card":{"v":1,"stops":3},"offered_at":"2026-08-18T07:00:00Z"}]}
        """.trimIndent()
        val parsed = json.decodeFromString<MyOffersDto>(offers)
        assertEquals(1, parsed.offers.size)
        assertEquals(3_500L, parsed.offers[0].tripCents)
        assertEquals(38_900L, parsed.offers[0].codAmountCents)
        assertNotNull(parsed.offers[0].offerCard)

        val earnings = """
            {"period":"2026-W33","balance_cents":-35400,
            "entries":[{"kind":"trip_earning","amount_cents":3500,
            "external_ref":"o1","at":"2026-08-17T10:00:00Z"}]}
        """.trimIndent()
        val e = json.decodeFromString<EarningsDto>(earnings)
        assertEquals(-35_400L, e.balanceCents)
        assertEquals("trip_earning", e.entries[0].kind)
    }

    /** A courier profile comes back flat too, with `id` doubling as the courier id. */
    @Test
    fun `courier register is flat`() {
        val body = """{"id":"c1","first_name":"Courier","last_name":"","status":"offline"}"""
        val c = json.decodeFromString<CourierDto>(body)
        assertEquals("c1", c.id)
        assertEquals("offline", c.status)
    }

    /**
     * The manifest is the widest contract in the app. An unknown key must be
     * ignored rather than fatal — the server adds fields over time and an app
     * that refused one would break on every backend deploy.
     */
    @Test
    fun `manifest parses and tolerates unknown fields`() {
        val body = """
            {"order_id":"o1","status":"collecting","cod_amount_cents":38900,
            "trip_cents":3500,"tip_cents":0,"something_new_from_the_server":true,
            "stops":[{"stop_ref":"v1","seq":1,"vendor_name":"Kuya's",
            "address":"142 Kalayaan Ave","lat":14.55,"lng":121.02,
            "vertical":"restaurant","prep_time_minutes":10,"picked_up":false,
            "lines":[{"qty":2,"item_name":"Chicken Adobo Bowl","modifiers":["Large"]}]}],
            "dropoff":{"stop_ref":"o1","lat":14.56,"lng":121.03,
            "customer_name":"Maria Reyes","customer_phone":"639171234567","notes":null}}
        """.trimIndent()

        val m = json.decodeFromString<ManifestDto>(body)
        assertEquals("o1", m.orderId)
        assertEquals(1, m.stops.size)
        assertEquals("Kuya's", m.stops[0].vendorName)
        assertEquals(2, m.stops[0].lines[0].qty)
        assertEquals("Maria Reyes", m.dropoff.customerName)
        // Null until the customer app grows a delivery-note field.
        assertEquals(null, m.dropoff.notes)
    }
}
