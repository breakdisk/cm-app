package net.cargomarket.omnideliv.courier.domain

/**
 * Turning a captured document photo into something the compliance service will
 * accept.
 *
 * The upload endpoint takes base64 inside JSON rather than multipart, so the
 * bytes on the wire are about a third larger than the file on disk. This object
 * owns that arithmetic and the two caps it has to clear, so the limits are
 * stated in one place instead of being folk knowledge spread across a ViewModel.
 */
object DocumentPayload {

    /**
     * What this app always sends.
     *
     * The storage layer accepts `image/jpeg`, `image/png`, `image/webp` and
     * `application/pdf` and rejects everything else outright. The encoder
     * produces WebP, so this is a constant rather than something sniffed from
     * the file — a mismatch between the declared type and the actual bytes
     * would be stored and then mislead every human who opened it.
     */
    const val CONTENT_TYPE = "image/webp"

    /**
     * The route's body cap (16 MB), which the whole JSON request must fit
     * inside — base64 payload, metadata and all.
     */
    const val MAX_BODY_BYTES = 16L * 1024 * 1024

    /** The storage layer's own cap on the decoded file (10 MB). */
    const val MAX_FILE_BYTES = 10L * 1024 * 1024

    /**
     * Exact encoded length for [rawBytes], padding included.
     *
     * Base64 emits 4 characters per 3 input bytes and pads the last group, so
     * this is `ceil(n/3) * 4` — not the "about a third bigger" approximation,
     * because the point of computing it is to decide whether something fits.
     */
    fun base64Size(rawBytes: Long): Long = ((rawBytes + 2) / 3) * 4

    /**
     * Will this file clear both caps once encoded?
     *
     * Checked before spending a courier's mobile data on an upload the server
     * is going to refuse. A ~300 KB WebP encodes to ~400 KB and clears both by
     * two orders of magnitude, so in practice this only ever fires for
     * something that has gone wrong upstream.
     */
    fun fits(rawBytes: Long): Boolean =
        rawBytes <= MAX_FILE_BYTES && base64Size(rawBytes) <= MAX_BODY_BYTES

    /**
     * Encode without line breaks.
     *
     * `java.util.Base64.getEncoder()` is RFC 4648 basic — no wrapping. This
     * matters: Android's own `android.util.Base64.DEFAULT` inserts a newline
     * every 76 characters, and the server decodes with Rust's
     * `STANDARD` engine, which **rejects** embedded newlines rather than
     * skipping them. The result would be a 400 on every upload with a message
     * about invalid base64 and a photo that looked perfectly fine on the phone.
     *
     * `java.util.Base64` needs API 26, which is this app's `minSdk` exactly.
     */
    fun encode(bytes: ByteArray): String = java.util.Base64.getEncoder().encodeToString(bytes)
}
