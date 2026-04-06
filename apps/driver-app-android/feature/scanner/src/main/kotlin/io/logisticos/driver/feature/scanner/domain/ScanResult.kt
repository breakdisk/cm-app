package io.logisticos.driver.feature.scanner.domain

data class ScanResult(val rawValue: String, val format: String)

sealed class ScanValidationResult {
    data class Match(val awb: String) : ScanValidationResult()
    data class Unexpected(val awb: String) : ScanValidationResult()
    data class Duplicate(val awb: String) : ScanValidationResult()
}
