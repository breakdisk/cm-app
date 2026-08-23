package net.cargomarket.omnideliv.courier.data

import net.cargomarket.omnideliv.courier.domain.SessionTokens
import okhttp3.Authenticator
import okhttp3.Request
import okhttp3.Response
import okhttp3.Route
import retrofit2.Call
import retrofit2.http.Body
import retrofit2.http.POST

/**
 * Exchange the refresh token for a new session when the access token dies.
 *
 * The access token lives one hour (`AUTH__JWT_EXPIRY_SECONDS: 3600`) and a
 * courier's shift is longer than that. Before this, the app held only the
 * access token — identity had always returned a refresh token and the app
 * dropped it — so an hour in, the manifest stopped refreshing, offers stopped
 * arriving and the outbound queue halted, with nothing on screen explaining
 * why.
 *
 * An OkHttp [Authenticator] rather than an interceptor: OkHttp calls it only on
 * a 401, hands over the response that failed, and retries the original request
 * with whatever this returns — including its body, which an interceptor
 * retrying by hand has to rebuild.
 *
 * `refreshApi` is a lambda so it can be built on a **client without this
 * authenticator attached**. Refreshing through the same client would send the
 * refresh call back through here on its own 401, forever.
 */
class RefreshAuthenticator(
    private val tokens: SessionTokens,
    private val refreshApi: () -> RefreshApi,
) : Authenticator {

    override fun authenticate(route: Route?, response: Response): Request? {
        // A 401 on a request that never carried a token is sign-in being
        // refused, not a stale session. Refreshing there would spend the token
        // on someone else's problem.
        val stale = response.request.header(AUTHORIZATION) ?: return null

        // One attempt. If the retry is refused too, the session is not the
        // problem and looping would only turn a failure into a hang.
        if (response.priorResponse != null) return null

        synchronized(this) {
            // Another thread may have refreshed while this one waited on the
            // lock. Identity rotates the refresh token, so a second exchange
            // would be refused and would sign a working courier out mid-shift.
            val current = tokens.accessToken
            if (current != null && bearer(current) != stale) return retry(response.request, current)

            val refresh = tokens.refreshToken ?: run {
                tokens.signOut()
                return null
            }

            val body = runCatching { refreshApi().refresh(RefreshRequest(refresh)).execute() }
                .getOrNull()
                ?.takeIf { it.isSuccessful }
                ?.body()
                ?.data

            if (body == null) {
                // The refresh token has expired or been retired. The session is
                // genuinely over: return to sign-in rather than leave a screen
                // that quietly never updates again.
                tokens.signOut()
                return null
            }

            tokens.update(body.accessToken, body.refreshToken)
            return retry(response.request, body.accessToken)
        }
    }

    private fun retry(request: Request, token: String): Request =
        request.newBuilder().header(AUTHORIZATION, bearer(token)).build()

    private fun bearer(token: String) = "Bearer $token"

    private companion object {
        const val AUTHORIZATION = "Authorization"
    }
}

/**
 * The refresh call, deliberately blocking.
 *
 * An [Authenticator] runs on OkHttp's own thread and cannot suspend, so this is
 * a `Call` rather than a `suspend fun`. It is the one place in this client that
 * is not a coroutine.
 *
 * The response is enveloped — `{"data": {...}}` — like the other two auth
 * endpoints and unlike everything else this app calls. That envelope is why
 * sign-in once reported "we could not reach the server" against a 200.
 */
interface RefreshApi {
    @POST("v1/auth/refresh")
    fun refresh(@Body body: RefreshRequest): Call<AuthEnvelope>
}
