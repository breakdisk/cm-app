package io.logisticos.driver.security

import org.junit.jupiter.api.Test
import org.junit.jupiter.api.Assertions.*

class RootCheckerTest {
    @Test
    fun `isRooted returns false for normal device simulation`() {
        val checker = RootChecker(isRooted = false)
        assertFalse(checker.check())
    }

    @Test
    fun `isRooted returns true for rooted device simulation`() {
        val checker = RootChecker(isRooted = true)
        assertTrue(checker.check())
    }
}
