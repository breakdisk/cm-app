package io.logisticos.driver.core.network.interceptor

import io.logisticos.driver.core.network.auth.SessionManager
import io.logisticos.driver.core.network.auth.TokenStorage
import io.mockk.every
import io.mockk.mockk
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.Assertions.*

class AuthInterceptorTest {
    private val server = MockWebServer()
    private val tokenStorage: TokenStorage = mockk()
    private lateinit var sessionManager: SessionManager

    @BeforeEach
    fun setUp() {
        server.start()
        // tokenStorage stub must be set before SessionManager is constructed,
        // because the constructor reads tokenStorage.getJwt() into cachedJwt.
        every { tokenStorage.getJwt() } returns null
        sessionManager = SessionManager(tokenStorage)
    }

    @AfterEach fun tearDown() { server.shutdown() }

    @Test
    fun `attaches Authorization header when JWT exists`() {
        // Directly seed the cache via saveTokens so the in-memory cache is populated
        every { tokenStorage.saveJwt(any()) } returns Unit
        every { tokenStorage.saveRefreshToken(any()) } returns Unit
        sessionManager.saveTokens(jwt = "test.jwt.token", refreshToken = "rt")

        server.enqueue(MockResponse().setResponseCode(200))

        val client = OkHttpClient.Builder()
            .addInterceptor(AuthInterceptor(sessionManager))
            .build()

        client.newCall(Request.Builder().url(server.url("/test")).build()).execute()

        val request = server.takeRequest()
        assertEquals("Bearer test.jwt.token", request.getHeader("Authorization"))
    }

    @Test
    fun `does not attach header when no JWT`() {
        // sessionManager already has null JWT from setUp
        server.enqueue(MockResponse().setResponseCode(401))

        val client = OkHttpClient.Builder()
            .addInterceptor(AuthInterceptor(sessionManager))
            .build()

        client.newCall(Request.Builder().url(server.url("/test")).build()).execute()

        val request = server.takeRequest()
        assertNull(request.getHeader("Authorization"))
    }
}
