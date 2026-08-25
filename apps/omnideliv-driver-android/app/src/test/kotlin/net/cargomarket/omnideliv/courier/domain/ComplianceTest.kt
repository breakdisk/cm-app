package net.cargomarket.omnideliv.courier.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.LocalDate

/**
 * The compliance checklist, tested without a device, a network or a clock.
 *
 * `TODAY` is a fixed date passed into every call. A `LocalDate.now()` inside the
 * production code would make the expiry tests pass or fail depending on the day
 * they ran, which is the kind of green that means nothing.
 */
class ComplianceTest {

    private val TODAY: LocalDate = LocalDate.of(2026, 8, 24)

    private fun type(
        id: String = "t1",
        code: String = "PH_LTO_LICENSE",
        name: String = "LTO Driving License",
        hasExpiry: Boolean = true,
        warn: Int = 30,
    ) = RequiredType(id, code, name, hasExpiry, warn)

    private fun doc(
        typeId: String = "t1",
        status: String = "approved",
        expiry: String? = null,
        reason: String? = null,
        at: String = "2026-08-01T00:00:00Z",
        number: String = "N-1",
    ) = SubmittedDoc(typeId, number, status, expiry, reason, at)

    // ── The join ───────────────────────────────────────────────────────────

    @Test
    fun a_required_type_with_nothing_submitted_is_missing() {
        val items = buildChecklist(listOf(type()), emptyList(), TODAY)
        assertEquals(1, items.size)
        assertEquals(DocState.Missing, items[0].state)
    }

    @Test
    fun submitted_and_under_review_both_read_as_waiting() {
        for (s in listOf("submitted", "under_review")) {
            val items = buildChecklist(listOf(type()), listOf(doc(status = s)), TODAY)
            assertEquals("status $s", DocState.UnderReview, items[0].state)
        }
    }

    @Test
    fun a_rejection_carries_its_reason() {
        val items = buildChecklist(
            listOf(type()),
            listOf(doc(status = "rejected", reason = "Photo is blurred")),
            TODAY,
        )
        assertEquals(DocState.Rejected("Photo is blurred"), items[0].state)
    }

    /** A reviewer who rejected without typing anything must not surface `""`. */
    @Test
    fun a_rejection_with_no_reason_carries_null_not_blank() {
        val items = buildChecklist(
            listOf(type()),
            listOf(doc(status = "rejected", reason = "   ")),
            TODAY,
        )
        assertEquals(DocState.Rejected(null), items[0].state)
    }

    /**
     * The case that decides whether this screen tells the truth. A courier
     * renews an approved licence and the renewal is refused: showing the older
     * approval would say everything is fine while the server disagreed.
     */
    @Test
    fun a_rejected_renewal_beats_an_older_approval() {
        val items = buildChecklist(
            listOf(type()),
            listOf(
                doc(status = "approved", expiry = "2030-01-01", at = "2026-01-01T00:00:00Z"),
                doc(status = "rejected", reason = "Expired scan", at = "2026-08-20T00:00:00Z"),
            ),
            TODAY,
        )
        assertEquals(DocState.Rejected("Expired scan"), items[0].state)
    }

    /** `superseded` is the server's bookkeeping and means nothing to a courier. */
    @Test
    fun a_superseded_document_is_ignored_even_when_it_is_the_newest() {
        val items = buildChecklist(
            listOf(type()),
            listOf(
                doc(status = "approved", expiry = "2026-12-24", at = "2026-01-01T00:00:00Z"),
                doc(status = "superseded", at = "2026-08-23T00:00:00Z"),
            ),
            TODAY,
        )
        assertEquals(DocState.Approved(122L), items[0].state)
    }

    /** Documents for other types must not leak into this row. */
    @Test
    fun a_document_for_another_type_does_not_satisfy_this_one() {
        val items = buildChecklist(
            listOf(type(id = "t1")),
            listOf(doc(typeId = "t2", status = "approved")),
            TODAY,
        )
        assertEquals(DocState.Missing, items[0].state)
    }

    // ── Expiry ─────────────────────────────────────────────────────────────

