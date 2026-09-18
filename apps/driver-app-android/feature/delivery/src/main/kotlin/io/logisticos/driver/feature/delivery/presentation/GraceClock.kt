package io.logisticos.driver.feature.delivery.presentation

import java.time.Duration
import java.time.Instant
import java.util.Locale

/** The last ten minutes turn the clock amber, as the design has it. */
const val CLOSING_WINDOW_MS = 10 * 60_000L

enum class GraceTone { Running, Closing, Over }

/**
 * The server's grace deadline as a countdown.
 *
 * It is anchored on the server's own clock (`as_of`) and then advanced by the
 * phone's monotonic clock, never by the phone's wall time — a phone set to the
 * wrong time, or changed while the driver waits, cannot move the deadline.
 */
data class GraceClock(
    /** Milliseconds left when the quote was taken; negative once past. */
    val remainingAtFetchMs: Long,
    /** `SystemClock.elapsedRealtime()` when the quote was taken. */
    val fetchedAtElapsedMs: Long,
) {
    fun remainingMs(nowElapsedMs: Long): Long =
        remainingAtFetchMs - (nowElapsedMs - fetchedAtElapsedMs).coerceAtLeast(0)

    companion object {
        /** Null when no clock runs: not at the stop yet, or switched off. */
        fun from(graceExpiresAt: String?, asOf: String, fetchedAtElapsedMs: Long): GraceClock? {
            if (graceExpiresAt.isNullOrBlank()) return null
            return runCatching {
                val left = Duration.between(Instant.parse(asOf), Instant.parse(graceExpiresAt)).toMillis()
                GraceClock(left, fetchedAtElapsedMs)
            }.getOrNull()
        }
    }
}

fun graceTone(remainingMs: Long): GraceTone = when {
    remainingMs <= 0 -> GraceTone.Over
    remainingMs <= CLOSING_WINDOW_MS -> GraceTone.Closing
    else -> GraceTone.Running
}

/**
 * "44:59" while it runs, rounded up so a fresh 45-minute clock reads 45:00.
 * Past the deadline, how long past: "+3:10". An hour or more: "1:02:03".
 */
fun formatGrace(remainingMs: Long): String {
    val past = remainingMs <= 0
    val seconds = if (past) (-remainingMs) / 1000 else (remainingMs + 999) / 1000
    val h = seconds / 3600
    val m = (seconds % 3600) / 60
    val s = seconds % 60
    val body = if (h > 0) String.format(Locale.US, "%d:%02d:%02d", h, m, s) else String.format(Locale.US, "%d:%02d", m, s)
    return if (past) "+$body" else body
}
