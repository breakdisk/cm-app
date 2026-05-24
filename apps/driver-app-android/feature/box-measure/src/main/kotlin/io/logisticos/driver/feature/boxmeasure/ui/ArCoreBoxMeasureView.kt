package io.logisticos.driver.feature.boxmeasure.ui

import android.content.Context
import android.graphics.PixelFormat
import android.opengl.GLES20
import android.opengl.GLSurfaceView
import android.os.Handler
import android.os.Looper
import android.view.MotionEvent
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView
import com.google.ar.core.*
import com.google.ar.core.exceptions.*
import io.logisticos.driver.feature.boxmeasure.presentation.BoxMeasureViewModel
import java.util.concurrent.ConcurrentLinkedQueue
import javax.microedition.khronos.egl.EGLConfig
import javax.microedition.khronos.opengles.GL10
import kotlin.math.abs
import kotlin.math.sqrt

/**
 * ARCore-backed measurement view embedded as a Compose [AndroidView].
 *
 * Plane detection mode: HORIZONTAL_AND_VERTICAL.
 * Depth mode: AUTOMATIC when supported, DISABLED otherwise.
 *
 * 4-tap guided measurement:
 *   Tap 1 → first corner of length edge
 *   Tap 2 → second corner (length = dist(pt1, pt2))
 *   Tap 3 → adjacent corner (width  = dist(pt2, pt3))
 *   Tap 4 → top corner     (height = |pt3.y - pt4.y|)
 *
 * On tap 4 the ViewModel's [onMeasurementComplete] callback fires with L, W, H in cm.
 *
 * ── Threading contract ──────────────────────────────────────────────────────────
 * ARCore [Session.update] and [Frame.hitTest] are called ONLY inside
 * [ArRenderer.onDrawFrame] on the GL thread — never from the UI thread.
 *
 * Tap events arrive on the UI thread and are enqueued into [tapQueue] (a lock-free
 * [ConcurrentLinkedQueue]). The GL thread drains one tap per [onDrawFrame] and
 * performs hit-testing with the just-updated frame.
 *
 * [Session.close] is dispatched via [GLSurfaceView.queueEvent] so it is guaranteed
 * to execute on the GL thread only after the current [onDrawFrame] finishes.
 *
 * ViewModel callbacks are posted back to the main thread via [Handler].
 */
@Composable
fun ArCoreBoxMeasureView(
    modifier: Modifier = Modifier,
    viewModel: BoxMeasureViewModel,
) {
    // Tap (x,y) coordinates produced on the UI thread, consumed on the GL thread.
    val tapQueue = remember { ConcurrentLinkedQueue<Pair<Float, Float>>() }

    // Stable renderer reference held across recompositions for teardown.
    val rendererRef = remember { mutableStateOf<ArRenderer?>(null) }

    DisposableEffect(Unit) {
        onDispose {
            // Dispatch Session.close() to the GL thread to avoid a race with onDrawFrame.
            // queueEvent() is safe to call from any thread — it schedules work after the
            // current frame render completes.
            rendererRef.value?.closeOnGlThread()
        }
    }

    AndroidView(
        modifier = modifier,
        factory = { ctx ->
            ArRenderer(
                ctx      = ctx,
                tapQueue = tapQueue,
                onSessionReady = { viewModel.onArSessionReady() },
                onSessionError = { msg -> viewModel.onArSessionError(msg) },
                onMeasurementPoint = { idx, x, y, z ->
                    viewModel.onMeasurementPoint(idx, x, y, z)
                },
                onMeasurementComplete = { l, w, h, confidence ->
                    viewModel.onMeasurementComplete(l, w, h, confidence)
                },
            ).also { renderer ->
                rendererRef.value = renderer
            }.glView
        },
        update = { glView ->
            glView.setOnTouchListener { _, event ->
                // Only enqueue the tap — never call session.update() here.
                // Hit-testing is deferred to the GL thread inside onDrawFrame.
                if (event.action == MotionEvent.ACTION_UP &&
                    viewModel.uiState.value.tapCount < 4
                ) {
                    tapQueue.offer(Pair(event.x, event.y))
                }
                true
            }
        }
    )
}

// ── Renderer ───────────────────────────────────────────────────────────────────

/**
 * [GLSurfaceView.Renderer] that owns the ARCore [Session] lifecycle.
 *
 * All ARCore calls ([Session.update], [Frame.hitTest]) happen on the GL thread.
 * ViewModel callbacks are marshalled back to the main thread via [mainHandler].
 */
