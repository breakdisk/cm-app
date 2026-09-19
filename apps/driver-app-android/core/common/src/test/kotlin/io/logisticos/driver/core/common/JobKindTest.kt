package io.logisticos.driver.core.common

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNotNull
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Test

class JobKindTest {
    @Test
    fun `every home-move slot has a headline and a line of detail`() {
        for (k in listOf("home_move", "home_move_joint", "home_move_support", "home_move_emergency")) {
            assertNotNull(JobKind.homeHeadline(k), k)
            assertNotNull(JobKind.homeDetail(k), k)
        }
        assertEquals("JOINT MISSION · 2 TRUCKS", JobKind.homeHeadline("home_move_joint"))
    }

    @Test
    fun `freight loads are named as before`() {
        assertNull(JobKind.homeHeadline("parcel"))
        assertNull(JobKind.homeHeadline("large"))
    }

    @Test
    fun `roles read in words`() {
        assertEquals("Mission Captain", JobKind.roleLabel("captain"))
        assertEquals("Support Lead", JobKind.roleLabel("support"))
        assertEquals("Lead", JobKind.roleLabel("sole"))
    }
}
