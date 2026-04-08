package io.logisticos.driver.core.network.authenticator

import io.logisticos.driver.core.network.auth.SessionManager
import io.logisticos.driver.core.network.model.RefreshRequest
import io.logisticos.driver.core.network.service.IdentityApiService
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import okhttp3.Authenticator
import okhttp3.Request
import okhttp3.Response
import okhttp3.Route
import javax.inject.Inject
import javax.inject.Provider

class TokenAuthenticator @Inject constructor(
    private val sessionManager: SessionManager,
    // Provider<> breaks circular dependency between NetworkModule and Authenticator
    private val identityApiServiceProvider: Provider<IdentityApiService>
) : Authenticator {

    private val refreshMutex = Mutex()

    override fun authenticate(route: Route?, response: Response): Request? {
        // Only retry once — prevent infinite 401 loop
        if (response.request.header("Authorization-Retry") != null) return null

        val refreshToken = sessionManager.getRefreshToken() ?: run {
            sessionManager.clearSession()
            return null
        }

        return runBlocking {
            refreshMutex.withLock {
                // Check if another coroutine already refreshed the token while we waited
                val currentJwt = sessionManager.getJwt()
                val requestJwt = response.request.header("Authorization")
                    ?.removePrefix("Bearer ")
                if (currentJwt != null && currentJwt != requestJwt) {
                    // Token was already rotated by a concurrent request — retry with new token
                    return@runBlocking response.request.newBuilder()
                        .header("Authorization", "Bearer $currentJwt")
                        .header("Authorization-Retry", "true")
                        .build()
                }

                try {
                    val tokenResponse = identityApiServiceProvider.get()
                        .refreshToken(RefreshRequest(refreshToken = refreshToken))
                    // Token rotation: save new JWT and new Refresh Token
                    sessionManager.saveTokens(
                        jwt = tokenResponse.jwt,
                        refreshToken = tokenResponse.refreshToken
                    )
                    response.request.newBuilder()
                        .header("Authorization", "Bearer ${tokenResponse.jwt}")
                        .header("Authorization-Retry", "true")
                        .build()
                } catch (e: Exception) {
                    sessionManager.clearSession()
                    null
                }
            }
        }
    }
}
