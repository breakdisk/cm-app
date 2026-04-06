package io.logisticos.driver.feature.scanner.domain

interface ScannerManager {
    fun startScan(onResult: (ScanResult) -> Unit)
    fun stopScan()
    val isHardwareScanner: Boolean
}
