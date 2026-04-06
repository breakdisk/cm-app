package io.logisticos.driver.feature.scanner.data

import android.content.Context
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.core.content.ContextCompat
import com.google.mlkit.vision.barcode.BarcodeScanning
import com.google.mlkit.vision.common.InputImage
import dagger.hilt.android.qualifiers.ApplicationContext
import io.logisticos.driver.feature.scanner.domain.ScanResult
import io.logisticos.driver.feature.scanner.domain.ScannerManager
import javax.inject.Inject

class MlKitScannerManager @Inject constructor(
    @ApplicationContext private val context: Context
) : ScannerManager {

    override val isHardwareScanner = false
    private var analysisUseCase: ImageAnalysis? = null
    private val scanner = BarcodeScanning.getClient()

    override fun startScan(onResult: (ScanResult) -> Unit) {
        analysisUseCase = ImageAnalysis.Builder()
            .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
            .build()
            .also { analysis ->
                analysis.setAnalyzer(ContextCompat.getMainExecutor(context)) { imageProxy ->
                    processImageProxy(imageProxy, onResult)
                }
            }
    }

    @androidx.camera.core.ExperimentalGetImage
    private fun processImageProxy(imageProxy: ImageProxy, onResult: (ScanResult) -> Unit) {
        val mediaImage = imageProxy.image ?: run { imageProxy.close(); return }
        val image = InputImage.fromMediaImage(mediaImage, imageProxy.imageInfo.rotationDegrees)
        scanner.process(image)
            .addOnSuccessListener { barcodes ->
                barcodes.firstOrNull()?.rawValue?.let { value ->
                    val format = barcodes.first().format.toString()
                    onResult(ScanResult(rawValue = value, format = format))
                }
            }
            .addOnCompleteListener { imageProxy.close() }
    }

    override fun stopScan() {
        analysisUseCase = null
        scanner.close()
    }
}
