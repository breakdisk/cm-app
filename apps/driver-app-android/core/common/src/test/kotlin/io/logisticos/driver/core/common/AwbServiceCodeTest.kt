package io.logisticos.driver.core.common

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Test

/**
 * The POP payload's `service_code` decides which billing track a pickup lands in
 * — `balikbayan` debits the driver ledger at the doorstep (Track A), anything
 * else defers to weight-based surcharging at hub weigh-in (Track B). Getting it
 * wrong misclassifies real money, so the parser is pinned here.
 */
class AwbServiceCodeTest {

    @Test
    fun `parses each service character from a master AWB`() {
        assertEquals(AwbServiceCode.STANDARD, AwbServiceCode.fromAwb("CM-PH1-S0001234X"))
        assertEquals(AwbServiceCode.EXPRESS, AwbServiceCode.fromAwb("CM-PH1-E0001234X"))
        assertEquals(AwbServiceCode.SAME_DAY, AwbServiceCode.fromAwb("CM-PH1-D0001234X"))
        assertEquals(AwbServiceCode.BALIKBAYAN, AwbServiceCode.fromAwb("CM-PH1-B0009012Z"))
        assertEquals(AwbServiceCode.INTERNATIONAL, AwbServiceCode.fromAwb("CM-PH1-N0001234X"))
    }

    @Test
    fun `parses a child piece label`() {
        // Piece labels append -{PPP}; the service char sits in the same position.
        assertEquals(AwbServiceCode.BALIKBAYAN, AwbServiceCode.fromAwb("CM-PH1-B0009012Z-002"))
    }

    @Test
    fun `is case insensitive and tolerates surrounding whitespace`() {
        assertEquals(AwbServiceCode.BALIKBAYAN, AwbServiceCode.fromAwb("  cm-ph1-b0009012z  "))
    }

    @Test
    fun `works across tenant codes`() {
        assertEquals(AwbServiceCode.BALIKBAYAN, AwbServiceCode.fromAwb("CM-SG2-B0009012Z"))
        assertEquals(AwbServiceCode.STANDARD, AwbServiceCode.fromAwb("CM-AE3-S0001234X"))
    }

    @Test
    fun `returns null for tracking numbers that are not CargoMarket AWBs`() {
        // A pickup must never fail because its label is legacy or carrier-issued.
        assertNull(AwbServiceCode.fromAwb(null))
        assertNull(AwbServiceCode.fromAwb(""))
        assertNull(AwbServiceCode.fromAwb("1Z999AA10123456784"))     // UPS
        assertNull(AwbServiceCode.fromAwb("CM-PH1"))                  // truncated
        assertNull(AwbServiceCode.fromAwb("XX-PH1-S0001234X"))        // wrong prefix
        assertNull(AwbServiceCode.fromAwb("CM-PHIL-S0001234X"))       // tenant not 3 chars
        assertNull(AwbServiceCode.fromAwb("CM-PH1-S001X"))            // serial too short
        assertNull(AwbServiceCode.fromAwb("CM-PH1-Q0001234X"))        // unknown service char
    }

    @Test
    fun `wireValue matches the backend ServiceCode as_str values`() {
        // Mirrors ServiceCode::as_str() in libs/types/src/awb.rs.
        assertEquals("standard", AwbServiceCode.STANDARD.wireValue)
        assertEquals("express", AwbServiceCode.EXPRESS.wireValue)
        assertEquals("same_day", AwbServiceCode.SAME_DAY.wireValue)
        assertEquals("balikbayan", AwbServiceCode.BALIKBAYAN.wireValue)
        assertEquals("international", AwbServiceCode.INTERNATIONAL.wireValue)
    }

    @Test
    fun `wireValueFor sends balikbayan for a Balikbayan AWB`() {
        // The regression this guards: every POP used to ship service_code
        // "standard" because no caller ever set it, so Track A pickups were
        // billed as Track B.
        assertEquals("balikbayan", AwbServiceCode.wireValueFor("CM-PH1-B0009012Z"))
    }

    @Test
    fun `wireValueFor falls back to standard for unrecognised input`() {
        // Matches the pod service's #[serde(default = "default_standard")].
        assertEquals("standard", AwbServiceCode.wireValueFor(null))
        assertEquals("standard", AwbServiceCode.wireValueFor("LEGACY-123"))
        assertEquals(AwbServiceCode.DEFAULT_WIRE_VALUE, AwbServiceCode.wireValueFor(null))
    }
}
