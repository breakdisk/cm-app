package io.logisticos.driver.core.common

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import java.io.ByteArrayOutputStream
import java.io.File

/**
 * Resizes a photo to a max 1280px long edge and re-encodes it as JPEG.
 *
 * Quality starts at 85. If the result is below MIN_SIZE_BYTES (200 KB) — which can
 * happen on featureless images — quality steps up until the floor is met or quality=100.
 * This keeps R2 storage predictable while ensuring OCR-readable output.
 *
 * Call only from a background thread (Dispatchers.IO).
 */
object ImageCompressor {

    private const val MAX_LONG_EDGE_PX = 1280
    private const val MIN_SIZE_BYTES = 200 * 1024

    /** Compress an existing JPEG file in-place. No-op if the file cannot be decoded. */
    fun compressToFile(file: File) {
        val original = BitmapFactory.decodeFile(file.absolutePath) ?: return
        val scaled = scaleDown(original)
        if (scaled !== original) original.recycle()
        val bytes = encodeWithFloor(scaled)
        scaled.recycle()
        file.writeBytes(bytes)
    }

    /** Scale + compress an in-memory bitmap directly to a file (used by PickupScreen). */
    fun compressBitmapToFile(bitmap: Bitmap, file: File) {
        val scaled = scaleDown(bitmap)
        val bytes = encodeWithFloor(scaled)
        if (scaled !== bitmap) scaled.recycle()
        file.writeBytes(bytes)
    }

    private fun scaleDown(src: Bitmap): Bitmap {
        val longEdge = maxOf(src.width, src.height)
        if (longEdge <= MAX_LONG_EDGE_PX) return src
        val scale = MAX_LONG_EDGE_PX.toFloat() / longEdge
        return Bitmap.createScaledBitmap(
            src,
            (src.width * scale).toInt(),
            (src.height * scale).toInt(),
            true
        )
    }

    private fun encodeWithFloor(bitmap: Bitmap): ByteArray {
        for (quality in intArrayOf(85, 90, 95, 100)) {
            val out = ByteArrayOutputStream()
            bitmap.compress(Bitmap.CompressFormat.JPEG, quality, out)
            val bytes = out.toByteArray()
            if (bytes.size >= MIN_SIZE_BYTES || quality == 100) return bytes
        }
        error("unreachable")
    }
}
