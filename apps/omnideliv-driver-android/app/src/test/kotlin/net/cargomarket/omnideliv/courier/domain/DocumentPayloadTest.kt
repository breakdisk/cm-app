package net.cargomarket.omnideliv.courier.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class DocumentPayloadTest {

    /**
     * The trap this object exists to close.
     *
     * Android's own `android.util.Base64.DEFAULT` inserts a newline every 76
     * characters. The server decodes with Rust's `STANDARD` engine, which
     * **rejects** embedded newlines rather than skipping them — so every upload
     * would 400 with a base64 error while the photo looked perfectly fine on
     * the phone. `java.util.Base64.getEncoder()` is RFC 4648 basic and wraps
     * nothing.
     */
    @Test
    fun encoding_never_wraps_however_large_the_payload() {
        val big = ByteArray(64 * 1024) { (it % 251).toByte() }
        val encoded = DocumentPayload.encode(big)

        assertFalse("a newline would be rejected by the server", encoded.contains('\n'))
        assertFalse(encoded.contains('\r'))
        assertTrue(encoded.length > 76)
    }

    @Test
    fun encoding_round_trips() {
        val bytes = byteArrayOf(0, 1, 2, 3, 127, -1, -128, 42)
        val decoded = java.util.Base64.getDecoder().decode(DocumentPayload.encode(bytes))
        assertTrue(bytes.contentEquals(decoded))
    }

    /** Exact, padding included — the point of computing it is to decide fit. */
    @Test
    fun the_encoded_size_is_exact_not_an_approximation() {
        for (n in 0..64) {
            val actual = DocumentPayload.encode(ByteArray(n)).length.toLong()
            assertEquals("n=$n", actual, DocumentPayload.base64Size(n.toLong()))
        }
    }

    /**
     * A typical encoded document is ~300 KB and clears both caps by orders of
     * magnitude. This is the check that says so rather than assuming it.
     */
    @Test
    fun a_normal_document_photo_fits_easily() {
        assertTrue(DocumentPayload.fits(300L * 1024))
        assertTrue(DocumentPayload.fits(DocumentPayload.MAX_FILE_BYTES))
    }

    /**
     * The storage layer's 10 MB file cap bites before the route's 16 MB body
     * cap does, because 10 MB of file is only ~13.3 MB of base64. Both are
     * checked so neither becomes the surprise.
     */
    @Test
    fun the_file_cap_is_what_bites_first() {
        assertFalse(DocumentPayload.fits(DocumentPayload.MAX_FILE_BYTES + 1))

        // 10 MB encodes to about 13.3 MB, comfortably inside the 16 MB body cap
        // — proving the file cap is the binding one rather than assuming it.
        val encodedAtFileCap = DocumentPayload.base64Size(DocumentPayload.MAX_FILE_BYTES)
        assertTrue(encodedAtFileCap < DocumentPayload.MAX_BODY_BYTES)
    }

    @Test
    fun an_empty_file_encodes_to_nothing_and_still_fits() {
        assertEquals("", DocumentPayload.encode(ByteArray(0)))
        assertEquals(0L, DocumentPayload.base64Size(0))
        assertTrue(DocumentPayload.fits(0))
    }
}