    @Test
    fun an_approved_document_with_no_expiry_is_simply_approved() {
        val items = buildChecklist(listOf(type()), listOf(doc(expiry = null)), TODAY)
        assertEquals(DocState.Approved(null), items[0].state)
    }

    @Test
    fun an_approved_document_well_inside_its_term_reports_days_left() {
        val items = buildChecklist(listOf(type(warn = 30)), listOf(doc(expiry = "2026-12-24")), TODAY)
        assertEquals(DocState.Approved(122L), items[0].state)
    }

    @Test
    fun inside_the_warning_window_it_is_expiring_soon() {
        val items = buildChecklist(listOf(type(warn = 30)), listOf(doc(expiry = "2026-09-10")), TODAY)
        assertEquals(DocState.ExpiringSoon(17L), items[0].state)
    }

    /** The boundary: exactly `warn_days_before` away is already a warning. */
    @Test
    fun the_warning_window_boundary_is_inclusive() {
        val items = buildChecklist(listOf(type(warn = 30)), listOf(doc(expiry = "2026-09-23")), TODAY)
        assertEquals(DocState.ExpiringSoon(30L), items[0].state)

        val justOutside = buildChecklist(listOf(type(warn = 30)), listOf(doc(expiry = "2026-09-24")), TODAY)
        assertEquals(DocState.Approved(31L), justOutside[0].state)
    }

    /**
     * The server marks documents expired on a daily sweep. Between a licence
     * lapsing and that job running it is still `approved` with a past date, and
     * the courier should be told the moment it lapses rather than whenever a
     * background job next happens to run.
     */
    @Test
    fun an_approved_document_whose_date_has_passed_reads_as_expired() {
        val items = buildChecklist(listOf(type()), listOf(doc(expiry = "2026-08-23")), TODAY)
        assertEquals(DocState.Expired, items[0].state)
    }

    @Test
    fun expiring_today_is_still_valid_not_expired() {
        val items = buildChecklist(listOf(type(warn = 30)), listOf(doc(expiry = "2026-08-24")), TODAY)
        assertEquals(DocState.ExpiringSoon(0L), items[0].state)
    }

    /**
     * A malformed date must not crash the one screen a blocked courier opens to
     * find out why. It degrades to "no expiry recorded".
     */
    @Test
    fun a_malformed_expiry_date_does_not_throw() {
        val items = buildChecklist(listOf(type()), listOf(doc(expiry = "not-a-date")), TODAY)
        assertEquals(DocState.Approved(null), items[0].state)
    }

    /** A status a newer backend added must never render as "you are done". */
    @Test
    fun an_unknown_document_status_waits_rather_than_passes() {
        val items = buildChecklist(listOf(type()), listOf(doc(status = "quarantined")), TODAY)
        assertEquals(DocState.UnderReview, items[0].state)
    }

    // ── Ordering ───────────────────────────────────────────────────────────

    /**
     * A courier opening this screen must not read past four approved licences
     * to find the rejected one. Ordering is the answer, not decoration.
     */
    @Test
    fun everything_the_courier_must_act_on_sorts_to_the_top() {
        val types = listOf(
            type(id = "a", name = "A approved"),
            type(id = "b", name = "B missing"),
            type(id = "c", name = "C rejected"),
            type(id = "d", name = "D expiring"),
            type(id = "e", name = "E waiting"),
        )
        val docs = listOf(
            doc(typeId = "a", status = "approved", expiry = "2030-01-01"),
            doc(typeId = "c", status = "rejected", reason = "no"),
            doc(typeId = "d", status = "approved", expiry = "2026-09-01"),
            doc(typeId = "e", status = "submitted"),
        )

        val order = buildChecklist(types, docs, TODAY).map { it.name }

        assertEquals(
            listOf("C rejected", "B missing", "D expiring", "E waiting", "A approved"),
            order,
        )
    }

    /** Same state, stable order by name — not by whatever the server sent. */
    @Test
    fun rows_in_the_same_state_are_ordered_by_name() {
        val types = listOf(type(id = "z", name = "Zebra"), type(id = "a", name = "Alpha"))
        val order = buildChecklist(types, emptyList(), TODAY).map { it.name }
        assertEquals(listOf("Alpha", "Zebra"), order)
    }

