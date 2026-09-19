package io.logisticos.driver.feature.profile.presentation

import io.logisticos.driver.core.network.service.AddendumDto
import io.logisticos.driver.core.network.service.HomeCatalogueItemDto
import io.logisticos.driver.core.network.service.SurveyExtraDto
import io.logisticos.driver.core.network.service.SurveyItemDto
import io.logisticos.driver.core.network.service.SurveyRequest
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

// The whole-home lead's rules, kept apart from the screens so they can be
// tested. The survey limits mirror order-intake's; the server holds them too.

const val SURVEY_MAX_LINES = 400
const val SURVEY_MAX_QTY = 200
const val SURVEY_MAX_EXTRAS = 50
const val EXTRA_MAX_QTY = 100
const val EXTRA_MAX_UNIT_CENTS = 1_000_000L
const val OFF_DAY_HORIZON_DAYS = 366L

val WEEKDAYS = listOf("Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun")

/** "2 trucks & 5-person crew" — the customer app's wording. */
fun crewLine(trucks: Int, crew: Int): String =
    "$trucks truck${if (trucks == 1) "" else "s"} & $crew-person crew"

/** Pesos as the rest of the app shows them; any other currency by its code. */
fun money(cents: Long, currency: String): String {
    val whole = cents % 100 == 0L
    val amount = if (whole) String.format(Locale.US, "%,d", cents / 100) else String.format(Locale.US, "%,.2f", cents / 100.0)
    return if (currency.equals("PHP", ignoreCase = true)) "₱$amount" else "$currency $amount"
}

/**
 * "250", "250.5", "1,250.50" → minor units. Null for anything else,
 * including more than two decimals and nothing at all.
 */
fun centsFromText(text: String): Long? {
    val t = text.trim().replace(",", "")
    if (!Regex("""\d{1,9}(\.\d{1,2})?""").matches(t)) return null
    val parts = t.split('.')
    val whole = parts[0].toLong()
    val frac = parts.getOrNull(1)?.padEnd(2, '0')?.toLong() ?: 0L
    return whole * 100 + frac
}

// ── Survey ────────────────────────────────────────────────────────────────────

/** One item the survey found beyond the booking. */
data class SurveyLine(
    val room: String,
    val itemKey: String,
    val name: String,
    val qty: Int,
    val dismantle: Boolean,
    val packing: Boolean,
)

/** A packing material or resource (a hoist, an extra helper), priced per unit. */
data class ExtraDraft(
    /** "material" | "resource" */
    val kind: String,
    val name: String,
    val qty: Int,
    val unitCents: Long,
)

/** One more of [item] in [room]; a new line takes the catalogue's defaults. */
fun addItem(lines: List<SurveyLine>, room: String, item: HomeCatalogueItemDto): List<SurveyLine> {
    val i = lines.indexOfFirst { it.room == room && it.itemKey == item.key }
    if (i >= 0) {
        val line = lines[i]
        if (line.qty >= SURVEY_MAX_QTY) return lines
        return lines.toMutableList().also { it[i] = line.copy(qty = line.qty + 1) }
    }
    return lines + SurveyLine(room, item.key, item.name, 1, dismantle = item.assembly, packing = item.packing)
}

/** One fewer; the line goes at zero. */
fun removeItem(lines: List<SurveyLine>, room: String, itemKey: String): List<SurveyLine> =
    lines.mapNotNull { l ->
        if (l.room != room || l.itemKey != itemKey) l
        else if (l.qty > 1) l.copy(qty = l.qty - 1)
        else null
    }

fun qtyOf(lines: List<SurveyLine>, room: String, itemKey: String): Int =
    lines.firstOrNull { it.room == room && it.itemKey == itemKey }?.qty ?: 0

