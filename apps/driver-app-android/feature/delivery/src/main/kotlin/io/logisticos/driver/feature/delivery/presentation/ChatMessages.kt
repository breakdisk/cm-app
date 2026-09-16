package io.logisticos.driver.feature.delivery.presentation

import io.logisticos.driver.core.network.service.JobMessageItem
import java.time.Instant

/**
 * Sorting key for a message. A timestamp this app cannot read sorts last rather
 * than being dropped — a message the driver cannot see is worse than one in an
 * odd position.
 */
private fun sentAt(message: JobMessageItem): Long =
    runCatching { Instant.parse(message.createdAt).toEpochMilli() }.getOrDefault(Long.MAX_VALUE)

/**
 * Folds a poll's messages into what is already on screen: each id once, oldest
 * first. A message this phone just sent is already there when the poll returns
 * it, and must not appear twice.
 */
fun mergeMessages(current: List<JobMessageItem>, incoming: List<JobMessageItem>): List<JobMessageItem> =
    (current + incoming)
        .associateBy { it.id }
        .values
        .sortedWith(compareBy({ sentAt(it) }, { it.id }))
