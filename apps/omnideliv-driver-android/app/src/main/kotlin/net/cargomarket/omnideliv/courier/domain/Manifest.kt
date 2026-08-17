package net.cargomarket.omnideliv.courier.domain

/**
 * A job as the courier works it.
 *
 * Mirrors `GET /v1/omnideliv/courier/jobs/{order_id}`. Held only for rendering
 * — the server is truth, and this is replaced wholesale on every fetch so a
 * route the Logistics agent rewrites mid-trip simply appears rather than having
 * to be reconciled against stale local state.
 */
data class Manifest(
    val orderId: String,
    val status: String,
    val codAmountCents: Long,
    val tripCents: Long,
    val tipCents: Long,
    val stops: List<Stop>,
    val dropoff: Dropoff,
)

data class Stop(
    /** Sent back on `arrived` and `collected`. Opaque to field-ops. */
    val stopRef: String,
    val seq: Int,
    val vendorName: String,
    val address: String,
    val lat: Double,
    val lng: Double,
    val vertical: String,
    val prepTimeMinutes: Int,
    val pickedUp: Boolean,
    val lines: List<Line>,
)

data class Line(
    val qty: Int,
    val itemName: String,
    val modifiers: List<String>,
)

data class Dropoff(
    val stopRef: String,
    val lat: Double,
    val lng: Double,
    val customerName: String?,
    val customerPhone: String?,
    val notes: String?,
)

/**
 * Where the courier is in the job.
 *
 * Derived, never stored. "En route" has no milestone precisely because it is
 * this: claimed, and not yet collected at the next stop.
 */
sealed interface Leg {
    /** Heading to a pickup that has not been collected. */
    data class ToPickup(val stop: Stop, val remainingPickups: Int) : Leg

    /** Every pickup is collected; the only thing left is the customer. */
    data class ToDropoff(val dropoff: Dropoff) : Leg

    /** Nothing left to do. */
    data object Done : Leg
}

/**
 * The next thing to do, and how much is left after it.
 *
 * Stops are taken in `seq` order rather than list order — the server sequences
 * them by readiness, and trusting the array's order would silently reorder a
 * route the moment a serialiser stopped preserving it.
 */
fun Manifest.currentLeg(): Leg {
    if (status == "delivered") return Leg.Done

    val pending = stops.filterNot { it.pickedUp }.sortedBy { it.seq }
    val next = pending.firstOrNull()
        ?: return Leg.ToDropoff(dropoff)

    return Leg.ToPickup(next, remainingPickups = pending.size)
}

/**
 * What the rail shows: every stop plus the dropoff, in order, each marked done
 * or not.
 *
 * The dropoff is appended rather than modelled as a stop because it is not one
 * — it has no vendor, no prep time and no line items, and pretending otherwise
 * would put four empty fields on every pickup card to accommodate it.
 */
data class RailEntry(
    val label: String,
    val seq: Int,
    val done: Boolean,
    val isDropoff: Boolean,
)

fun Manifest.rail(): List<RailEntry> {
    val pickups = stops.sortedBy { it.seq }.map {
        RailEntry(
            label = it.vendorName,
            seq = it.seq,
            done = it.pickedUp,
            isDropoff = false,
        )
    }
    val dropLabel = dropoff.customerName ?: "Customer"
    return pickups + RailEntry(
        label = dropLabel,
        seq = (stops.maxOfOrNull { it.seq } ?: 0) + 1,
        done = status == "delivered",
        isDropoff = true,
    )
}

/**
 * Total the courier is owed for this job, in cents.
 *
 * COD is deliberately absent: it is the customer's money passing through the
 * courier's hands, not earnings, and adding it here is the same sign error the
 * backend ledger exists to prevent.
 */
fun Manifest.courierEarningsCents(): Long = tripCents + tipCents
