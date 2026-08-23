package net.cargomarket.omnideliv.courier.data

import com.jakewharton.retrofit2.converter.kotlinx.serialization.asConverterFactory
import net.cargomarket.omnideliv.courier.domain.SessionTokens
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import retrofit2.Retrofit
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * The access token lives one hour and a shift is longer than that.
 *
 * Until this existed the app held only an access token, so every courier
 * working past the hour met a wall of 401s: the manifest stopped refreshing,
 * offers stopped arriving, and the outbound queue halted. `SyncDecision.Halt`
 * kept that from *destroying* a delivery, but it could not let anyone keep
 * working — the courier had to notice and sign in again, mid-shift, on a
 * screen that gave them no reason to.
 *
 * Identity has always returned a `refresh_token` on OTP verify; the app simply
 * dropped it on the floor.
 */
class RefreshAuthenticatorTest {

    private lateinit var server: MockWebServer
    private lateinit var client: OkHttpClient

    /** Test double for the encrypted store, which needs a real Android keystore. */
    private class FakeTokens(
        var access: String? = "expired-token",
        var refresh: String? = "refresh-token",
    ) : SessionTokens {
        var signedOut = false
        var updates = 0

        override val accessToken: String? get() = access
        override val refreshToken: String? get() = refresh

        override fun update(accessToken: String, refreshToken: String?) {
            access = accessToken
            if (refreshToken != null) refresh = refreshToken
            updates++
        }

        override fun signOut() {
            access = null
            refresh = null
            signedOut = true
        }
    }

    private val tokens = FakeTokens()

    @BeforeEach
    fun setUp() {
        server = MockWebServer()
        server.start()

        val bare = OkHttpClient.Builder().build()
        val refreshApi = Retrofit.Builder()
            .baseUrl(server.url("/"))
            .client(bare)
            .addConverterFactory(CourierJson.asConverterFactory("application/json".toMediaType()))
            .build()
            .create(RefreshApi::class.java)

        client = OkHttpClient.Builder()
            // Mirrors NetworkModule, `v1/auth/*` exemption included — that
            // exemption is what the third test below exercises.
            .addInterceptor { chain ->
                val token = tokens.accessToken
                val isAuth = chain.request().url.encodedPath.contains("/v1/auth/")
                val request = if (token == null || isAuth) {
                    chain.request()
                } else {
                    chain.request().newBuilder().header("Authorization", "Bearer $token").build()
                }
                chain.proceed(request)
            }
            .authenticator(RefreshAuthenticator(tokens) { refreshApi })
            .build()
    }

    @AfterEach
    fun tearDown() = server.shutdown()

    private fun call(path: String = "/v1/field-ops/assignments/mine") =
        client.newCall(Request.Builder().url(server.url(path)).build()).execute()

    /** Identity wraps auth responses in `{"data": ...}` — refresh included. */
    private fun refreshBody(access: String, refresh: String) = MockResponse()
        .setResponseCode(200)
        .setBody(
            """{"data":{"access_token":"$access","refresh_token":"$refresh",""" +
                """"expires_in":3600,"token_type":"Bearer"}}""",
        )

    @Test
    fun `an expired access token is refreshed and the original request retried`() {
        server.enqueue(MockResponse().setResponseCode(401))
        server.enqueue(refreshBody("fresh-token", "next-refresh"))
        server.enqueue(MockResponse().setResponseCode(200).setBody("""{"offers":[]}"""))

        val response = call()

        assertEquals(200, response.code)
        assertEquals(3, server.requestCount)

        val first = server.takeRequest()
        assertEquals("Bearer expired-token", first.getHeader("Authorization"))

        val refresh = server.takeRequest()
        assertEquals("/v1/auth/refresh", refresh.path)
        assertTrue(
            refresh.body.readUtf8().contains("refresh-token"),
            "the refresh token, not the dead access token, is what proves identity here",
        )

        val retry = server.takeRequest()
        assertEquals("Bearer fresh-token", retry.getHeader("Authorization"))

        // Both halves are stored: identity rotates the refresh token, and
        // keeping the old one would work exactly once more.
        assertEquals("fresh-token", tokens.access)
        assertEquals("next-refresh", tokens.refresh)
    }

    /**
     * The refresh token expires too. When it does the session is genuinely
     * over, and the courier must be returned to sign-in rather than left
     * looking at a screen that quietly never updates again.
     */
    @Test
    fun `a refused refresh signs the courier out instead of looping`() {
        server.enqueue(MockResponse().setResponseCode(401))
        server.enqueue(MockResponse().setResponseCode(401).setBody("""{"error":"expired"}"""))

        val response = call()

        assertEquals(401, response.code)
        assertEquals(2, server.requestCount, "one attempt, then stop")
        assertTrue(tokens.signedOut)
        assertNull(tokens.access)
    }

    /**
     * A 401 on a request that never carried a token is not a stale session —
     * it is sign-in itself being refused. Refreshing there would spend the
     * refresh token on someone else's mistake and could loop.
     */
    @Test
    fun `a request that carried no token is never refreshed`() {
        // A signed-in courier re-verifying an OTP: the session is live and the
        // refresh token is spendable, but this request carries no bearer.
        // Refreshing here would burn a single-use token on a 401 that says
        // nothing about the session.
        server.enqueue(MockResponse().setResponseCode(401))

        val response = call("/v1/auth/otp/verify")

        assertEquals(401, response.code)
        assertEquals(1, server.requestCount)
        assertEquals(0, tokens.updates, "no refresh may be attempted")
        assertEquals("refresh-token", tokens.refresh, "the refresh token must be left unspent")
        assertTrue(!tokens.signedOut, "a 401 on an auth endpoint is not the session ending")
    }

    /**
     * Every screen polls. When a token dies, several requests meet the 401 at
     * once — and identity rotates the refresh token, so a second refresh with
     * the one already spent is refused and signs the courier out mid-shift.
     * Exactly one refresh per expiry.
     */
    @Test
    fun `concurrent unauthorized requests refresh once between them`() {
        repeat(2) { server.enqueue(MockResponse().setResponseCode(401)) }
        server.enqueue(refreshBody("fresh-token", "next-refresh"))
        repeat(2) { server.enqueue(MockResponse().setResponseCode(200).setBody("{}")) }

        val start = CountDownLatch(1)
        val done = CountDownLatch(2)
        val codes = java.util.Collections.synchronizedList(mutableListOf<Int>())
        repeat(2) {
            Thread {
                start.await()
                codes += call().code
                done.countDown()
            }.start()
        }
        start.countDown()
        assertTrue(done.await(10, TimeUnit.SECONDS), "both calls must finish")

        assertEquals(listOf(200, 200), codes.sorted())
        assertEquals(1, tokens.updates, "the refresh token may only be spent once")
    }
}
