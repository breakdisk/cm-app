package net.cargomarket.omnideliv.courier.domain

import java.time.LocalDate
import java.time.format.DateTimeParseException

/**
 * What the courier has to hold to be allowed to work, and where each document
 * stands.
 *
 * All of it is pure arithmetic over data the server already sent, so the whole
 * checklist can be tested without a device, a network or a clock. `today` is a
 * parameter everywhere for that reason — a `LocalDate.now()` buried in here
 * would make every expiry test depend on the day it ran.
 *
 * **This file does not decide whether a courier may work.** That is compliance's
 * rule and it is enforced server-side; `overall_status` arrives already
 * computed. Re-deriving it here would give the app a second opinion, and the
 * two would disagree the first time the backend's rule changed — `expired` is
 * assignable there, deliberately, because there is a grace period.
 */

/**
 * The overall verdict, as compliance words it.
 *
 * [Unknown] is not one of the server's statuses. It is what this app shows
 * before the first successful load, and for a status a newer backend added that
 * this build does not know — an unrecognised status must not be silently
 * rendered as a good one.
 */
enum class ComplianceState {
    PendingSubmission,
    UnderReview,
    Compliant,
    ExpiringSoon,
    Expired,
    Suspended,
    Rejected,
    Unknown,
    ;

    companion object {
        fun from(raw: String?): ComplianceState = when (raw) {
            "pending_submission" -> PendingSubmission
            "under_review" -> UnderReview
            "compliant" -> Compliant
            "expiring_soon" -> ExpiringSoon
            "expired" -> Expired
            "suspended" -> Suspended
            "rejected" -> Rejected
            else -> Unknown
        }
    }
}

/**
 * Where one required document stands, phrased as what the courier has to do
 * about it.
 *
 * Not a mirror of the server's `DocumentStatus`: `submitted` and `under_review`
 * both mean "we are waiting, do nothing", and `superseded` means nothing at all
 * to the person holding the phone.
 */
sealed interface DocState {
    /** Never submitted. The courier has to send one. */
    data object Missing : DocState

    /** Submitted, waiting on a reviewer. Nothing for the courier to do. */
    data object UnderReview : DocState

    /** Approved and not near expiry. [daysLeft] is null when it never expires. */
    data class Approved(val daysLeft: Long?) : DocState

    /** Approved but inside the warning window. Still valid; renew soon. */
    data class ExpiringSoon(val daysLeft: Long) : DocState

    /** Past its expiry date, or the server marked it expired. */
    data object Expired : DocState

    /** A reviewer refused it. [reason] is what they typed, if anything. */
    data class Rejected(val reason: String?) : DocState
}

/**
 * Whether this state is the courier's move.
 *
 * Drives ordering and the outstanding count. `UnderReview` is deliberately
 * *not* actionable — telling someone to act on a document a human is currently
 * reading produces duplicate submissions.
 */
val DocState.needsCourier: Boolean
    get() = when (this) {
        is DocState.Missing, is DocState.Rejected, is DocState.Expired -> true
        is DocState.ExpiringSoon, is DocState.Approved, is DocState.UnderReview -> false
    }

/** One row of the checklist. */
data class ChecklistItem(
    val typeId: String,
    val code: String,
    val name: String,
    val hasExpiry: Boolean,
    val state: DocState,
    /** Carried through so a resubmission can prefill what they typed last time. */
    val lastDocumentNumber: String? = null,
)

/** A required document type, as the server describes it. */
data class RequiredType(
    val id: String,
    val code: String,
    val name: String,
    val hasExpiry: Boolean,
    val warnDaysBefore: Int,
)

/** A document the courier has already submitted. */
data class SubmittedDoc(
    val documentTypeId: String,
    val documentNumber: String,
    val status: String,
    val expiryDate: String?,
    val rejectionReason: String?,
    /** ISO 8601. Compared as a string — see [newestOf]. */
    val submittedAt: String,
)

/**
 * Join what is required against what has been submitted.
 *
 * Ordering is part of the answer, not decoration: everything the courier has to
 * act on comes first, then what is expiring, then what is done. A courier
 * opening this screen should not have to read past four approved licences to
 * find the one that was rejected.
 */
