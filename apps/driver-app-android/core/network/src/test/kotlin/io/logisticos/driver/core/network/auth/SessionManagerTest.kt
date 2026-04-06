package io.logisticos.driver.core.network.auth

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.Assertions.*

class SessionManagerTest {

    private val tokenStorage: TokenStorage = mockk(relaxed = true)
    private val sessionManager = SessionManager(tokenStorage)

    @Test
    fun `isLoggedIn returns false when no JWT stored`() {
        every { tokenStorage.getJwt() } returns null
        assertFalse(sessionManager.isLoggedIn())
    }

    @Test
    fun `isLoggedIn returns true when JWT stored`() {
        every { tokenStorage.getJwt() } returns "valid.jwt.token"
        assertTrue(sessionManager.isLoggedIn())
    }

    @Test
    fun `saveTokens stores both jwt and refresh token`() {
        sessionManager.saveTokens(jwt = "jwt123", refreshToken = "refresh456")
        verify { tokenStorage.saveJwt("jwt123") }
        verify { tokenStorage.saveRefreshToken("refresh456") }
    }

    @Test
    fun `clearSession removes both tokens`() {
        sessionManager.clearSession()
        verify { tokenStorage.clearAll() }
    }

    @Test
    fun `isOfflineModeActive returns true when jwt null but refresh token exists`() {
        every { tokenStorage.getJwt() } returns null
        every { tokenStorage.getRefreshToken() } returns "refresh456"
        assertTrue(sessionManager.isOfflineModeActive())
    }
}
