package io.logisticos.driver.feature.delivery.domain

import io.logisticos.driver.core.database.entity.TaskStatus
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

class TaskStateMachineTest {

    @Test
    fun `ASSIGNED can transition to EN_ROUTE`() {
        assertTrue(TaskStateMachine.canTransition(TaskStatus.ASSIGNED, TaskStatus.EN_ROUTE))
    }

    @Test
    fun `EN_ROUTE can transition to ARRIVED`() {
        assertTrue(TaskStateMachine.canTransition(TaskStatus.EN_ROUTE, TaskStatus.ARRIVED))
    }

    @Test
    fun `ARRIVED can transition to IN_PROGRESS`() {
        assertTrue(TaskStateMachine.canTransition(TaskStatus.ARRIVED, TaskStatus.IN_PROGRESS))
    }

    @Test
    fun `IN_PROGRESS can transition to COMPLETED`() {
        assertTrue(TaskStateMachine.canTransition(TaskStatus.IN_PROGRESS, TaskStatus.COMPLETED))
    }

    @Test
    fun `IN_PROGRESS can transition to ATTEMPTED`() {
        assertTrue(TaskStateMachine.canTransition(TaskStatus.IN_PROGRESS, TaskStatus.ATTEMPTED))
    }

    @Test
    fun `COMPLETED cannot transition to any other status`() {
        TaskStatus.entries.forEach { target ->
            if (target != TaskStatus.COMPLETED) {
                assertFalse(TaskStateMachine.canTransition(TaskStatus.COMPLETED, target))
            }
        }
    }

    @Test
    fun `ASSIGNED cannot skip to COMPLETED`() {
        assertFalse(TaskStateMachine.canTransition(TaskStatus.ASSIGNED, TaskStatus.COMPLETED))
    }

    @Test
    fun `IN_PROGRESS can transition to FAILED`() {
        assertTrue(TaskStateMachine.canTransition(TaskStatus.IN_PROGRESS, TaskStatus.FAILED))
    }

    @Test
    fun `ATTEMPTED can transition to IN_PROGRESS`() {
        assertTrue(TaskStateMachine.canTransition(TaskStatus.ATTEMPTED, TaskStatus.IN_PROGRESS))
    }

    @Test
    fun `ATTEMPTED can transition to RETURNED`() {
        assertTrue(TaskStateMachine.canTransition(TaskStatus.ATTEMPTED, TaskStatus.RETURNED))
    }

    @Test
    fun `FAILED can transition to RETURNED`() {
        assertTrue(TaskStateMachine.canTransition(TaskStatus.FAILED, TaskStatus.RETURNED))
    }
}