fun buildChecklist(
    required: List<RequiredType>,
    documents: List<SubmittedDoc>,
    today: LocalDate,
): List<ChecklistItem> =
    required
        .map { type -> item(type, newestOf(documents, type.id), today) }
        .sortedWith(compareBy({ rank(it.state) }, { it.name }))

/**
 * Sort key. Lower sorts first.
 *
 * Rejected outranks missing: a refusal is a thing that already went wrong and
 * carries a reason the courier needs to read, whereas a missing document is
 * merely not started.
 */
private fun rank(state: DocState): Int = when (state) {
    is DocState.Rejected -> 0
    is DocState.Expired -> 1
    is DocState.Missing -> 2
    is DocState.ExpiringSoon -> 3
    is DocState.UnderReview -> 4
    is DocState.Approved -> 5
}

/**
 * The document that decides this requirement's state.
 *
 * **Newest wins, and `superseded` is discarded first.** A courier who renews an
 * approved licence and has the renewal refused must see the refusal — showing
 * the older approval instead would tell them everything is fine while the
 * server disagreed. `superseded` is the server's own bookkeeping for a replaced
 * document and means nothing to the person holding the phone.
 */
private fun newestOf(documents: List<SubmittedDoc>, typeId: String): SubmittedDoc? =
    documents
        .filter { it.documentTypeId == typeId && it.status != "superseded" }
        // ISO 8601 in UTC sorts correctly as text, which is why the server's
        // format is worth relying on rather than parsing 100 timestamps to
        // order a list of four.
        .maxByOrNull { it.submittedAt }

private fun item(type: RequiredType, doc: SubmittedDoc?, today: LocalDate): ChecklistItem =
    ChecklistItem(
        typeId = type.id,
        code = type.code,
        name = type.name,
        hasExpiry = type.hasExpiry,
        state = stateOf(type, doc, today),
        lastDocumentNumber = doc?.documentNumber,
    )

private fun stateOf(type: RequiredType, doc: SubmittedDoc?, today: LocalDate): DocState {
    if (doc == null) return DocState.Missing

    return when (doc.status) {
        "submitted", "under_review" -> DocState.UnderReview
        "rejected" -> DocState.Rejected(doc.rejectionReason?.takeIf { it.isNotBlank() })
        "expired" -> DocState.Expired
        "approved" -> approvedState(type, doc.expiryDate, today)
        // A status this build does not know. Treated as under review rather
        // than approved: the safe direction for an unknown is "wait", never
        // "you are done".
        else -> DocState.UnderReview
    }
}

/**
 * An approved document is not necessarily a currently-valid one.
 *
 * The server marks documents expired on a daily sweep, so between a document
 * lapsing and that sweep running it is still `approved` with a past date. The
 * courier should be told the moment it lapses, not the next time a background
 * job happens to run.
 */
private fun approvedState(type: RequiredType, expiry: String?, today: LocalDate): DocState {
    val date = parseDate(expiry) ?: return DocState.Approved(null)
    val daysLeft = java.time.temporal.ChronoUnit.DAYS.between(today, date)

    return when {
        daysLeft < 0 -> DocState.Expired
        daysLeft <= type.warnDaysBefore -> DocState.ExpiringSoon(daysLeft)
        else -> DocState.Approved(daysLeft)
    }
}

/**
 * `null` for absent *and* for unparseable.
 *
 * A malformed date must not crash the one screen a blocked courier opens to
 * find out why they cannot work. Treated as "no expiry", which renders as
 * approved-without-a-date rather than as an error the courier cannot act on.
 */
internal fun parseDate(raw: String?): LocalDate? {
    if (raw.isNullOrBlank()) return null
    return try {
        LocalDate.parse(raw.take(10))
    } catch (_: DateTimeParseException) {
        null
    }
}

/** How many rows are the courier's move. Drives the badge on the shift screen. */
fun outstandingCount(items: List<ChecklistItem>): Int = items.count { it.state.needsCourier }