    // ── Actionability ──────────────────────────────────────────────────────

    /**
     * Under review is deliberately NOT the courier's move. Telling someone to
     * act on a document a human is currently reading produces two copies of the
     * same licence for that reviewer to choose between.
     */
    @Test
    fun waiting_on_a_reviewer_is_not_the_couriers_move() {
        assertFalse(DocState.UnderReview.needsCourier)
        assertFalse(DocState.Approved(null).needsCourier)
        assertFalse(DocState.ExpiringSoon(3).needsCourier)

        assertTrue(DocState.Missing.needsCourier)
        assertTrue(DocState.Rejected(null).needsCourier)
        assertTrue(DocState.Expired.needsCourier)
    }

    @Test
    fun outstanding_counts_only_what_the_courier_can_act_on() {
        val types = listOf(type(id = "a"), type(id = "b"), type(id = "c"))
        val docs = listOf(
            doc(typeId = "a", status = "approved", expiry = "2030-01-01"),
            doc(typeId = "b", status = "submitted"),
        )
        assertEquals(1, outstandingCount(buildChecklist(types, docs, TODAY)))
    }

    /** Resubmitting under review would hand a reviewer a duplicate. */
    @Test
    fun only_actionable_or_expiring_documents_can_be_submitted() {
        assertTrue(canSubmit(DocState.Missing))
        assertTrue(canSubmit(DocState.Rejected(null)))
        assertTrue(canSubmit(DocState.Expired))
        assertTrue("renewing early must be possible", canSubmit(DocState.ExpiringSoon(5)))

        assertFalse(canSubmit(DocState.UnderReview))
        assertFalse(canSubmit(DocState.Approved(400)))
    }

    // ── Status mapping ─────────────────────────────────────────────────────

    @Test
    fun every_status_the_server_emits_maps_to_a_known_state() {
        val mapping = mapOf(
            "pending_submission" to ComplianceState.PendingSubmission,
            "under_review" to ComplianceState.UnderReview,
            "compliant" to ComplianceState.Compliant,
            "expiring_soon" to ComplianceState.ExpiringSoon,
            "expired" to ComplianceState.Expired,
            "suspended" to ComplianceState.Suspended,
            "rejected" to ComplianceState.Rejected,
        )
        mapping.forEach { (raw, expected) -> assertEquals(raw, expected, ComplianceState.from(raw)) }
    }

    /** An unrecognised status must not be rendered as a good one. */
    @Test
    fun an_unrecognised_status_is_unknown_not_compliant() {
        assertEquals(ComplianceState.Unknown, ComplianceState.from("brand_new_status"))
        assertEquals(ComplianceState.Unknown, ComplianceState.from(null))
    }

    // ── Summary wording ────────────────────────────────────────────────────

    @Test
    fun the_summary_counts_outstanding_documents_before_anything_else() {
        assertEquals(
            "1 document needs your attention",
            summaryLine(ComplianceState.Compliant, 1),
        )
        assertEquals(
            "3 documents need your attention",
            summaryLine(ComplianceState.UnderReview, 3),
        )
    }

    /**
     * Compliance gating ships switched off, so a courier with outstanding
     * documents IS still being offered work. Nothing on this screen may claim
     * otherwise — a message describing a rule that is not in force is a lie the
     * courier can immediately disprove.
     */
    @Test
    fun no_summary_wording_claims_the_courier_cannot_work() {
        val forbidden = listOf("cannot work", "can't work", "blocked", "not receiving")
        for (state in ComplianceState.entries) {
            for (n in 0..2) {
                val line = summaryLine(state, n).lowercase()
                forbidden.forEach { phrase ->
                    assertFalse("$state/$n said: $line", line.contains(phrase))
                }
            }
        }
    }

    /**
     * The badge on the shift screen is read by couriers who never open the
     * compliance screen at all, so the same rule binds it — and it is the more
     * likely of the two to be reworded in a hurry.
     */
    @Test
    fun no_badge_wording_claims_the_courier_cannot_work() {
        val forbidden = listOf("cannot work", "can't work", "blocked", "not receiving")
        for (n in 0..5) {
            val spoken = documentsBadgeDescription(n).lowercase()
            forbidden.forEach { phrase ->
                assertFalse("$n said: $spoken", spoken.contains(phrase))
            }
        }
    }

