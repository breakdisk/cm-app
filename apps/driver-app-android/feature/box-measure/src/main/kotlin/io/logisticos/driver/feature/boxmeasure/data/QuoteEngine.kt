package io.logisticos.driver.feature.boxmeasure.data

import kotlin.math.max
import kotlin.math.round
import kotlin.math.roundToInt

/**
 * Self-contained Kotlin quote engine for the driver app.
 * Mirrors apps/landing/src/lib/quote-engine.ts — kept in sync manually.
 * All amounts expressed in origin currency (PH delivery converted from PHP via static FX).
 */

// ── Data models ────────────────────────────────────────────────────────────────

data class BoxSize(
    val id: String,
    val name: String,
    val dimensions: String,
    val dimsCm: Triple<Int, Int, Int>,   // L×W×H
    val maxWeightKg: Int,
) {
    val cbm: Double get() {
        val (l, w, h) = dimsCm
        return ((l.toLong() * w * h) / 1_000_000.0).let {
            (it * 10000).roundToInt() / 10000.0
        }
    }
}

data class QuoteLine(
    val label: String,
    val amount: Double,
    val currency: String,
    val note: String? = null,
    val component: Component,
) {
    enum class Component { SEA, AIR, PH_DELIVERY }
}

data class QuoteResult(
    val lines: List<QuoteLine>,
    val totalOriginCurrency: Double,
    val originCurrency: String,
    val cbm: Double,
    val transitDays: String,
)

// ── PHP → origin FX (indicative) ──────────────────────────────────────────────

private val PHP_PER = mapOf(
    "USD" to 57.0, "CAD" to 42.0, "GBP" to 72.0, "EUR" to 61.5,
    "AUD" to 37.0, "NZD" to 34.0, "JPY" to 0.38,  "KRW" to 0.042,
    "HKD" to 7.3,  "SGD" to 43.0, "AED" to 15.5,  "SAR" to 15.2,
    "QAR" to 15.7, "NOK" to 5.3,  "SEK" to 5.2,
)

private fun phpToOrigin(phpAmount: Double, currency: String): Double {
    val rate = PHP_PER[currency] ?: PHP_PER["USD"]!!
    return (phpAmount / rate * 100).roundToInt() / 100.0
}

// ── Province → zone lookup ─────────────────────────────────────────────────────

data class ProvinceEntry(val province: String, val zoneCode: String)

/**
 * Shortest prefix accepted for a fuzzy province match.
 *
 * Below this, prefixes are not discriminating: "b" alone is a prefix of
 * batangas, baguio, benguet, bulacan and bacolod, which span four different
 * delivery zones. Requiring a few characters means a partial entry either
 * resolves to something defensible or resolves to nothing.
 */
private const val MIN_PREFIX_LEN = 4

/**
 * Resolves free-text province input to a delivery zone.
 *
 * Returns null rather than guessing when the input is short or ambiguous. The
 * previous implementation fell back to `it.province.startsWith(q)` with no length
 * floor and took the first list hit, so a partially-typed province silently
 * priced a different zone: "ba" on the way to *Bacolod* (Zone 5C) matched
 * **Batangas** (Zone 2B) — roughly a 31 % error on a small box, shown to the user
 * as a settled figure with nothing to indicate it was a guess.
 *
 * Matching order:
 *  1. exact name
 *  2. the query starting with a full province name ("davao del sur" -> davao),
 *     longest name first so "cebu city" cannot be captured by "cebu"
 *  3. a query of at least [MIN_PREFIX_LEN] chars prefixing exactly one province
 */
fun resolveProvince(input: String): ProvinceEntry? {
    val q = input.lowercase().trim()
    if (q.isEmpty()) return null

    PROVINCE_MAP.find { it.province.lowercase() == q }?.let { return it }

    // Longest-first so a more specific entry wins over a shorter one it contains.
    PROVINCE_MAP
        .filter { q.startsWith(it.province.lowercase()) }
        .maxByOrNull { it.province.length }
        ?.let { return it }

    if (q.length < MIN_PREFIX_LEN) return null

    // Ambiguous prefixes resolve to nothing: silently picking one zone over
    // another is worse than declining to price, because the caller cannot tell
    // a guess from a match.
    val prefixHits = PROVINCE_MAP.filter { it.province.lowercase().startsWith(q) }
    val zones = prefixHits.map { it.zoneCode }.distinct()
    return if (prefixHits.isNotEmpty() && zones.size == 1) prefixHits.first() else null
}

