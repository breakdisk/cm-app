/**
 * ArMeasurementModule — TypeScript bindings for the native ARCore / ARKit module.
 *
 * On Android: uses ARCore with plane detection + hit-testing to place measurement
 *             anchors. The user taps 3 edges (length, width, height) sequentially.
 * On iOS:     uses ARKit ARWorldTrackingConfiguration + ARRaycastQuery for the same
 *             3-tap guided measurement flow.
 *
 * Usage:
 *   const { length, width, height } = await ArMeasurementModule.measureBox();
 *   // All values in centimetres, rounded to 1 decimal place.
 *   // Throws with code "USER_CANCELLED" if the user dismisses the AR view.
 *   // Throws with code "AR_UNAVAILABLE" if the device has no AR support or the
 *   //   native module is not linked in the current build.
 *
 * ── Build compatibility ────────────────────────────────────────────────────────
 * requireOptionalNativeModule returns null (never throws) when the native module
 * is absent from the build — e.g. Expo Go, simulator builds, or any EAS build
 * that pre-dates the ar-measurement native module being added.
 * All public methods below guard on ArMeasurementNative != null and resolve
 * gracefully so QuoteScreen degrades to manual size selection without crashing.
 */

import { requireOptionalNativeModule } from "expo-modules-core";

export interface BoxDimensions {
  /** Length in centimetres */
  length: number;
  /** Width in centimetres */
  width: number;
  /** Height in centimetres */
  height: number;
  /** Confidence score 0–1 returned by the AR session (higher = better tracking) */
  confidence: number;
}

export type ArErrorCode = "USER_CANCELLED" | "AR_UNAVAILABLE" | "PERMISSION_DENIED" | "TRACKING_FAILED";

export interface ArError {
  code: ArErrorCode;
  message: string;
}

// Returns null when the native module is not linked — never throws.
// This keeps QuoteScreen importable on any build, including ones that pre-date
// the ar-measurement native module. The screen degrades to manual entry in that case.
const ArMeasurementNative = requireOptionalNativeModule("ArMeasurement");

const ArMeasurementModule = {
  /**
   * Opens the full-screen AR measurement UI.
   * Guides the user through tapping 3 edges of a box.
   * Resolves with BoxDimensions on success.
   * Rejects with ArError on failure, cancellation, or when AR is not available.
   */
  measureBox(): Promise<BoxDimensions> {
    if (!ArMeasurementNative) {
      return Promise.reject({
        code: "AR_UNAVAILABLE" as ArErrorCode,
        message: "AR measurement is not available in this build. Please select a box size manually.",
      });
    }
    return ArMeasurementNative.measureBox();
  },

  /**
   * Returns true if ARCore/ARKit is supported AND the native module is linked in
   * the current build. Call this before showing the "AR Measure" button.
   */
  isAvailable(): Promise<boolean> {
    if (!ArMeasurementNative) {
      // Native module not in this build — AR is definitively unavailable.
      return Promise.resolve(false);
    }
    return ArMeasurementNative.isAvailable();
  },
};

export default ArMeasurementModule;
