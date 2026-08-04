package io.logisticos.driver.core.network.auth

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.Assertions.*

/**
 * [SessionManager] reads the persisted JWT exactly once, in its constructor, into
 * an in-memory cache; thereafter only `saveTokens`/`clearSession` mutate it. That
 * is deliberate — it keeps EncryptedSharedPreferences decryption off hot paths,
 * and the real instance is a Hilt `@Singleton` built once at app start.
 *
 * It does mean stubbing `getJwt()` after construction has no effect. These tests
 * therefore build the subject *inside* each test, after the storage stub is in
 * place, rather than in a field initializer.
 */
class SessionManagerTest {

    private val tokenStorage: TokenStorage = mockk(relaxed = true)

    /** Build the subject only once the storage stubs are set. */
    private fun sessionManager() = SessionManager(tokenStorage)

    @Test
    fun `isLoggedIn returns false when no JWT stored`() {
        every { tokenStorage.getJwt() } returns null
        assertFalse(sessionManager().isLoggedIn())
    }

    @Test
    fun `isLoggedIn returns true when JWT stored`() {
        every { tokenStorage.getJwt() } returns "valid.jwt.token"
        assertTrue(sessionManager().isLoggedIn())
    }

    @Test
    fun `isLoggedIn treats a blank token as no session`() {
        // A relaxed mock returns "" for String, and a corrupted pref write could
        // do the same in production. An empty token is not a credential: reporting
        // a session here would let the sync worker fire unauthenticated requests
        // and would suppress offline-mode detection.
        every { tokenStorage.getJwt() } returns ""
        assertFalse(sessionManager().isLoggedIn())
    }

    @Test
    fun `saveTokens stores both jwt and refresh token`() {
        val sm = sessionManager()
        sm.saveTokens(jwt = "jwt123", refreshToken = "refresh456")
        verify { tokenStorage.saveJwt("jwt123") }
        verify { tokenStorage.saveRefreshToken("refresh456") }
    }

    @Test
    fun `saveTokens updates the cached jwt so isLoggedIn flips without a reload`() {
        every { tokenStorage.getJwt() } returns null
        val sm = sessionManager()
        assertFalse(sm.isLoggedIn())

        sm.saveTokens(jwt = "jwt123", refreshToken = "refresh456")

        // Proves the cache is write-through: storage is never re-read.
        assertTrue(sm.isLoggedIn())
    }

    @Test
    fun `clearSession invokes clearAll on storage and drops the cached jwt`() {
        every { tokenStorage.getJwt() } returns "valid.jwt.token"
        val sm = sessionManager()
        assertTrue(sm.isLoggedIn())

        sm.clearSession()

        verify { tokenStorage.clearAll() }
        assertFalse(sm.isLoggedIn())
    }

    @Test
    fun `isOfflineModeActive returns true when jwt null but refresh token exists`() {
        every { tokenStorage.getJwt() } returns null
        every { tokenStorage.getRefreshToken() } returns "refresh456"
        assertTrue(sessionManager().isOfflineModeActive())
    }

    @Test
    fun `isOfflineModeActive returns false when a valid jwt is held`() {
        every { tokenStorage.getJwt() } returns "valid.jwt.token"
        every { tokenStorage.getRefreshToken() } returns "refresh456"
        assertFalse(sessionManager().isOfflineModeActive())
    }

    @Test
    fun `isOfflineModeActive returns false with neither token`() {
        every { tokenStorage.getJwt() } returns null
        every { tokenStorage.getRefreshToken() } returns null
        assertFalse(sessionManager().isOfflineModeActive())
    }
}
