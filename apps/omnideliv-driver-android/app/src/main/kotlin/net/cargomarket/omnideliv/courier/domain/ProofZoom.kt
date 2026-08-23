package net.cargomarket.omnideliv.courier.domain

/**
 * Where a pinch leaves the camera's zoom ratio.
 *
 * Clamped here rather than left to the hardware: `setZoomRatio` outside the
 * lens's reported range is rejected, and on some devices that surfaces as a
 * thrown exception rather than a saturated value — mid-delivery, on the screen
 * that captures the only evidence a delivery happened.
 *
 * `min` and `max` come from the camera's own `ZoomState`, never from constants:
 * a phone with an ultra-wide reports a minimum below 1.0, and hard-coding 1.0
 * would make its widest lens unreachable.
 */
fun zoomAfterPinch(current: Float, scale: Float, min: Float, max: Float): Float =
    (current * scale).coerceIn(min, max)
