package net.cargomarket.omnideliv.courier.domain

/**
 * Why a courier cannot finish the job they hold.
 *
 * A closed set, mirrored exactly from field-ops' `ExceptionReason`. The server
 * answers an unknown value with a 400 rather than storing it, so ops triage can
 * rely on the set. The wire strings must match character for character, and
 * the wire-contract tests pin them.
 *
 * A report is a claim, not a verdict (decision D2). It moves no status and no
 * money. The courier still holds the job until ops resolves it.
 */
enum class ExceptionReason(
    val wire: String,
    val label: String,
    /**
     * Whether the goods are still in the courier's hands when this happens, so
     * "where are the goods now?" is worth asking. A courier stuck after a
     * breakdown may have them too, but that report is about the courier, and
     * the question there is a distraction.
     */
    val asksForGoods: Boolean,
) {
    CUSTOMER_UNREACHABLE("customer_unreachable", "Customer not answering", true),
    ADDRESS_UNREACHABLE("address_unreachable", "Can't reach the address", true),
    CUSTOMER_REFUSED("customer_refused", "Customer refused the order", true),
    CANNOT_PAY("cannot_pay", "Customer can't pay (COD)", true),
    GOODS_DAMAGED("goods_damaged", "Goods are damaged", true),
    COURIER_BLOCKED("courier_blocked", "Accident, breakdown or safety", false),
}

/** Longest free text the sheet accepts. Long enough for a sentence, short enough for an ops row. */
const val EXCEPTION_TEXT_MAX = 500

/**
 * What the sheet hands back. Blank text is treated as absent rather than sent
 * as an empty string the ops console would show as a field with nothing in it.
 */
data class ExceptionDraft(
    val reason: ExceptionReason,
    val note: String?,
    val goodsDisposition: String?,
) {
    companion object {
        fun of(reason: ExceptionReason, note: String, goods: String): ExceptionDraft =
            ExceptionDraft(
                reason = reason,
                note = note.trim().take(EXCEPTION_TEXT_MAX).ifBlank { null },
                goodsDisposition = if (reason.asksForGoods) {
                    goods.trim().take(EXCEPTION_TEXT_MAX).ifBlank { null }
                } else {
                    null
                },
            )
    }
}
