package io.logisticos.driver.feature.scanner.domain

object ScanValidator {
    fun validate(
        scannedAwb: String,
        expectedAwbs: List<String>,
        alreadyScanned: List<String>
    ): ScanValidationResult = when {
        scannedAwb in alreadyScanned -> ScanValidationResult.Duplicate(scannedAwb)
        scannedAwb in expectedAwbs -> ScanValidationResult.Match(scannedAwb)
        else -> ScanValidationResult.Unexpected(scannedAwb)
    }
}
