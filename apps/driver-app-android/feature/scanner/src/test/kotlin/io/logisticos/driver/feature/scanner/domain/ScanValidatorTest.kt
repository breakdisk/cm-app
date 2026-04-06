package io.logisticos.driver.feature.scanner.domain

import org.junit.jupiter.api.Test
import org.junit.jupiter.api.Assertions.*

class ScanValidatorTest {

    @Test
    fun `valid awb returns Match when in expected list`() {
        val result = ScanValidator.validate(
            scannedAwb = "LS-ABC123",
            expectedAwbs = listOf("LS-ABC123", "LS-DEF456"),
            alreadyScanned = emptyList()
        )
        assertTrue(result is ScanValidationResult.Match)
    }

    @Test
    fun `unknown awb returns Unexpected`() {
        val result = ScanValidator.validate(
            scannedAwb = "LS-UNKNOWN",
            expectedAwbs = listOf("LS-ABC123"),
            alreadyScanned = emptyList()
        )
        assertTrue(result is ScanValidationResult.Unexpected)
    }

    @Test
    fun `already scanned awb returns Duplicate`() {
        val result = ScanValidator.validate(
            scannedAwb = "LS-ABC123",
            expectedAwbs = listOf("LS-ABC123"),
            alreadyScanned = listOf("LS-ABC123")
        )
        assertTrue(result is ScanValidationResult.Duplicate)
    }
}
