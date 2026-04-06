package io.logisticos.driver.feature.scanner.data

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import dagger.hilt.android.qualifiers.ApplicationContext
import io.logisticos.driver.feature.scanner.domain.ScanResult
import io.logisticos.driver.feature.scanner.domain.ScannerManager
import javax.inject.Inject

class HardwareScannerManager @Inject constructor(
    @ApplicationContext private val context: Context
) : ScannerManager {

    override val isHardwareScanner = true
    private var receiver: BroadcastReceiver? = null

    override fun startScan(onResult: (ScanResult) -> Unit) {
        receiver = object : BroadcastReceiver() {
            override fun onReceive(ctx: Context?, intent: Intent?) {
                intent?.getStringExtra("com.symbol.datawedge.data_string")?.let { value ->
                    onResult(ScanResult(rawValue = value, format = "ZEBRA_HW"))
                    return
                }
                intent?.getStringExtra("com.honeywell.aidc.barcodedata")?.let { value ->
                    onResult(ScanResult(rawValue = value, format = "HONEYWELL_HW"))
                }
            }
        }
        val filter = IntentFilter().apply {
            addAction("com.symbol.datawedge.api.RESULT_ACTION")
            addAction("com.honeywell.aidc.action.ACTION_AIDC_DATA")
        }
        context.registerReceiver(receiver, filter)
    }

    override fun stopScan() {
        receiver?.let { context.unregisterReceiver(it) }
        receiver = null
    }
}
