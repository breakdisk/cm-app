package io.logisticos.driver.feature.assignment.data

import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.core.network.service.RejectAssignmentRequest
import javax.inject.Inject

class AssignmentRepository @Inject constructor(
    private val api: DriverOpsApiService,
) {
    /**
     * Accept a dispatch assignment. Returns [Result.success] on HTTP 200/204.
     * The backend flips the assignment status → 'accepted' and signals the
     * dispatch engine to remove it from the pending queue.
     */
    suspend fun accept(assignmentId: String): Result<Unit> = runCatching {
        api.acceptAssignment(assignmentId)
    }

    /**
     * Reject a dispatch assignment with a driver-supplied reason.
     * The backend marks the assignment 'rejected', removes the unique constraint
     * block on the driver, and re-queues the shipment for re-dispatch.
     */
    suspend fun reject(assignmentId: String, reason: String): Result<Unit> = runCatching {
        api.rejectAssignment(assignmentId, RejectAssignmentRequest(reason))
    }
}