// ── CBM helpers ────────────────────────────────────────────────────────────────

fun computeCbm(l: Double, w: Double, h: Double): Double {
    if (l <= 0 || w <= 0 || h <= 0) return 0.0
    return ((l * w * h) / 1_000_000.0 * 10000).roundToInt() / 10000.0
}

/** IATA volumetric divisor (cm³ per kg) used for air dimensional weight. */
const val VOLUMETRIC_DIVISOR = 5000.0

/**
 * Volumetric (dimensional) weight in kg: L×W×H(cm) ÷ [VOLUMETRIC_DIVISOR].
 * Air freight is billed on the greater of actual vs. volumetric weight, so this is
 * what a light-but-bulky box is effectively charged at. Rounded to 0.1 kg.
 */
fun computeVolumetricWeight(l: Double, w: Double, h: Double, divisor: Double = VOLUMETRIC_DIVISOR): Double {
    if (l <= 0 || w <= 0 || h <= 0) return 0.0
    return ((l * w * h) / divisor * 10).roundToInt() / 10.0
}

/**
 * How far a measurement may sit from a standard box's CBM and still be called
 * that box. 0.5 means "within 50 % of the nominal volume".
 *
 * A tolerance is required because [BOX_SIZES] is bounded: nearest-match alone
 * maps *any* volume, however large, onto Jumbo. That matters beyond the quote —
 * the matched size is carried into the POP dimensioning record, so an unbounded
 * match records a 2 m crate as a verified Jumbo and the anti-fraud check it
 * feeds becomes decorative.
 */
private const val SIZE_MATCH_TOLERANCE = 0.5

/**
 * Nearest standard box for a measurement, or null when nothing is close enough.
 *
 * Null means "non-standard, needs manual handling" — which is a real outcome the
 * caller must be able to distinguish from a confident match.
 */
fun matchToStandardSize(l: Double, w: Double, h: Double): BoxSize? {
    val measured = computeCbm(l, w, h)
    if (measured <= 0.0) return null
    val nearest = BOX_SIZES.minByOrNull { kotlin.math.abs(it.cbm - measured) } ?: return null
    if (nearest.cbm <= 0.0) return null
    val deviation = kotlin.math.abs(nearest.cbm - measured) / nearest.cbm
    return if (deviation <= SIZE_MATCH_TOLERANCE) nearest else null
}

// ── Main compute function ──────────────────────────────────────────────────────

enum class FreightMode { SEA, AIR }