/** Why an extra would be refused, or null. */
fun extraProblem(e: ExtraDraft): String? = when {
    e.kind != "material" && e.kind != "resource" -> "An extra is a material or a resource."
    e.name.isBlank() || e.name.length > 80 -> "Name it — up to 80 characters."
    e.qty !in 1..EXTRA_MAX_QTY -> "Quantity is 1 to $EXTRA_MAX_QTY."
    e.unitCents !in 1..EXTRA_MAX_UNIT_CENTS -> "The unit price must be above zero and at most ${money(EXTRA_MAX_UNIT_CENTS, "PHP")}."
    else -> null
}

/** Why the survey would be refused, or null. An empty survey is allowed: it
 *  records that the home matched the booking. */
fun surveyProblem(lines: List<SurveyLine>, extras: List<ExtraDraft>): String? = when {
    lines.size > SURVEY_MAX_LINES -> "At most $SURVEY_MAX_LINES item lines."
    lines.any { it.qty !in 1..SURVEY_MAX_QTY } -> "Each item's quantity is 1 to $SURVEY_MAX_QTY."
    extras.size > SURVEY_MAX_EXTRAS -> "At most $SURVEY_MAX_EXTRAS extras."
    else -> extras.firstNotNullOfOrNull(::extraProblem)
}

fun surveyRequest(lines: List<SurveyLine>, extras: List<ExtraDraft>, note: String): SurveyRequest =
    SurveyRequest(
        items = lines.map { SurveyItemDto(it.room, it.itemKey, it.qty, it.dismantle, it.packing) },
        extras = extras.map { SurveyExtraDto(it.kind, it.name.trim(), it.qty, it.unitCents) },
        note = note.trim().take(500),
    )

/** What the extras add, before the server re-prices the items. */
fun extrasTotal(extras: List<ExtraDraft>): Long = extras.sumOf { it.qty * it.unitCents }

/** Where the survey's addendum stands, from the lead's side. */
fun addendumLine(a: AddendumDto): String = when (a.status) {
    "pending" -> "Waiting on the customer: +${money(a.totalCents, a.currency)}"
    "approved" -> "Approved by the customer — awaiting payment of ${money(a.totalCents, a.currency)}"
    "paid" -> "Approved and paid: +${money(a.totalCents, a.currency)}. The crew is ${crewLine(a.trucks, a.crewTotal)}."
    "declined" -> "Declined — the move stands as booked."
    else -> a.status
}

// ── Availability ─────────────────────────────────────────────────────────────

fun toggleDay(days: List<Int>, day: Int): List<Int> =
    (if (day in days) days - day else days + day).filter { it in 0..6 }.distinct().sorted()

/** Why [day] cannot be taken off, or null. */
fun offDayProblem(day: LocalDate, today: LocalDate): String? = when {
    day.isBefore(today) -> "That day has passed."
    day.isAfter(today.plusDays(OFF_DAY_HORIZON_DAYS)) -> "Off days can be set up to a year ahead."
    else -> null
}

/** [days] with [day] added, in order, once. */
fun addOffDay(days: List<String>, day: LocalDate): List<String> =
    (days + day.toString()).distinct().sorted()

/** "2026-09-26" → "Sat 26 Sep"; anything unparseable as given. */
fun dayLabel(isoDate: String): String =
    runCatching { LocalDate.parse(isoDate.take(10)).format(DateTimeFormatter.ofPattern("EEE d MMM", Locale.US)) }
        .getOrDefault(isoDate)

/** An instant in the phone's zone: "Sat 26 Sep · 8:00 AM". */
fun whenLabel(iso: String, zone: ZoneId = ZoneId.systemDefault()): String =
    runCatching {
        Instant.parse(iso).atZone(zone).format(DateTimeFormatter.ofPattern("EEE d MMM · h:mm a", Locale.US))
    }.getOrDefault(iso)

/** The server's `error.message`, or [fallback]. */
fun serverMessage(body: String?, fallback: String): String {
    val m = body?.let { Regex(""""message"\s*:\s*"((?:[^"\\]|\\.)*)"""").find(it) }?.groupValues?.get(1)
    return m?.replace("\\\"", "\"")?.takeIf { it.isNotBlank() } ?: fallback
}
