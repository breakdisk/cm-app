package net.cargomarket.omnideliv.courier.domain

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Test

/**
 * Pinch-to-zoom on the proof camera.
 *
 * The camera bound its preview and threw away the `Camera` handle
 * `bindToLifecycle` returns, so there was no `cameraControl` to zoom with and
 * no gesture reading one — a courier photographing a door number, a receipt or
 * a gate code from arm's length had no way to get closer. Reported from a
 * device 2026-08-23.
 *
 * The arithmetic is here rather than in the composable because clamping is the
 * part that goes wrong, and it cannot be seen in a preview.
 */
class ProofZoomTest {

    @Test
    fun `a pinch multiplies the current ratio`() {
        assertEquals(2.0f, zoomAfterPinch(current = 1.0f, scale = 2.0f, min = 1.0f, max = 8.0f))
        assertEquals(3.0f, zoomAfterPinch(current = 1.5f, scale = 2.0f, min = 1.0f, max = 8.0f))
    }

    /** Pinching in returns towards the wide end, never past it. */
    @Test
    fun `zoom never goes below the lens minimum`() {
        assertEquals(1.0f, zoomAfterPinch(current = 1.2f, scale = 0.1f, min = 1.0f, max = 8.0f))
        assertEquals(0.5f, zoomAfterPinch(current = 1.0f, scale = 0.1f, min = 0.5f, max = 8.0f))
    }

    /**
     * Past the maximum the camera rejects the value outright, which on some
     * devices throws rather than saturating — so the clamp has to happen here,
     * not be left to the hardware.
     */
    @Test
    fun `zoom never goes above the lens maximum`() {
        assertEquals(8.0f, zoomAfterPinch(current = 6.0f, scale = 4.0f, min = 1.0f, max = 8.0f))
        assertEquals(8.0f, zoomAfterPinch(current = 8.0f, scale = 1.5f, min = 1.0f, max = 8.0f))
    }

    /**
     * A gesture that is not a pinch reports scale 1. Recomputing on every frame
     * of a drag must leave the zoom exactly where it was, or the picture creeps
     * while the courier is only trying to steady the phone.
     */
    @Test
    fun `a gesture with no pinch leaves the zoom alone`() {
        assertEquals(2.5f, zoomAfterPinch(current = 2.5f, scale = 1.0f, min = 1.0f, max = 8.0f))
    }

    /** A camera that reports a degenerate range must not produce NaN. */
    @Test
    fun `a lens that cannot zoom stays at its single ratio`() {
        assertEquals(1.0f, zoomAfterPinch(current = 1.0f, scale = 3.0f, min = 1.0f, max = 1.0f))
    }
}