    // ── The shift-screen badge ─────────────────────────────────────────────

    /**
     * Nothing outstanding shows nothing. A permanent dot beside a link teaches
     * couriers to stop seeing it, which costs exactly the one case the badge
     * exists for.
     */
    @Test
    fun nothing_outstanding_shows_no_badge() {
        assertEquals(null, documentsBadge(0))
    }

    @Test
    fun outstanding_documents_show_their_count() {
        assertEquals("1", documentsBadge(1))
        assertEquals("4", documentsBadge(4))
    }

    /** A negative count is nonsense, and nonsense must not render as a badge. */
    @Test
    fun a_negative_count_shows_no_badge() {
        assertEquals(null, documentsBadge(-1))
    }

    /** A bare "3" beside a word means nothing to a screen reader. */
    @Test
    fun the_badge_is_spoken_in_words() {
        assertEquals("Documents", documentsBadgeDescription(0))
        assertEquals("Documents, 1 needs your attention", documentsBadgeDescription(1))
        assertEquals("Documents, 3 need your attention", documentsBadgeDescription(3))
    }

    /**
     * The badge counts the courier's move, not the checklist. Four required
     * documents of which one was refused is a badge of one — a badge of four
     * would send them to resubmit three the server is holding as approved.
     */
    @Test
    fun the_badge_counts_only_what_is_the_couriers_move() {
        val types = listOf(
            type(id = "t1", name = "Licence"),
            type(id = "t2", name = "Insurance"),
            type(id = "t3", name = "Registration"),
        )
        val docs = listOf(
            doc(typeId = "t1", status = "rejected"),
            doc(typeId = "t2", status = "approved", expiry = "2030-01-01"),
            doc(typeId = "t3", status = "submitted"),
        )
        val items = buildChecklist(types, docs, TODAY)
        assertEquals(1, outstandingCount(items))
        assertEquals("1", documentsBadge(outstandingCount(items)))
    }

    @Test
    fun a_suspended_courier_is_told_who_to_talk_to() {
        assertTrue(
            summaryLine(ComplianceState.Suspended, 0).contains("operations team"),
        )
    }

    // ── Submission validation ──────────────────────────────────────────────

    @Test
    fun a_blank_document_number_is_refused_before_a_photo_is_spent() {
        assertEquals(
            SubmitValidation.Invalid("Enter the number printed on the document."),
            validateSubmission("   ", "2030-01-01", true, TODAY),
        )
    }

    /** Mirrors the server's 100-character cap, so the refusal is instant. */
    @Test
    fun an_over_long_document_number_is_refused() {
        val long = "x".repeat(101)
        assertTrue(validateSubmission(long, "2030-01-01", true, TODAY) is SubmitValidation.Invalid)
        assertEquals(
            SubmitValidation.Valid,
            validateSubmission("x".repeat(100), "2030-01-01", true, TODAY),
        )
    }

    @Test
    fun a_type_with_an_expiry_requires_a_parseable_date() {
        assertTrue(validateSubmission("N-1", null, true, TODAY) is SubmitValidation.Invalid)
        assertTrue(validateSubmission("N-1", "24/08/2026", true, TODAY) is SubmitValidation.Invalid)
        assertEquals(SubmitValidation.Valid, validateSubmission("N-1", "2026-09-01", true, TODAY))
    }

    /**
     * Uploading an already-expired document would be accepted and then fail the
     * server's expiry sweep, so the courier would upload, wait, and be refused
     * for a reason nothing told them up front.
     */
    @Test
    fun an_already_expired_date_is_refused_up_front() {
        assertTrue(validateSubmission("N-1", "2026-08-23", true, TODAY) is SubmitValidation.Invalid)
    }

    @Test
    fun a_type_without_an_expiry_does_not_demand_a_date() {
        assertEquals(SubmitValidation.Valid, validateSubmission("N-1", null, false, TODAY))
    }
}
