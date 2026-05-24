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
 *   // Throws with code "AR_UNAVAILABLE" if the device has no AR support.
 */

import { requireNativeModule } from "expo-modules-core";

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

// Lazily loaded — module may not be linked on simulator builds.
// requireNativeModule throws if the native side is absent, which is caught
// in the hook layer and surfaces the "AR_UNAVAILABLE" fallback path.
const ArMeasurementNative = requireNativeModule("ArMeasurement");

const ArMeasurementModule = {
  /**
   * Opens the full-screen AR measurement UI.
   * Guides the user through tapping 3 edges of a box.
   * Resolves with BoxDimensions on success.
   * Rejects with ArError on failure or cancellation.
   */
  measureBox(): Promise<BoxDimensions> {
    return ArMeasurementNative.measureBox();
  },

  /**
   * Returns true if ARCore/ARKit is supported and initialised on this device.
   * Call this before showing the "AR Measure" button.
   */
  isAvailable(): Promise<boolean> {
    return ArMeasurementNative.isAvailable();
  },
};

export default ArMeasurementModule;
