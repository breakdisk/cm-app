package net.cargomarket.omnideliv.courier.domain

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonPrimitive

/**
 * What a courier is shown before they claim.
 *
 * The server sends this as an opaque, versioned blob that `field-ops` stores and
 * forwards without reading. This app is its only interpreter, which is exactly
 * why parsing has to be defensive: the backend can ship a `v2` before this APK
 * is updated, and a courier's inbox going blank because a number moved is worse
 * than an inbox missing one line of detail.
 *
 * Everything is optional and nothing throws. An absent card is a legitimate
 * state — a product may offer work without describing it, and the pay and the
 * cash figure ride on the assignment itself rather than in here.
 */
data class OfferCard(
    /** Doors, not legs: pickups plus the single dropoff. */
    val stops: Int?,
    val pickups: Int?,
    val distanceM: Int?,
    val deadlineHintMins: Int?,
    val vendors: List<String>,
    val verticals: List<String>,
    val temperature: List<String>,
    /** True when this job was offered before and nobody took it. */
    val isRetry: Boolean,
    /**
     * The card declared a version this build does not know.
     *
     * Kept rather than discarded so the UI can say "some detail may be missing"
     * instead of quietly presenting a partial card as the whole picture.
     */
    val unknownVersion: Boolean,
) {
    /** Nothing worth drawing. Callers render the bare pay figure instead. */
    fun isEmpty(): Boolean =
        stops == null && pickups == null && distanceM == null &&
            vendors.isEmpty() && verticals.isEmpty()

    /** e.g. "3 stops · 4.2 km". Omits any part the card did not carry. */
    fun headline(): String {
        val parts = buildList {
            stops?.let { add(if (it == 1) "1 stop" else "$it stops") }
            distanceM?.let { add(formatKm(it)) }
        }
        return parts.joinToString(" · ")
    }
}

/** The one version this build understands. */
const val OFFER_CARD_VERSION = 1

private fun formatKm(metres: Int): String {
    // Integer arithmetic: a courier reads "4.2 km", and floating point here
    // would be a rounding decision made twice for no benefit.
    val tenths = (metres + 50) / 100
    return "${tenths / 10}.${tenths % 10} km"
}

private fun JsonObject.intOrNull(key: String): Int? =
    runCatching { this[key]?.jsonPrimitive?.content?.toIntOrNull() }.getOrNull()

private fun JsonObject.stringList(key: String): List<String> =
    runCatching {
        this[key]?.jsonArray?.mapNotNull { it.jsonPrimitive.content.takeIf(String::isNotBlank) }
            ?: emptyList()
    }.getOrElse { emptyList() }

/**
 * Read a card off the wire.
 *
 * `null` in, `null` out — an offer without a card is normal. Anything malformed
 * also yields `null` rather than an exception: this runs while rendering a list,
 * and one bad card must not take the whole inbox down with it.
 */
fun parseOfferCard(raw: JsonElement?): OfferCard? {
    val obj = raw as? JsonObject ?: return null
    val version = obj.intOrNull("v")

    val card = OfferCard(
        stops = obj.intOrNull("stops"),
        pickups = obj.intOrNull("pickups"),
        distanceM = obj.intOrNull("distance_m"),
        deadlineHintMins = obj.intOrNull("deadline_hint_mins"),
        vendors = obj.stringList("vendors"),
        verticals = obj.stringList("verticals"),
        temperature = obj.stringList("temperature"),
        isRetry = runCatching {
            obj["retry"]?.jsonPrimitive?.content == "true"
        }.getOrDefault(false),
        // An absent version is treated as unknown, not as v1. A card with no
        // version did not come from a writer that agreed to this contract.
        unknownVersion = version != OFFER_CARD_VERSION,
    )
    return if (card.isEmpty() && !card.isRetry) null else card
}