fun computeQuote(
    mode: FreightMode,
    originKey: String,
    sizeId: String,
    qty: Int,
    weightKg: Double,
    dimsCm: Triple<Double, Double, Double>,
    province: String,
): QuoteResult {
    val lines = mutableListOf<QuoteLine>()
    val (dimL, dimW, dimH) = dimsCm
    var originCurrency = "USD"
    var transitDays = ""
    val cbm = computeCbm(dimL, dimW, dimH)

    // ── Sea ────────────────────────────────────────────────────────────────────
    if (mode == FreightMode.SEA) {
        val row = SEA_CARGO.find { it.origin == originKey }
        if (row != null && qty > 0) {
            originCurrency = row.currency
            transitDays = row.transitDays
            val unitPrice = row.prices[sizeId] ?: 0.0
            val sizeName = BOX_SIZES.find { it.id == sizeId }?.name ?: sizeId
            lines += QuoteLine(
                label     = "Sea Freight — $sizeName × $qty",
                amount    = unitPrice * qty,
                currency  = originCurrency,
                note      = row.origin,
                component = QuoteLine.Component.SEA,
            )
        }
    }

    // ── Air ────────────────────────────────────────────────────────────────────
    if (mode == FreightMode.AIR) {
        val zone = AIR_ZONES.find { it.zoneName == originKey }
        if (zone != null) {
            originCurrency = zone.currency
            transitDays = zone.transitDays
            val volKg = if (dimL > 0 && dimW > 0 && dimH > 0) (dimL * dimW * dimH) / zone.volumetricDivisor else 0.0
            val chargeKg = max(zone.minWeightKg, max(weightKg, volKg))
            val freight = (chargeKg * zone.ratePerKg * 100).roundToInt() / 100.0
            val fscAmt  = (freight * zone.fuelSurchargePct * 100).roundToInt() / 100.0

            lines += QuoteLine(
                label     = "Air Freight — ${"%.1f".format(chargeKg)} kg",
                amount    = freight,
                currency  = originCurrency,
                note      = if (volKg > weightKg) "vol ${"%.1f".format(volKg)} kg > actual ${"%.1f".format(weightKg)} kg"
                            else "actual ${"%.1f".format(weightKg)} kg",
                component = QuoteLine.Component.AIR,
            )
            if (fscAmt > 0) lines += QuoteLine("FSC (${(zone.fuelSurchargePct * 100).roundToInt()}%)", fscAmt, originCurrency, component = QuoteLine.Component.AIR)
            if (zone.awbFee > 0) lines += QuoteLine("AWB Fee", zone.awbFee, originCurrency, component = QuoteLine.Component.AIR)
            if (zone.thc > 0) lines += QuoteLine("THC", zone.thc, originCurrency, component = QuoteLine.Component.AIR)
            if (zone.customs > 0) lines += QuoteLine("PH Customs", zone.customs, originCurrency, component = QuoteLine.Component.AIR)
        }
    }

    // ── PH delivery ────────────────────────────────────────────────────────────
    val resolved = resolveProvince(province)
    if (resolved != null && qty > 0) {
        val zone = PH_ZONES.find { it.zoneCode == resolved.zoneCode }
        if (zone != null) {
            val phpPrice = (zone.prices[sizeId] ?: 0.0) * qty
            val converted = phpToOrigin(phpPrice, originCurrency)
            val sizeName = BOX_SIZES.find { it.id == sizeId }?.name ?: sizeId
            lines += QuoteLine(
                label     = "PH Delivery (${zone.zoneCode}) — $sizeName × $qty",
                amount    = converted,
                currency  = originCurrency,
                note      = "${zone.zoneName} · ${zone.transitDays} · ₱${"%.0f".format(phpPrice)} est.",
                component = QuoteLine.Component.PH_DELIVERY,
            )
        }
    }

    val total = (lines.sumOf { it.amount } * 100).roundToInt() / 100.0
    return QuoteResult(lines, total, originCurrency, cbm, transitDays)
}

// ── Rate data ──────────────────────────────────────────────────────────────────

data class SeaCargoRate(
    val origin: String,
    val currency: String,
    val transitDays: String,
    val prices: Map<String, Double>,
)

data class AirCargoZone(
    val zoneName: String,
    val currency: String,
    val ratePerKg: Double,
    val fuelSurchargePct: Double,
    val awbFee: Double,
    val thc: Double,
    val customs: Double,
    val minWeightKg: Double,
    val volumetricDivisor: Double,
    val transitDays: String,
)

data class PhDeliveryZone(
    val zoneCode: String,
    val zoneName: String,
    val prices: Map<String, Double>,
    val transitDays: String,
)

private fun p(xl: Double, jumbo: Double, large: Double, small: Double): Map<String, Double> = mapOf(
    "xl" to xl, "jumbo" to jumbo, "large" to large, "small" to small,
    "medium" to (((large + small) / 2 * 0.97) * 10).roundToInt() / 10.0,
    "bulilit" to ((small * 0.79) * 10).roundToInt() / 10.0,
)

private fun ph(xl: Double, jumbo: Double, large: Double, small: Double): Map<String, Double> = mapOf(
    "xl" to xl, "jumbo" to jumbo, "large" to large, "small" to small,
    "medium" to max(100.0, ((large + small) / 2 * 0.95)).let { (it * 10).roundToInt() / 10.0 },
    "bulilit" to max(100.0, small * 0.75).let { (it * 10).roundToInt() / 10.0 },
)

