package net.cargomarket.omnideliv.courier.data

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.os.Build
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import net.cargomarket.omnideliv.courier.domain.ProofEncoding
import java.io.ByteArrayOutputStream
import java.io.File
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Turns a captured photo into a proof payload on disk.
 *
 * The *policy* — what quality to try, when to step down, when to give up and
 * accept an oversize image — lives in [ProofEncoding] and is unit-tested without
 * a device. This class is only the Android calls that policy directs.
 */
@Singleton
class ProofEncoder @Inject constructor() {

    /**
     * Encode [source] to WebP and replace it with the result.
     *
     * Runs on [Dispatchers.Default]: libwebp is roughly 2–3× slower than
     * libjpeg, which is tens of milliseconds on a modern phone but a few hundred
     * on the low-end hardware couriers actually carry — and the capture screen
     * must not stall before the payload is enqueued.
     *
     * @return the encoded file, or `null` if the image could not be decoded at
     * all, in which case the caller must not pretend a proof exists.
     */
    suspend fun encode(source: File): Encoded? = withContext(Dispatchers.Default) {
        val bitmap = BitmapFactory.decodeFile(source.absolutePath) ?: return@withContext null

        // Walks the ladder the policy publishes rather than looping until it
        // decides to stop. Bounded by construction — four attempts, and the
        // bound is visible here instead of being an emergent property of the
        // step arithmetic.
        var last: Encoded? = null
        for (quality in ProofEncoding.qualityLadder()) {
            val bytes = compress(bitmap, quality)
            when (val outcome = ProofEncoding.evaluate(quality, bytes.size.toLong())) {
                is ProofEncoding.Outcome.Retry -> {
                    // Keep going; nothing written yet.
                    last = null
                }

                is ProofEncoding.Outcome.Accepted -> {
                    val out = writeWebp(source, bytes)
                    return@withContext Encoded(out, outcome.quality, bytes.size.toLong(), false)
                }

                // Enqueued anyway. A courier must not be stuck holding an
                // undeliverable proof because their camera produced an awkward
                // image — an oversized photo that arrives beats a perfect one
                // that never does.
                is ProofEncoding.Outcome.AcceptedOversize -> {
                    val out = writeWebp(source, bytes)
                    return@withContext Encoded(out, outcome.quality, bytes.size.toLong(), true)
                }
            }
        }
        last
    }

    /**
     * Write the encoded bytes beside the capture and drop the original.
     *
     * A `.jpg` holding WebP bytes would be a lie the whole way down the pipe:
     * the server sniffs magic bytes so it would still be accepted, and then
     * every human reading a filename would be misled about what they have.
     */
    private fun writeWebp(source: File, bytes: ByteArray): File {
        val out = File(source.parentFile, source.nameWithoutExtension + ".webp")
        out.writeBytes(bytes)
        if (out.absolutePath != source.absolutePath) source.delete()
        return out
    }

    private fun compress(bitmap: Bitmap, quality: Int): ByteArray {
        val out = ByteArrayOutputStream()
        // WEBP_LOSSY is API 30+. minSdk here is 26 — the right floor for a
        // courier app in PH, where the demographic is disproportionately on
        // older hardware — so 26–29 uses the deprecated-but-functional constant.
        // Same libwebp underneath.
        val format = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            Bitmap.CompressFormat.WEBP_LOSSY
        } else {
            @Suppress("DEPRECATION")
            Bitmap.CompressFormat.WEBP
        }
        bitmap.compress(format, quality, out)
        return out.toByteArray()
    }

    data class Encoded(
        val file: File,
        val quality: Int,
        val bytes: Long,
        /** True when the ladder bottomed out and this is over the ceiling. */
        val oversize: Boolean,
    )
}