/**
 * What the shift screen's **Documents** link carries, or `null` for nothing.
 *
 * The gap this closes: that link used to be bare text. A courier whose licence
 * was refused was told nothing at all until they happened to tap through, and
 * a refusal is precisely the state that needs finding — it is their move, it
 * carries a reason someone typed for them to read, and it is what will stop
 * them working the day gating is switched on.
 *
 * A count, not a word. Anything longer competes with the duty switch beside it,
 * and a number is the same in every language this app ships in.
 */
fun documentsBadge(outstanding: Int): String? =
    if (outstanding > 0) outstanding.toString() else null

/**
 * The badge, spoken.
 *
 * A bare "3" beside a link is meaningless to a screen reader, so the whole
 * control gets a description rather than the number carrying it alone.
 *
 * Says what is true and no more. Gating still ships off, so a courier with
 * outstanding documents *is* being offered work; a badge implying otherwise
 * describes a rule not in force, which they can disprove by simply waiting for
 * the next offer. The forbidden-phrasings test covers this string too.
 */
fun documentsBadgeDescription(outstanding: Int): String = when {
    outstanding <= 0 -> "Documents"
    outstanding == 1 -> "Documents, 1 needs your attention"
    else -> "Documents, $outstanding need your attention"
}

/**
 * One line saying where the courier stands, in words rather than a status code.
 *
 * Deliberately does not promise anything about whether they will be offered
 * work. Compliance gating ships switched off, so a courier with outstanding
 * documents is still receiving jobs today, and a screen that said "you cannot
 * work" would be describing a rule that is not in force.
 */
fun summaryLine(state: ComplianceState, outstanding: Int): String = when {
    outstanding == 1 -> "1 document needs your attention"
    outstanding > 1 -> "$outstanding documents need your attention"
    state == ComplianceState.UnderReview -> "Everything is submitted — waiting on review"
    state == ComplianceState.Compliant -> "All your documents are up to date"
    state == ComplianceState.ExpiringSoon -> "All approved, but something is expiring soon"
    state == ComplianceState.Suspended -> "Your account is suspended. Contact your operations team."
    state == ComplianceState.Unknown -> "We could not read your document status"
    else -> "Nothing outstanding"
}

/**
 * Whether the courier may submit this document type right now.
 *
 * A document under review is not resubmittable: the server would accept it and
 * create a second row, giving a reviewer two copies of the same licence to
 * decide between. An approved one that is not near expiry is not worth
 * replacing either.
 */
fun canSubmit(state: DocState): Boolean = when (state) {
    is DocState.Missing, is DocState.Rejected, is DocState.Expired, is DocState.ExpiringSoon -> true
    is DocState.UnderReview, is DocState.Approved -> false
}

/**
 * Validate what the courier typed, before spending their data on an upload.
 *
 * Mirrors the server's rules (`document_number` trimmed, non-empty, 100 chars
 * max) so a refusal arrives instantly instead of after a photo upload — but the
 * server still enforces them. This is a courtesy, not the check.
 */
sealed interface SubmitValidation {
    data object Valid : SubmitValidation
    data class Invalid(val message: String) : SubmitValidation
}

fun validateSubmission(
    documentNumber: String,
    expiryDate: String?,
    requiresExpiry: Boolean,
    today: LocalDate,
): SubmitValidation {
    val number = documentNumber.trim()
    if (number.isEmpty()) return SubmitValidation.Invalid("Enter the number printed on the document.")
    if (number.length > 100) return SubmitValidation.Invalid("That number is too long — 100 characters at most.")

    if (requiresExpiry) {
        val parsed = parseDate(expiryDate)
            ?: return SubmitValidation.Invalid("Enter the expiry date as YYYY-MM-DD.")
        // An already-expired document would be accepted by the server and then
        // immediately fail the expiry sweep, so the courier would upload,
        // wait, and be refused for a reason nothing told them up front.
        if (parsed.isBefore(today)) {
            return SubmitValidation.Invalid("That date has already passed — this document is expired.")
        }
    }
    return SubmitValidation.Valid
}
