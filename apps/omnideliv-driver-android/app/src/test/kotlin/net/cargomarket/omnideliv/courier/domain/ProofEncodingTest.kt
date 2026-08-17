package net.cargomarket.omnideliv.courier.domain

import net.cargomarket.omnideliv.courier.domain.ProofEncoding.Outcome
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertInstanceOf
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

class ProofEncodingTest {

    @Test
    fun `a photo under the target is accepted as is`() {
        val o = ProofEncoding.evaluate(quality = 80, bytes = 250L * 1024)
        assertInstanceOf(Outcome.Accepted::class.java, o)
        assertEquals(80, (o as Outcome.Accepted).quality)
    }

    /**
     * Between target and ceiling is accepted rather than re-encoded. Another
     * pass costs the courier a wait on a screen they are standing still for, to
     * save bytes that are already within budget.
     */
    @Test
    fun `a photo between the target and the ceiling is accepted without another pass`() {
        val o = ProofEncoding.evaluate(quality = 80, bytes = 380L * 1024)
        assertInstanceOf(Outcome.Accepted::class.java, o)
    }

    @Test
    fun `a photo over the ceiling steps the quality down`() {
        val o = ProofEncoding.evaluate(quality = 80, bytes = 900L * 1024)
        assertInstanceOf(Outcome.Retry::class.java, o)
        assertEquals(70, (o as Outcome.Retry).nextQuality)
    }

    /**
     * The case that decides whether a courier can finish a delivery. A camera
     * that produces an awkward image must not strand them holding a proof the
     * app refuses to accept — an oversized photo that reaches the server beats
     * a perfect one that never does.
     */
    @Test
    fun `a stubbornly large photo is accepted oversize rather than refused`() {
        val o = ProofEncoding.evaluate(quality = ProofEncoding.MIN_QUALITY, bytes = 2L * 1024 * 1024)
        assertInstanceOf(Outcome.AcceptedOversize::class.java, o)
        assertEquals(ProofEncoding.MIN_QUALITY, (o as Outcome.AcceptedOversize).quality)
    }

    /**
     * Bounded, and visibly so: the courier is waiting behind this. Four
     * attempts, not an unbounded loop.
     */
    @Test
    fun `the quality ladder is short and never goes below the floor`() {
        val ladder = ProofEncoding.qualityLadder()
        assertEquals(listOf(80, 70, 60, 50), ladder)
        assertTrue(ladder.all { it >= ProofEncoding.MIN_QUALITY })
        assertTrue(ladder.size <= 4, "a courier should not wait through more than four encodes")
    }

    /**
     * Walking the whole ladder must terminate in an accept, whatever the
     * camera produced. A policy that could return Retry forever is the same
     * hang as an unbounded loop.
     */
    @Test
    fun `the ladder always terminates in an accept`() {
        var quality = ProofEncoding.START_QUALITY
        var outcome: Outcome = ProofEncoding.evaluate(quality, 5L * 1024 * 1024)
        var passes = 0

        while (outcome is Outcome.Retry) {
            quality = outcome.nextQuality
            outcome = ProofEncoding.evaluate(quality, 5L * 1024 * 1024)
            passes++
            assertTrue(passes <= 10, "the ladder must not loop")
        }

        assertInstanceOf(Outcome.AcceptedOversize::class.java, outcome)
        assertEquals(ProofEncoding.qualityLadder().size - 1, passes)
    }

    /** The amendment to the POP directive, pinned so it cannot drift silently. */
    @Test
    fun `the target is the WebP figure, not the JPEG one it replaced`() {
        assertEquals(300L * 1024, ProofEncoding.TARGET_BYTES)
        assertEquals(400L * 1024, ProofEncoding.MAX_BYTES)
        assertTrue(
            ProofEncoding.MAX_BYTES < 800L * 1024,
            "the POP directive's JPEG ceiling was 800 KB; WebP should land well under it",
        )
    }
}
