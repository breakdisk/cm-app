package io.logisticos.driver.core.common

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import java.io.ByteArrayOutputStream
import java.io.File

/**
 * Resizes a photo to a max 1280px long edge and re-encodes it as JPEG
 * within a [MIN_SIZE_BYTES, MAX_SIZE_BYTES] size window.
 *
 * # Algorithm
 *
 * 1. Scale down to [MAX_LONG_EDGE_PX] if needed.
 * 2. Start encoding at quality 85.
 * 3. **Ceiling pass** — if result > [MAX_SIZE_BYTES] (800 KB): step quality down
 *    by 10 each iteration until it fits or quality reaches the minimum floor (30).
 *    Quality 30 is still readable for signature-and-parcel confirmation photos.
 * 4. **Floor pass** — if result < [MIN_SIZE_BYTES] (200 KB): step quality up
 *    until the floor is met or quality = 100. This keeps R2 storage predictable
 *    and ensures OCR-readable output on featureless backgrounds.
 *
 * Call only from a background thread (Dispatchers.IO).
 *
 * Thread-safety: all operations are stateless; safe to call concurrently.
 */
object ImageCompressor {

    /** Hard upload ceiling — R2 policy, Cloudflare body limit, and UX responsiveness. */
    const val MAX_SIZE_BYTES = 800 * 1024   // 800 KB

    /** Soft minimum — featureless images can compress to near-zero at quality 85. */
    private const val MIN_SIZE_BYTES = 200 * 1024   // 200 KB

    private const val MAX_LONG_EDGE_PX = 1280

    private const val QUALITY_START = 85
    private const val QUALITY_FLOOR = 30    // lowest acceptable quality (still readable)
    private const val QUALITY_STEP  = 10

    // ── Public API ────────────────────────────────────────────────────────────

    /** Compress an existing file in-place. No-op if the file cannot be decoded. */
    fun compressToFile(file: File) {
        val original = BitmapFactory.decodeFile(file.absolutePath) ?: return
        val scaled = scaleDown(original)
        if (scaled !== original) original.recycle()
        val bytes = encode(scaled)
        scaled.recycle()
        file.writeBytes(bytes)
    }

    /** Scale + compress an in-memory bitmap directly to a file (used by PickupScreen). */
    fun compressBitmapToFile(bitmap: Bitmap, file: File) {
        val scaled = scaleDown(bitmap)
        val bytes = encode(scaled)
        if (scaled !== bitmap) scaled.recycle()
        file.writeBytes(bytes)
    }

    /**
     * Returns compressed JPEG bytes for the given bitmap.
     * Result is guaranteed to be ≤ [MAX_SIZE_BYTES] unless quality hits the floor,
     * in which case the smallest achievable encoding is returned.
     */
    fun encode(bitmap: Bitmap): ByteArray {
        val scaled = scaleDown(bitmap)
        val result = encodeWithBounds(scaled)
        if (scaled !== bitmap) scaled.recycle()
        return result
    }

    // ── Internals ─────────────────────────────────────────────────────────────

    private fun scaleDown(src: Bitmap): Bitmap {
        val longEdge = maxOf(src.width, src.height)
        if (longEdge <= MAX_LONG_EDGE_PX) return src
        val scale = MAX_LONG_EDGE_PX.toFloat() / longEdge
        return Bitmap.createScaledBitmap(
            src,
            (src.width  * scale).toInt(),
            (src.height * scale).toInt(),
            /* filter = */ true,
        )
    }

    /**
     * Encode within [MIN_SIZE_BYTES, MAX_SIZE_BYTES].
     *
     * Ceiling pass first (quality down), then floor pass (quality up).
     * Two-pass guarantees we don't bounce forever: ceiling takes priority
     * because an oversized upload fails hard, while an undersized one
     * just wastes a small amount of storage.
     */
    private fun encodeWithBounds(bitmap: Bitmap): ByteArray {
        var quality = QUALITY_START
        var best: ByteArray? = null

        // ── Ceiling pass — reduce quality until ≤ 800 KB ──────────────────
        while (quality >= QUALITY_FLOOR) {
            val out = ByteArrayOutputStream()
            bitmap.compress(Bitmap.CompressFormat.JPEG, quality, out)
            val bytes = out.toByteArray()

            if (bytes.size <= MAX_SIZE_BYTES) {
                best = bytes
                break
            }
            best = bytes               // keep as "best so far" in case floor is hit
            quality -= QUALITY_STEP
        }

        // If still oversized after hitting the floor, return smallest we got.
        val afterCeiling = best ?: run {
            val out = ByteArrayOutputStream()
            bitmap.compress(Bitmap.CompressFormat.JPEG, QUALITY_FLOOR, out)
            out.toByteArray()
        }

        if (afterCeiling.size > MAX_SIZE_BYTES) {
            android.util.Log.w(
                "ImageCompressor",
                "Photo still ${afterCeiling.size / 1024} KB at quality $QUALITY_FLOOR — uploading anyway"
            )
            return afterCeiling
        }

        // ── Floor pass — bump quality if result is < 200 KB ───────────────
        if (afterCeiling.size >= MIN_SIZE_BYTES) return afterCeiling

        var floorBest = afterCeiling
        var q2 = quality + QUALITY_STEP
        while (q2 <= 100) {
            val out = ByteArrayOutputStream()
            bitmap.compress(Bitmap.CompressFormat.JPEG, q2, out)
            val bytes = out.toByteArray()
            floorBest = bytes
            if (bytes.size >= MIN_SIZE_BYTES) break
            q2 += QUALITY_STEP
        }
        return floorBest
    }
}
