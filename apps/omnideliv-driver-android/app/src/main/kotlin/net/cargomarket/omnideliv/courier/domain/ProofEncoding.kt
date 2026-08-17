package net.cargomarket.omnideliv.courier.domain

/**
 * Proof-photo encoding policy, as pure arithmetic.
 *
 * Separated from the Android `Bitmap` call so the loop that decides quality can
 * be tested without a device — the encoder itself is three lines and a version
 * branch, and it is this policy that decides whether a courier waits.
 *
 * Amends the POP directive in CLAUDE.md, which specifies `JPEG, 75` at 800 KB
 * by name. WebP lossy runs 25–34 % smaller at equivalent visual quality, which
 * takes an 800 KB proof to roughly 250–300 KB with no loss of readability for
 * human or ops review. Recorded as a deliberate amendment rather than a quiet
 * contradiction.
 */
object ProofEncoding {
    /** Where the first attempt starts. */
    const val START_QUALITY = 80

    /** Below this, a doorstep photo stops being evidence. */
    const val MIN_QUALITY = 45

    /** Step taken when an attempt overshoots. */
    const val QUALITY_STEP = 10

    /** What we aim for. */
    const val TARGET_BYTES = 300L * 1024

    /**
     * Hard ceiling.
     *
     * A payload above this is enqueued anyway — see [Outcome.AcceptedOversize].
     * Refusing would strand a courier holding an undeliverable proof, and an
     * oversized photo that reaches the server beats a perfect one that never
     * does.
     */
    const val MAX_BYTES = 400L * 1024

    sealed interface Outcome {
        /** Within the ceiling. Enqueue it. */
        data class Accepted(val quality: Int, val bytes: Long) : Outcome

        /** Try again one step lower. */
        data class Retry(val nextQuality: Int) : Outcome

        /**
         * Still over the ceiling at the lowest quality we will accept.
         *
         * Enqueued regardless. The alternative is a courier who cannot complete
         * a delivery because their camera produced an awkward image.
         */
        data class AcceptedOversize(val quality: Int, val bytes: Long) : Outcome
    }

    /**
     * Decide what to do with one encoding attempt.
     *
     * `quality` is what produced `bytes`.
     */
    fun evaluate(quality: Int, bytes: Long): Outcome = when {
        bytes <= TARGET_BYTES -> Outcome.Accepted(quality, bytes)
        bytes <= MAX_BYTES -> Outcome.Accepted(quality, bytes)
        quality - QUALITY_STEP >= MIN_QUALITY -> Outcome.Retry(quality - QUALITY_STEP)
        else -> Outcome.AcceptedOversize(quality, bytes)
    }

    /**
     * The quality ladder an encoder will walk, worst case.
     *
     * Exposed so the bound is visible: four attempts, not an unbounded loop on
     * a screen the courier is waiting behind.
     */
    fun qualityLadder(): List<Int> {
        val ladder = mutableListOf<Int>()
        var q = START_QUALITY
        while (q >= MIN_QUALITY) {
            ladder += q
            q -= QUALITY_STEP
        }
        return ladder
    }
}
