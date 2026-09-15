package io.logisticos.driver.core.designsystem

import java.util.Locale

/** Driver amounts are pesos across the app (TaskCards, EarningsScreen); kept consistent. */
fun pesos(cents: Long): String =
    if (cents % 100 == 0L) "₱" + String.format(Locale.US, "%,d", cents / 100)
    else "₱" + String.format(Locale.US, "%,.2f", cents / 100.0)

fun kilograms(grams: Long): String {
    val kg = grams / 1000.0
    return if (kg >= 100) String.format(Locale.US, "%,d kg", kg.toLong()) else String.format(Locale.US, "%.1f kg", kg)
}

/** "6h 12m", "11h", "45m" from minutes; never negative. */
fun hoursMinutes(minutes: Long): String {
    val m = minutes.coerceAtLeast(0)
    return when {
        m < 60 -> "${m}m"
        m % 60 == 0L -> "${m / 60}h"
        else -> "${m / 60}h ${m % 60}m"
    }
}
