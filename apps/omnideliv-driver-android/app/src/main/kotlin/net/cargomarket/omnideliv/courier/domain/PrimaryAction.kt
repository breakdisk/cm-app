package net.cargomarket.omnideliv.courier.domain

/**
 * What the one pinned button at the bottom of the manifest does right now.
 *
 * A function rather than a `when` inside the composable, because the screen had
 * this wrong in a way no compiler or preview could catch: on the final leg the
 * label read "Done" and the button was **disabled** — `enabled = currentLeg()
 * != Leg.Done` — while the shift screen had been popped off the back stack at
 * claim time. A courier who finished a delivery was left on a dead screen with
 * a grey button, at the end of every job. Reported from a device 2026-08-23.
 */
data class PrimaryAction(
    val label: String,
    val kind: Kind,
) {
    /** Always true. Kept explicit so the disabled-by-default mistake cannot recur silently. */
    val enabled: Boolean get() = true

    enum class Kind {
        /** Record the next milestone. */
        Advance,

        /** The job is over — leave the manifest and go back on duty. */
        Finish,
    }
}

fun primaryAction(leg: Leg): PrimaryAction = when (leg) {
    is Leg.ToPickup -> PrimaryAction("Picked up", PrimaryAction.Kind.Advance)
    is Leg.ToDropoff -> PrimaryAction("Delivered", PrimaryAction.Kind.Advance)
    // "Back on duty", not "Done": the courier has already been told the job is
    // complete by the banner above it, so the button should say where it goes.
    Leg.Done -> PrimaryAction("Back on duty", PrimaryAction.Kind.Finish)
}
