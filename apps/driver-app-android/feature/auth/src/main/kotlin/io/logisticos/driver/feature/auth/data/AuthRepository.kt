package io.logisticos.driver.feature.auth.data

import io.logisticos.driver.core.network.auth.SessionManager
import io.logisticos.driver.core.network.service.IdentityApiService
import io.logisticos.driver.core.network.service.OtpSendRequest
import io.logisticos.driver.core.network.service.OtpVerifyRequest
import okhttp3.OkHttpClient
import javax.inject.Inject
import javax.inject.Named
import javax.inject.Singleton

@Singleton
class AuthRepository @Inject constructor(
    private val apiService: IdentityApiService,
    private val sessionManager: SessionManager,
    /** Shared OkHttpClient — used on logout to cancel all in-flight requests so
     *  that pending TokenAuthenticator.runBlocking calls don't deadlock the
     *  OkHttp dispatcher and prevent the next sendOtp call from being dispatched. */
    private val okHttpClient: OkHttpClient,
    /** True only in debug builds — gates the 123456 OTP shortcut for local development. */
    @Named("dev_bypass_enabled") private val devBypassEnabled: Boolean,
) {
    /**
     * Sends an OTP to the given phone/email for the specified tenant.
     *
     * @param tenantSlug  Runtime slug resolved from invite link or company code.
     *                    Must not be blank — the identity service rejects requests
     *                    with no tenant context.
     */
    suspend fun sendOtp(
        phone: String? = null,
        email: String? = null,
        tenantSlug: String,
    ): Result<Unit> {
        // runCatching is intentionally avoided here: it catches CancellationException,
        // breaking structured concurrency and leaving isLoading=true if the coroutine
        // is cancelled mid-flight (e.g. screen rotation while the call is in flight).
        return try {
            apiService.sendOtp(
                OtpSendRequest(phone = phone, email = email, tenantSlug = tenantSlug, role = "driver")
            )
            Result.success(Unit)
        } catch (e: kotlinx.coroutines.CancellationException) {
            throw e   // must not swallow — propagate so structured concurrency works
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    /**
     * Verifies the OTP and persists the session tokens.
     * On success, saves `tenantSlug` to SessionManager so dispatch queries
     * resolve the correct tenant for this driver going forward.
     */
    suspend fun verifyOtp(
        phone: String? = null,
        otp: String,
        email: String? = null,
        tenantSlug: String,
    ): Result<Unit> {
        if (devBypassEnabled && otp == "123456") {
            sessionManager.saveTokens(jwt = "dev-token", refreshToken = "dev-refresh")
            sessionManager.saveTenantId(tenantSlug)
            sessionManager.saveTenantSlug(tenantSlug)
            sessionManager.saveDriverId("dev-driver-id")
            return Result.success(Unit)
        }
        return try {
            val response = apiService.verifyOtp(
                OtpVerifyRequest(phone = phone, email = email, otp = otp, tenantSlug = tenantSlug, role = "driver")
            ).data
            sessionManager.saveTokens(jwt = response.jwt, refreshToken = response.refreshToken)
            sessionManager.saveTenantId(response.tenantId)
            sessionManager.saveTenantSlug(tenantSlug)
            sessionManager.saveDriverId(response.driverId)
            // FCM token registration is handled by MainViewModel.onAuthSuccess() via
            // addOnSuccessListener (non-blocking). Do NOT await it here — Firebase token
            // fetch can stall indefinitely on first launch and would keep isLoading=true.
            Result.success(Unit)
        } catch (e: kotlinx.coroutines.CancellationException) {
            throw e
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    fun isLoggedIn(): Boolean = sessionManager.isLoggedIn()
    fun isOfflineModeActive(): Boolean = sessionManager.isOfflineModeActive()

    /**
     * Logs the driver out.
     *
     * Cancels ALL in-flight OkHttp calls BEFORE clearing the session.
     * This is critical to prevent a post-logout spinner deadlock:
     *
     *   1. Background calls (location updates, sync) that received a 401 enter
     *      TokenAuthenticator.authenticate() → runBlocking { refreshMutex.withLock { ... } }.
     *   2. Inside runBlocking, they enqueue a refreshToken() HTTP call through the
     *      SAME OkHttp dispatcher.
     *   3. If maxRequestsPerHost (default 5) slots are all occupied by step-1 threads,
     *      the refresh calls queue forever — and so does the next sendOtp request.
     *
     * Calling dispatcher.cancelAll() causes the in-progress socket reads to throw
     * IOException immediately, unwinding the runBlocking blocks and freeing the slots
     * before the new Activity's sendOtp call is enqueued.
     */
    fun logout() {
        okHttpClient.dispatcher.cancelAll()
        sessionManager.clearSession()
    }
}
