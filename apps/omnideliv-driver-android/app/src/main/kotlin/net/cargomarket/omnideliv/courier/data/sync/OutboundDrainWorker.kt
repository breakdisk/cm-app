package net.cargomarket.omnideliv.courier.data.sync

import android.content.Context
import androidx.hilt.work.HiltWorker
import androidx.work.CoroutineWorker
import androidx.work.WorkerParameters
import dagger.assisted.Assisted
import dagger.assisted.AssistedInject
import net.cargomarket.omnideliv.courier.data.OutboundRepository

/**
 * Gets the outbound queue to the server when nobody is looking at the app.
 *
 * Before this existed the queue drained only from the manifest screen, right
 * after a milestone — so a sync that failed because the courier was in a
 * basement waited for the *next* delivery to be attempted again, and the last
 * delivery of a shift waited until the next shift. A courier who finished their
 * final drop in a car park and put the phone away was owed money the platform
 * had never been told about.
 *
 * The work itself is entirely [OutboundRepository.drain]'s: ordering,
 * attempt-counting, parking and the proof-before-milestone rule all live there,
 * where they are testable on the JVM. This class is scheduling and nothing
 * else.
 */
@HiltWorker
class OutboundDrainWorker @AssistedInject constructor(
    @Assisted appContext: Context,
    @Assisted params: WorkerParameters,
    private val outbound: OutboundRepository,
) : CoroutineWorker(appContext, params) {

    override suspend fun doWork(): Result {
        // A throw here is a bug, not a queue state: the repository already
        // converts every network failure into a decision. Retrying is still the
        // right answer — the rows are on disk either way, and failing the work
        // would cancel anything appended behind it.
        val drained = runCatching { outbound.drain() }.getOrDefault(false)

        // Anything short of a clean sweep comes back with backoff: a row that
        // is still waiting, a session that needs signing in again, or a row
        // that parked and left the queue no longer clean. The next pass costs
        // one query when there is nothing left to do.
        return if (drained) Result.success() else Result.retry()
    }
}
