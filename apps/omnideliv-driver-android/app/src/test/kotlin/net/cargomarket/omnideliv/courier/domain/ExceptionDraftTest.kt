package net.cargomarket.omnideliv.courier.domain

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Test

class ExceptionDraftTest {

    @Test
    fun `blank text is absent, not an empty string`() {
        val d = ExceptionDraft.of(ExceptionReason.CUSTOMER_UNREACHABLE, "   ", "")
        assertNull(d.note)
        assertNull(d.goodsDisposition)
    }

    /** The goods field is hidden for a courier-side problem, so nothing typed earlier may leak through. */
    @Test
    fun `a courier-blocked report never carries a goods disposition`() {
        val d = ExceptionDraft.of(ExceptionReason.COURIER_BLOCKED, "flat tyre", "with me")
        assertEquals("flat tyre", d.note)
        assertNull(d.goodsDisposition)
    }

    @Test
    fun `text is trimmed and capped`() {
        val d = ExceptionDraft.of(ExceptionReason.GOODS_DAMAGED, "  x".padEnd(900, 'y'), " box crushed ")
        assertEquals(EXCEPTION_TEXT_MAX, d.note!!.length)
        assertEquals("box crushed", d.goodsDisposition)
    }
}
