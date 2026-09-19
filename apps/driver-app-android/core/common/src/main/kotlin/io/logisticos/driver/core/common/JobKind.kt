package io.logisticos.driver.core.common

/**
 * What an offer's `delivery_category` says about the job, where it is a
 * whole-home move's slot. Dispatch names them:
 * - `home_move` — the whole move, one lead;
 * - `home_move_joint` — two trucks: the first single-truck lead to accept is
 *   the Mission Captain, and the second truck is offered next;
 * - `home_move_support` — that second truck (the Support Lead);
 * - `home_move_emergency` — an extra truck an addendum outgrew the booking by,
 *   needed today.
 * Anything else is a freight load, named as before.
 */
object JobKind {
    /** The card's headline for a home-move slot; null for any other load. */
    fun homeHeadline(category: String): String? = when (category) {
        "home_move" -> "WHOLE-HOME MOVE"
        "home_move_joint" -> "JOINT MISSION · 2 TRUCKS"
        "home_move_support" -> "SUPPORT LEAD · 2ND TRUCK"
        "home_move_emergency" -> "EMERGENCY TRUCK · TODAY"
        else -> null
    }

    /** One line on what taking it means. */
    fun homeDetail(category: String): String? = when (category) {
        "home_move" -> "You bring the truck and the crew. Claiming holds the day; the job lands 12 hours before."
        "home_move_joint" -> "Two trucks needed. Take it with one truck and you're the Mission Captain: you survey and run the move; a second lead brings the other truck."
        "home_move_support" -> "Bring the second truck and your crew. The Mission Captain runs the move; you're paid your share on delivery."
        "home_move_emergency" -> "The move outgrew its trucks. Bring yours and your crew now; you're paid a share of the addition on delivery."
        else -> null
    }

    /** The role a lead holds on a reserved move, in words. */
    fun roleLabel(role: String): String = when (role) {
        "captain" -> "Mission Captain"
        "support" -> "Support Lead"
        "emergency" -> "Extra truck"
        else -> "Lead"
    }
}
