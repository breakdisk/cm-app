package io.logisticos.driver.feature.delivery.domain

import io.logisticos.driver.core.database.entity.TaskStatus

object TaskStateMachine {
    private val validTransitions: Map<TaskStatus, Set<TaskStatus>> = mapOf(
        // Delivery tasks follow: ASSIGNED → EN_ROUTE → ARRIVED → IN_PROGRESS → COMPLETED
        // Pickup tasks skip the arrival screen and jump directly to IN_PROGRESS from
        // ASSIGNED or EN_ROUTE — no explicit arrival confirmation step exists for pickups.
        TaskStatus.ASSIGNED    to setOf(TaskStatus.EN_ROUTE, TaskStatus.IN_PROGRESS),
        TaskStatus.EN_ROUTE    to setOf(TaskStatus.ARRIVED,  TaskStatus.IN_PROGRESS),
        TaskStatus.ARRIVED     to setOf(TaskStatus.IN_PROGRESS),
        TaskStatus.IN_PROGRESS to setOf(TaskStatus.COMPLETED, TaskStatus.ATTEMPTED, TaskStatus.FAILED),
        TaskStatus.ATTEMPTED   to setOf(TaskStatus.IN_PROGRESS, TaskStatus.RETURNED),
        TaskStatus.FAILED      to setOf(TaskStatus.RETURNED),
        TaskStatus.COMPLETED   to emptySet(),
        TaskStatus.RETURNED    to emptySet()
    )

    fun canTransition(from: TaskStatus, to: TaskStatus): Boolean =
        validTransitions[from]?.contains(to) == true
}
