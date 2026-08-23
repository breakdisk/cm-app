package net.cargomarket.omnideliv.courier.domain

import java.net.URLEncoder
import java.util.Locale

/**
 * A stop, as something the courier's own map app can open.
 *
 * `geo:` rather than a Google Maps or Waze URL on purpose. It is the platform's
 * chooser: whatever the courier already has installed and prefers answers it,
 * which on a bike in Manila is as likely to be Waze as Maps. Hard-coding one
 * vendor would send a courier to an app they do not use, or to the Play Store.
 *
 * An order carries no street address — checkout captures coordinates only — so
 * the pin *is* the destination, and the label is the only human-readable part
 * of it.
 */
fun navigationUri(lat: Double, lng: Double, label: String?): String {
    // Locale.ROOT, always. A phone set to a locale that uses a comma for the
    // decimal separator would otherwise produce `geo:14,5995` and send the
    // courier nowhere at all.
    val point = String.format(Locale.ROOT, "%s,%s", trim(lat), trim(lng))

    val name = label?.trim().orEmpty()
    if (name.isEmpty()) return "geo:$point?q=$point"

    // Only the label is encoded. The parentheses are structural in a `geo:`
    // query — `?q=lat,lng(Label)` — so encoding them turns the label into part
    // of the search text and the pin lands on a text search instead of the
    // point. The name itself must be encoded: vendors are called things like
    // "Kuya's Bar & Grill", and a raw space truncates the query at "Kuya's".
    val encoded = URLEncoder.encode(name, "UTF-8").replace("+", "%20")
    return "geo:$point?q=$point($encoded)"
}

/** `14.5995` rather than `14.599500`, without ever emitting a locale comma. */
private fun trim(v: Double): String =
    String.format(Locale.ROOT, "%.5f", v).trimEnd('0').trimEnd('.')
