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

class TenantInterceptorTest {
    private val server = MockWebServer()
    private val tokenStorage: TokenStorage = mockk()
    private lateinit var sessionManager: SessionManager

    @BeforeEach
    fun setUp() {
        server.start()
        // tokenStorage.getJwt() must be stubbed before SessionManager construction
        every { tokenStorage.getJwt() } returns null
        sessionManager = SessionManager(tokenStorage)
    }

    @AfterEach fun tearDown() { server.shutdown() }

    @Test
    fun `attaches X-Tenant-ID header when tenantId exists`() {
        every { tokenStorage.getTenantId() } returns "tenant-abc"
        server.enqueue(MockResponse().setResponseCode(200))

        val client = OkHttpClient.Builder()
            .addInterceptor(TenantInterceptor(sessionManager))
            .build()

        client.newCall(Request.Builder().url(server.url("/test")).build()).execute()

        assertEquals("tenant-abc", server.takeRequest().getHeader("X-Tenant-ID"))
    }

    @Test
    fun `omits X-Tenant-ID header when no tenantId`() {
        every { tokenStorage.getTenantId() } returns null
        server.enqueue(MockResponse().setResponseCode(200))

        val client = OkHttpClient.Builder()
            .addInterceptor(TenantInterceptor(sessionManager))
            .build()

        client.newCall(Request.Builder().url(server.url("/test")).build()).execute()

        assertNull(server.takeRequest().getHeader("X-Tenant-ID"))
    }
}
