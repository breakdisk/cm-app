package net.cargomarket.omnideliv.courier.domain

/**
 * Asks for the outbound queue to be drained, some time soon.
 *
 * An interface, and in the domain layer, so the queue can say *when a drain is
 * due* without knowing that a drain is a WorkManager job — and so the rule that
 * every recorded milestone asks for one is testable on the JVM, which is the
 * only place this app's tests can run.
 *
 * Deliberately fire-and-forget: the caller is a courier who has just tapped a
 * button and must not wait on scheduling, let alone on a network.
 */
fun interface SyncScheduler {
    fun kick()
}
