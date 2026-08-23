package net.cargomarket.omnideliv.courier.domain

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/**
 * Handing a stop off to whatever navigation app the courier already uses.
 *
 * The manifest showed a dropoff as a pair of raw coordinates and nothing else —
 * no address exists on an order — and there was no way to act on them: no
 * `geo:` intent anywhere in the app. A courier could read the numbers and not
 * open them. Reported from a device on 2026-08-23.
 *
 * Deliberately a `geo:` URI rather than a Google Maps or Waze URL. It is the
 * platform's own chooser: whatever the courier has installed and prefers
 * answers it, which on a bike in Manila is as likely to be Waze as Maps.
 */
class NavigationTest {

    @Test
    fun `a stop becomes a geo uri the platform can resolve`() {
        val uri = navigationUri(14.59950, 120.98420, "Kuya's")

        assertTrue(uri.startsWith("geo:"), "must be a geo intent, not a vendor-specific URL")
        assertTrue(uri.startsWith("geo:14.5995,120.9842"), "the pin is the coordinates: $uri")
    }

    /**
     * The label is what the courier sees in the map app's search field, and it
     * is the difference between "you are going here" and a bare dot.
     */
    @Test
    fun `the stop name travels as the pin label`() {
        val uri = navigationUri(14.6, 120.98, "Restodemo1")
        assertTrue(uri.contains("(Restodemo1)"), uri)
    }

    /**
     * Names contain spaces, ampersands and apostrophes — "Kuya's Bar & Grill"
     * is an ordinary vendor. Unescaped, the query silently truncates at the
     * space and the courier is navigated to a pin labelled "Kuya's".
     */
    @Test
    fun `a name with spaces and punctuation survives encoding`() {
        val uri = navigationUri(14.6, 120.98, "Kuya's Bar & Grill")

        assertTrue(!uri.contains(" "), "a raw space truncates the query: $uri")
        assertTrue(uri.contains("Kuya%27s") || uri.contains("Kuya's"), uri)
        assertTrue(uri.contains("%26") || uri.contains("&amp;"), "the ampersand must be escaped: $uri")
    }

    /**
     * Coordinates are formatted with a decimal point, always. A courier's phone
     * set to a locale that uses a comma would otherwise produce `geo:14,5995`
     * and send them nowhere.
     */
    @Test
    fun `coordinates never take a locale decimal separator`() {
        val uri = navigationUri(14.5995, 120.9842, "x")
        assertTrue(uri.startsWith("geo:14.5995,120.9842"), uri)
        assertEquals(2, uri.substringBefore("?").count { it == '.' }, uri)
    }

    /** No label is still a usable pin, not a broken URI. */
    @Test
    fun `a stop with no name still navigates`() {
        val uri = navigationUri(14.6, 120.98, null)
        assertTrue(uri.startsWith("geo:14.6,120.98"), uri)
        assertTrue(!uri.contains("()"), "an empty label is worse than none: $uri")
    }
}
