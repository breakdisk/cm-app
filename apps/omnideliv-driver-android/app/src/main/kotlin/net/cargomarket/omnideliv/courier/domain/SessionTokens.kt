package net.cargomarket.omnideliv.courier.domain

/**
 * The session, as the network layer needs to see it.
 *
 * An interface because the real store encrypts at rest against the Android
 * keystore, which does not exist on the JVM — and the rule that matters most
 * here (spend the refresh token exactly once, sign out when it is refused) is
 * only provable in a test that can run.
 */
interface SessionTokens {

    /** The bearer for outgoing calls. Null means signed out. */
    val accessToken: String?

    /**
     * Proves identity when the access token has expired.
     *
     * Identity **rotates** this: every refresh returns a new one and retires
     * the old. A second refresh with a spent token is refused, which is why
     * concurrent 401s must not each try.
     */
    val refreshToken: String?

    /** Store a refreshed pair. A null refresh token leaves the current one alone. */
    fun update(accessToken: String, refreshToken: String?)

    /** End the session. The courier goes back to sign-in. */
    fun signOut()
}