private class ArRenderer(
    private val ctx: Context,
    private val tapQueue: ConcurrentLinkedQueue<Pair<Float, Float>>,
    private val onSessionReady: () -> Unit,
    private val onSessionError: (String) -> Unit,
    private val onMeasurementPoint: (index: Int, x: Float, y: Float, z: Float) -> Unit,
    private val onMeasurementComplete: (l: Double, w: Double, h: Double, confidence: Double) -> Unit,
) : GLSurfaceView.Renderer {

    /** The [GLSurfaceView] owned by this renderer — returned to [AndroidView.factory]. */
    val glView: GLSurfaceView = GLSurfaceView(ctx).also { surface ->
        surface.setEGLContextClientVersion(2)
        surface.setEGLConfigChooser(8, 8, 8, 8, 16, 0)
        surface.holder.setFormat(PixelFormat.TRANSLUCENT)
        surface.setRenderer(this)
        surface.renderMode = GLSurfaceView.RENDERMODE_CONTINUOUSLY
    }

    // Owned exclusively by the GL thread after onSurfaceCreated.
    private var session: Session? = null

    private val mainHandler = Handler(Looper.getMainLooper())

    // World-space tap points in order: p0 (length start), p1 (length end),
    // p2 (width end), p3 (height top). Owned by the GL thread.
    private val worldPts = mutableListOf<FloatArray>()

    override fun onSurfaceCreated(gl: GL10?, cfg: EGLConfig?) {
        GLES20.glClearColor(0.03f, 0.03f, 0.06f, 1f)
        try {
            // Check availability before creating a Session. This surfaces a clear
            // error on devices that don't support ARCore instead of a cryptic crash.
            val availability = ArCoreApk.getInstance().checkAvailability(ctx)
            if (!availability.isSupported) {
                val reason = when {
                    availability == ArCoreApk.Availability.UNSUPPORTED_DEVICE_NOT_CAPABLE ->
                        "AR not supported on this device — use manual entry."
                    else ->
                        "ARCore not available ($availability) — use manual entry."
                }
                mainHandler.post { onSessionError(reason) }
                return
            }

            val sess = Session(ctx)
            sess.configure(Config(sess).apply {
                planeFindingMode = Config.PlaneFindingMode.HORIZONTAL_AND_VERTICAL
                depthMode = if (sess.isDepthModeSupported(Config.DepthMode.AUTOMATIC))
                    Config.DepthMode.AUTOMATIC else Config.DepthMode.DISABLED
            })
            sess.resume()   // throws CameraNotAvailableException if permission not granted
            session = sess
            mainHandler.post { onSessionReady() }
        } catch (e: UnavailableException) {
            // ARCore SDK/APK version mismatch, or device incompatible.
            mainHandler.post { onSessionError("AR unavailable: ${e.javaClass.simpleName} — use manual entry.") }
        } catch (e: Exception) {
            // Catches CameraNotAvailableException (camera permission denied) and any
            // other unexpected error. Without this broad catch the crash escapes the
            // GL thread and takes down the whole app.
            mainHandler.post { onSessionError("AR failed: ${e.message ?: e.javaClass.simpleName} — use manual entry.") }
        }
    }

    override fun onSurfaceChanged(gl: GL10?, w: Int, h: Int) {
        GLES20.glViewport(0, 0, w, h)
        session?.setDisplayGeometry(0, w, h)
    }

    override fun onDrawFrame(gl: GL10?) {
        GLES20.glClear(GLES20.GL_COLOR_BUFFER_BIT or GLES20.GL_DEPTH_BUFFER_BIT)
        val sess = session ?: return
        // update() is the sole caller for this session — always on the GL thread.
        val frame = runCatching { sess.update() }.getOrNull() ?: return

        // Drain one pending tap per frame. Hit-testing runs against the frame
        // returned by the update() above, so there is never a concurrent update race.
        val tap = tapQueue.poll() ?: return
        if (worldPts.size >= 4) return  // measurement already complete

        val hits = frame.hitTest(tap.first, tap.second)
        val hit = hits.firstOrNull { h ->
            h.trackable is Plane && (h.trackable as Plane).isPoseInPolygon(h.hitPose)
        } ?: hits.firstOrNull() ?: return

        val p = hit.hitPose
        worldPts.add(floatArrayOf(p.tx(), p.ty(), p.tz()))
        val idx = worldPts.size

        // Snapshot scalar values for the main-thread lambda.
        val wx = p.tx(); val wy = p.ty(); val wz = p.tz()
        mainHandler.post { onMeasurementPoint(idx, wx, wy, wz) }

        if (worldPts.size == 4) {
            val (p0, p1, p2, p3) = worldPts
            val l  = dist(p0, p1) * 100
            val w  = dist(p1, p2) * 100
            val hh = abs((p3[1] - p2[1]).toDouble()) * 100
            val confidence = when (frame.camera.trackingState) {
                TrackingState.TRACKING -> 0.92
                else -> 0.6
            }
            val rL = round1(l); val rW = round1(w); val rH = round1(hh)
            mainHandler.post { onMeasurementComplete(rL, rW, rH, confidence) }
        }
    }

    /** Dispatches [Session.close] to the GL thread via [GLSurfaceView.queueEvent]. */
    fun closeOnGlThread() {
        glView.queueEvent {
            session?.close()
            session = null
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

private fun dist(a: FloatArray, b: FloatArray): Double {
    val dx = (a[0] - b[0]).toDouble()
    val dy = (a[1] - b[1]).toDouble()
    val dz = (a[2] - b[2]).toDouble()
    return sqrt(dx * dx + dy * dy + dz * dz)
}

private fun round1(v: Double): Double = (v * 10).toLong() / 10.0