val BOX_SIZES = listOf(
    BoxSize("bulilit", "Bulilit", "30×25×25 cm", Triple(30, 25, 25), 10),
    BoxSize("small",   "Small",   "41×30×30 cm", Triple(41, 30, 30), 15),
    BoxSize("medium",  "Medium",  "46×36×36 cm", Triple(46, 36, 36), 18),
    BoxSize("large",   "Large",   "51×41×41 cm", Triple(51, 41, 41), 20),
    BoxSize("xl",      "XL",      "61×46×46 cm", Triple(61, 46, 46), 25),
    BoxSize("jumbo",   "Jumbo",   "61×61×61 cm", Triple(61, 61, 61), 30),
)

val SEA_CARGO = listOf(
    SeaCargoRate("USA West Coast (CA, WA, OR)",          "USD", "30–45 days", p(120.0, 150.0, 95.0,  70.0) ),
    SeaCargoRate("USA East Coast (NY, NJ, CT, MA)",      "USD", "40–55 days", p(150.0, 185.0, 118.0, 88.0) ),
    SeaCargoRate("USA South & Central (TX, FL, IL, GA)", "USD", "40–55 days", p(140.0, 175.0, 110.0, 82.0) ),
    SeaCargoRate("Hawaii",                               "USD", "25–35 days", p(110.0, 138.0, 88.0,  65.0) ),
    SeaCargoRate("Guam / Saipan (CNMI)",                 "USD", "14–21 days", p(86.0,  108.0, 68.0,  51.0) ),
    SeaCargoRate("Canada — British Columbia (Vancouver)", "CAD", "35–48 days", p(165.0, 205.0, 130.0, 96.0) ),
    SeaCargoRate("Canada — Ontario / Quebec",             "CAD", "42–56 days", p(180.0, 225.0, 143.0, 106.0)),
    SeaCargoRate("United Kingdom — London",              "GBP", "38–55 days", p(110.0, 138.0, 87.0,  65.0) ),
    SeaCargoRate("Ireland — Dublin",                     "EUR", "40–55 days", p(130.0, 163.0, 103.0, 76.0) ),
    SeaCargoRate("Germany — Frankfurt / Munich",         "EUR", "40–55 days", p(129.0, 161.0, 102.0, 75.0) ),
    SeaCargoRate("UAE — Dubai / Abu Dhabi",              "AED", "18–28 days", p(368.0, 460.0, 294.0, 213.0)),
    SeaCargoRate("Saudi Arabia — Riyadh",                "SAR", "20–30 days", p(383.0, 480.0, 308.0, 225.0)),
    SeaCargoRate("Qatar — Doha",                         "QAR", "20–30 days", p(357.0, 444.0, 284.0, 208.0)),
    SeaCargoRate("Kuwait",                               "USD", "20–30 days", p(99.0,  124.0, 79.0,  58.0) ),
    SeaCargoRate("Australia — Sydney / Melbourne",       "AUD", "22–32 days", p(163.0, 204.0, 130.0, 95.0) ),
    SeaCargoRate("Japan — Tokyo / Osaka",                "JPY", "10–18 days", p(11900.0, 14900.0, 9500.0, 7000.0)),
    SeaCargoRate("South Korea — Seoul / Busan",          "KRW", "10–18 days", p(100000.0, 125000.0, 79000.0, 58000.0)),
    SeaCargoRate("Hong Kong",                            "HKD", "7–12 days",  p(500.0, 625.0, 398.0, 297.0)),
    SeaCargoRate("Singapore",                            "SGD", "7–14 days",  p(92.0,  115.0, 73.0,  54.0) ),
)

val AIR_ZONES = listOf(
    AirCargoZone("Zone 1 — ASEAN / NE Asia",           "USD", 5.50,  0.18, 25.0, 15.0, 20.0, 5.0, 5000.0, "2–5 days"),
    AirCargoZone("Zone 2 — Australia / NZ",             "USD", 7.00,  0.18, 25.0, 15.0, 20.0, 5.0, 5000.0, "4–8 days"),
    AirCargoZone("Zone 3 — Middle East",                "USD", 8.00,  0.18, 25.0, 15.0, 20.0, 5.0, 5000.0, "5–9 days"),
    AirCargoZone("Zone 4 — Europe",                     "USD", 10.50, 0.18, 25.0, 15.0, 20.0, 5.0, 5000.0, "7–12 days"),
    AirCargoZone("Zone 5 — N. America West",            "USD", 12.00, 0.18, 25.0, 15.0, 20.0, 5.0, 5000.0, "7–12 days"),
    AirCargoZone("Zone 6 — N. America East / Central",  "USD", 13.50, 0.18, 25.0, 15.0, 20.0, 5.0, 5000.0, "8–14 days"),
)

