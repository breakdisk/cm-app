package io.logisticos.driver.core.common

/**
 * Service classification carried inside every CargoMarket AWB.
 *
 * # Why this exists
 *
 * The Proof of Pickup payload has to tell the backend which billing track a
 * pickup belongs to — `balikbayan` routes to Track A (driver ledger debit at the
 * doorstep), everything else routes to Track B (weight-based surcharge at hub
 * weigh-in). The driver app has no dedicated service-code field on its task
 * records, but it does not need one: the AWB already encodes the classification
 * in a fixed position, so it can be read off the tracking number the driver is
 * already holding.
 *
 * # AWB format
 *
 * Master: `CM-{TTT}-{S}{NNNNNNN}{C}` — e.g. `CM-PH1-S0001234X`
 * Child:  `CM-{TTT}-{S}{NNNNNNN}{C}-{PPP}` — e.g. `CM-PH1-B0009012Z-002`
 *
 * `{S}` is the single service character; this parser reads exactly that and
 * ignores the rest, so it works on both master and child (piece) labels.
 *
 * The wire values returned by [wireValue] mirror `ServiceCode::as_str()` in
 * `libs/types/src/awb.rs`. Keep the two in sync.
 */
enum class AwbServiceCode(val char: Char, val wireValue: String) {
    STANDARD('S', "standard"),
    EXPRESS('E', "express"),
    SAME_DAY('D', "same_day"),
    BALIKBAYAN('B', "balikbayan"),
    INTERNATIONAL('N', "international"),
    ;

    companion object {
        /** Wire value sent when the AWB cannot be classified. Matches the
         *  backend's `#[serde(default = "default_standard")]` so an unparseable
         *  AWB behaves exactly as it did before this parser existed. */
        const val DEFAULT_WIRE_VALUE: String = "standard"

        /**
         * Extracts the service code from a master or child AWB.
         *
         * Returns null rather than throwing when the input is not a
         * well-formed CargoMarket AWB — pickups can carry legacy or
         * externally-issued tracking numbers, and a pickup must never fail
         * because its label predates this format.
         */
        fun fromAwb(awb: String?): AwbServiceCode? {
            val parts = awb?.trim()?.uppercase()?.split('-') ?: return null
            // Expect at least CM / tenant / serial. Child labels add a 4th part.
            if (parts.size < 3) return null
            if (parts[0] != "CM") return null
            if (parts[1].length != 3) return null
            val serial = parts[2]
            // {S} + 7-digit sequence + 1 check char.
            if (serial.length != 9) return null
            return entries.firstOrNull { it.char == serial[0] }
        }

        /**
         * Service code for the wire, falling back to "standard" when the AWB is
         * absent or unrecognised. This is the value callers should send — it is
         * always a valid `service_code` for the pod service.
         */
        fun wireValueFor(awb: String?): String =
            fromAwb(awb)?.wireValue ?: DEFAULT_WIRE_VALUE
    }
}
