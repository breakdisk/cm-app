package io.logisticos.customer.armeasurement

/**
 * ArMeasurementModule — Android ARCore implementation (Expo Modules API)
 *
 * Flow:
 *   1. measureBox() called from JS → launches ArMeasureActivity.
 *   2. ARCore session initialises, detects horizontal planes.
 *   3. User taps 4 corners guided by on-screen labels:
 *      Tap 1 → Tap 2 = Length
 *      Tap 2 → Tap 3 = Width
 *      Tap 3 → Tap 4 = Height (vertical)
 *   4. Confirm button sends result back via ActivityResultContracts.
 *   5. Module resolves the JS promise with BoxDimensions.
 *
 * Requires in AndroidManifest.xml:
 *   <uses-feature android:name="android.hardware.camera.ar" android:required="true"/>
 *   <meta-data android:name="com.google.ar.core" android:value="required"/>
 *   <activity android:name=".armeasurement.ArMeasureActivity" ... />
 *
 * Gradle dependency (customer-app/android/build.gradle):
 *   implementation("com.google.ar:core:1.44.0")
 */

import android.app.Activity
import android.content.Intent
import expo.modules.core.interfaces.ActivityEventListener
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition
import expo.modules.kotlin.Promise
import com.google.ar.core.ArCoreApk

private const val REQUEST_CODE_AR_MEASURE = 0xAB01

class ArMeasurementModule : Module(), ActivityEventListener {

  private var pendingPromise: Promise? = null

  override fun definition() = ModuleDefinition {
    Name("ArMeasurement")

    OnCreate {
      appContext.registerActivityEventListener(this@ArMeasurementModule)
    }

    OnDestroy {
      appContext.unregisterActivityEventListener(this@ArMeasurementModule)
    }

    AsyncFunction("isAvailable") { promise: Promise ->
      val ctx = appContext.reactContext ?: run {
        promise.resolve(false); return@AsyncFunction
      }
      when (ArCoreApk.getInstance().checkAvailability(ctx)) {
        ArCoreApk.Availability.SUPPORTED_INSTALLED,
        ArCoreApk.Availability.SUPPORTED_APK_TOO_OLD,
        ArCoreApk.Availability.SUPPORTED_NOT_INSTALLED -> promise.resolve(true)
        else -> promise.resolve(false)
      }
    }

    AsyncFunction("measureBox") { promise: Promise ->
      val activity = appContext.currentActivity ?: run {
        promise.reject("AR_UNAVAILABLE", "No foreground activity.", null); return@AsyncFunction
      }
      val ctx = appContext.reactContext ?: run {
        promise.reject("AR_UNAVAILABLE", "No React context.", null); return@AsyncFunction
      }
      when (ArCoreApk.getInstance().checkAvailability(ctx)) {
        ArCoreApk.Availability.SUPPORTED_INSTALLED -> {
          // Reject any in-flight promise before accepting a new one.
          // Without this, the first caller's await hangs indefinitely if measureBox()
          // is called again before the AR activity returns.
          pendingPromise?.reject("USER_CANCELLED", "Superseded by a new measurement request.", null)
          pendingPromise = promise
          val intent = Intent(activity, ArMeasureActivity::class.java)
          activity.startActivityForResult(intent, REQUEST_CODE_AR_MEASURE)
        }
        else -> promise.reject("AR_UNAVAILABLE", "ARCore is not available on this device.", null)
      }
    }
  }

  // ── ActivityEventListener ──────────────────────────────────────────────────

  override fun onActivityResult(
    activity: Activity,
    requestCode: Int,
    resultCode: Int,
    data: Intent?,
  ) {
    if (requestCode != REQUEST_CODE_AR_MEASURE) return
    val promise = pendingPromise ?: return
    pendingPromise = null

    when (resultCode) {
      Activity.RESULT_OK -> {
        val l = data?.getDoubleExtra(ArMeasureActivity.EXTRA_LENGTH, 0.0) ?: 0.0
        val w = data?.getDoubleExtra(ArMeasureActivity.EXTRA_WIDTH,  0.0) ?: 0.0
        val h = data?.getDoubleExtra(ArMeasureActivity.EXTRA_HEIGHT, 0.0) ?: 0.0
        val c = data?.getDoubleExtra(ArMeasureActivity.EXTRA_CONFIDENCE, 0.8) ?: 0.8
        promise.resolve(mapOf("length" to l, "width" to w, "height" to h, "confidence" to c))
      }
      Activity.RESULT_CANCELED -> promise.reject("USER_CANCELLED", "User dismissed AR view.", null)
      else -> promise.reject("TRACKING_FAILED", "AR measurement failed.", null)
    }
  }

  override fun onNewIntent(intent: Intent) { /* no-op */ }
}
