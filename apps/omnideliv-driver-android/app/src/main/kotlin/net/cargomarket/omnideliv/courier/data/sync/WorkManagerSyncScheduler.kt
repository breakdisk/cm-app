package net.cargomarket.omnideliv.courier.data.sync

import android.content.Context
import androidx.work.BackoffPolicy
import androidx.work.Constraints
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import dagger.hilt.android.qualifiers.ApplicationContext
import net.cargomarket.omnideliv.courier.domain.SyncScheduler
import java.util.concurrent.TimeUnit
import javax.inject.Inject
import javax.inject.Singleton

/**
 * [SyncScheduler] over WorkManager — the only part of the sync design that
 * knows Android exists.
 */
@Singleton
class WorkManagerSyncScheduler @Inject constructor(
    @ApplicationContext private val context: Context,
) : SyncScheduler {

    /**
     * A drain is due because a courier just recorded something.
     *
     * `APPEND_OR_REPLACE`, not `REPLACE`: replacing cancels a pass that may be
     * mid-request, which is precisely how a milestone the server accepted loses
     * its response and comes back as a retry. Not `KEEP` either — a row
     * inserted a moment after a running pass read the queue would be dropped by
     * KEEP and left to the fifteen-minute safety net.
     *
     * Appending costs a few redundant passes over an empty queue, which is one
     * database query each.
     */
    override fun kick() {
        val request = OneTimeWorkRequestBuilder<OutboundDrainWorker>()
            .setConstraints(networkConstraints())
            .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, BACKOFF_SECONDS, TimeUnit.SECONDS)
            .build()

        WorkManager.getInstance(context)
            .enqueueUniqueWork(KICK_WORK, ExistingWorkPolicy.APPEND_OR_REPLACE, request)
    }

    companion object {
        private const val KICK_WORK = "outbound_drain_kick"
        private const val SAFETY_NET_WORK = "outbound_drain_safety_net"
        private const val BACKOFF_SECONDS = 30L

        private fun networkConstraints() = Constraints.Builder()
            // Not battery or idle constrained. This queue holds deliveries the
            // platform has not been told about; a courier's phone at 14 % is
            // not a reason to keep the money waiting.
            .setRequiredNetworkType(NetworkType.CONNECTED)
            .build()

        /**
         * The recurring floor, scheduled once at application start.
         *
         * Covers everything a kick cannot: the process was killed before its
         * one-shot ran, Doze deferred it past the shift, or the courier signed
         * in again after the queue had already halted on an expired session.
         * Fifteen minutes is WorkManager's own minimum period.
         *
         * `KEEP` so a relaunch does not restart the interval.
         */
        fun scheduleSafetyNet(context: Context) {
            val request = PeriodicWorkRequestBuilder<OutboundDrainWorker>(15, TimeUnit.MINUTES)
                .setConstraints(networkConstraints())
                .build()

            WorkManager.getInstance(context).enqueueUniquePeriodicWork(
                SAFETY_NET_WORK,
                ExistingPeriodicWorkPolicy.KEEP,
                request,
            )
        }
    }
}