val PH_ZONES = listOf(
    PhDeliveryZone("Zone 1A", "Metro Manila / NCR",              ph(850.0,  1000.0, 650.0,  450.0),  "1–2 days"),
    PhDeliveryZone("Zone 2A", "Region III — Bulacan / Pampanga", ph(1100.0, 1300.0, 850.0,  620.0),  "2–3 days"),
    PhDeliveryZone("Zone 2B", "CALABARZON",                      ph(1100.0, 1300.0, 850.0,  620.0),  "2–3 days"),
    PhDeliveryZone("Zone 3A", "Ilocos / La Union / Pangasinan",  ph(1450.0, 1800.0, 1100.0, 850.0),  "3–5 days"),
    PhDeliveryZone("Zone 3C", "CAR — Baguio / Cordillera",       ph(1700.0, 2100.0, 1350.0, 1000.0), "4–6 days"),
    PhDeliveryZone("Zone 5A", "Metro Cebu",                      ph(1350.0, 1700.0, 1050.0, 800.0),  "3–5 days"),
    PhDeliveryZone("Zone 5C", "Iloilo / Bacolod City",           ph(1550.0, 2000.0, 1250.0, 900.0),  "4–6 days"),
    PhDeliveryZone("Zone 6A", "Metro Davao",                     ph(1800.0, 2250.0, 1400.0, 1000.0), "5–7 days"),
    PhDeliveryZone("Zone 6C", "Cagayan de Oro / Bukidnon",       ph(1900.0, 2350.0, 1500.0, 1100.0), "5–8 days"),
)

val PROVINCE_MAP = listOf(
    ProvinceEntry("metro manila", "Zone 1A"), ProvinceEntry("manila", "Zone 1A"),
    ProvinceEntry("quezon city",  "Zone 1A"), ProvinceEntry("makati", "Zone 1A"),
    ProvinceEntry("pasig",        "Zone 1A"), ProvinceEntry("taguig", "Zone 1A"),
    ProvinceEntry("ncr",          "Zone 1A"), ProvinceEntry("bulacan","Zone 2A"),
    ProvinceEntry("pampanga",     "Zone 2A"), ProvinceEntry("cavite", "Zone 2B"),
    ProvinceEntry("laguna",       "Zone 2B"), ProvinceEntry("batangas","Zone 2B"),
    ProvinceEntry("pangasinan",   "Zone 3A"), ProvinceEntry("la union","Zone 3A"),
    ProvinceEntry("baguio",       "Zone 3C"), ProvinceEntry("benguet", "Zone 3C"),
    ProvinceEntry("cebu",         "Zone 5A"), ProvinceEntry("cebu city","Zone 5A"),
    ProvinceEntry("iloilo",       "Zone 5C"), ProvinceEntry("bacolod", "Zone 5C"),
    ProvinceEntry("davao",        "Zone 6A"), ProvinceEntry("davao city","Zone 6A"),
    ProvinceEntry("cagayan de oro","Zone 6C"),
)

fun formatAmount(amount: Double, currency: String): String {
    val sym = mapOf(
        "USD" to "$", "CAD" to "CA$", "GBP" to "£", "EUR" to "€",
        "AUD" to "A$", "NZD" to "NZ$", "JPY" to "¥", "KRW" to "₩",
        "HKD" to "HK$", "SGD" to "S$", "AED" to "AED ", "SAR" to "SAR ",
        "QAR" to "QR ", "NOK" to "kr ", "SEK" to "kr ",
    )[currency] ?: "$currency "
    return if (currency == "JPY" || currency == "KRW")
        "$sym${amount.roundToInt().toLong().toCommaString()}"
    else
        "$sym${"%.2f".format(amount)}"
}

private fun Long.toCommaString(): String = "%,d".format(this)
