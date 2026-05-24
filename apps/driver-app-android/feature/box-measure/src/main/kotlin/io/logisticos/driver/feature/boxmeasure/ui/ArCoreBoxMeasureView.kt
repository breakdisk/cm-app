package io.logisticos.driver.feature.boxmeasure.ui

import android.content.Context
import android.graphics.PixelFormat
import android.opengl.GLES11Ext
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
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.FloatBuffer
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
 *
 * ── Camera background rendering ─────────────────────────────────────────────────
 * ARCore does NOT automatically composite the camera feed into the GL surface.
 * [ArRenderer] creates an External OES texture, registers it with the session via
 * [Session.setCameraTextureName], then draws a full-screen quad each frame using
 * that texture. [Frame.transformCoordinates2d] handles orientation and crop so the
 * image is always upright regardless of device rotation.
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
 *
 * Camera background is rendered each frame via a full-screen External OES quad.
 * The quad texture coordinates are recomputed via [Frame.transformCoordinates2d]
 * whenever the display geometry changes (first frame, screen rotation).
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

    // ── Camera background GL state ─────────────────────────────────────────────

    /** External OES texture receiving the ARCore camera stream. */
    private var cameraTextureId = -1

    /** Compiled GLSL program for drawing the camera background quad. */
    private var bgProgram = 0
    private var bgPositionAttr = 0
    private var bgTexCoordAttr = 0
    private var bgTextureUniform = 0

    /**
     * Full-screen quad vertex positions in OpenGL NDC:
     *   (-1,-1)  (1,-1)
     *   (-1, 1)  (1, 1)
     * Arranged for GL_TRIANGLE_STRIP: BL, BR, TL, TR.
     */
    private val quadPosBuf: FloatBuffer = ByteBuffer
        .allocateDirect(4 * 2 * 4)
        .order(ByteOrder.nativeOrder())
        .asFloatBuffer()
        .also { buf ->
            buf.put(floatArrayOf(-1f, -1f,   1f, -1f,   -1f, 1f,   1f, 1f))
            buf.rewind()
        }

    /**
     * Texture coordinates for the camera quad.
     * Populated/updated by [Frame.transformCoordinates2d] on each frame where
     * display geometry changed (first frame + screen rotations).
     */
    private val quadTexBuf: FloatBuffer = ByteBuffer
        .allocateDirect(4 * 2 * 4)
        .order(ByteOrder.nativeOrder())
        .asFloatBuffer()

    /** True until [Frame.transformCoordinates2d] has been called at least once. */
    private var texCoordsReady = false

    // ── GLSurfaceView.Renderer ─────────────────────────────────────────────────

    override fun onSurfaceCreated(gl: GL10?, cfg: EGLConfig?) {
        GLES20.glClearColor(0f, 0f, 0f, 1f)

        // ── Create External OES camera texture ─────────────────────────────────
        // Must be created on the GL thread. setCameraTextureName() registers this
        // texture with the session so ARCore writes camera frames into it.
        val texIds = IntArray(1)
        GLES20.glGenTextures(1, texIds, 0)
        cameraTextureId = texIds[0]
        GLES20.glBindTexture(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, cameraTextureId)
        GLES20.glTexParameteri(GLES11Ext.GL_TEXTURE_EXTERNAL_OES,
            GLES20.GL_TEXTURE_MIN_FILTER, GLES20.GL_LINEAR)
        GLES20.glTexParameteri(GLES11Ext.GL_TEXTURE_EXTERNAL_OES,
            GLES20.GL_TEXTURE_MAG_FILTER, GLES20.GL_LINEAR)
        GLES20.glTexParameteri(GLES11Ext.GL_TEXTURE_EXTERNAL_OES,
            GLES20.GL_TEXTURE_WRAP_S, GLES20.GL_CLAMP_TO_EDGE)
        GLES20.glTexParameteri(GLES11Ext.GL_TEXTURE_EXTERNAL_OES,
            GLES20.GL_TEXTURE_WRAP_T, GLES20.GL_CLAMP_TO_EDGE)
        GLES20.glBindTexture(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, 0)

        // ── Compile background shader ──────────────────────────────────────────
        bgProgram = compileProgram(BG_VERT_SRC, BG_FRAG_SRC)
        bgPositionAttr   = GLES20.glGetAttribLocation(bgProgram,  "a_Position")
        bgTexCoordAttr   = GLES20.glGetAttribLocation(bgProgram,  "a_TexCoord")
        bgTextureUniform = GLES20.glGetUniformLocation(bgProgram, "sTexture")

        // ── ARCore session ─────────────────────────────────────────────────────
        try {
            val availability = ArCoreApk.getInstance().checkAvailability(ctx)
            if (!availability.isSupported) {
                val reason = when (availability) {
                    ArCoreApk.Availability.UNSUPPORTED_DEVICE_NOT_CAPABLE ->
                        "AR not supported on this device — use manual entry."
                    else ->
                        "ARCore not available ($availability) — use manual entry."
                }
                mainHandler.post { onSessionError(reason) }
                return
            }

            val sess = Session(ctx)

            // setCameraTextureName MUST be called before resume().
            sess.setCameraTextureName(cameraTextureId)

            sess.configure(Config(sess).apply {
                planeFindingMode = Config.PlaneFindingMode.HORIZONTAL_AND_VERTICAL
                depthMode = if (sess.isDepthModeSupported(Config.DepthMode.AUTOMATIC))
                    Config.DepthMode.AUTOMATIC else Config.DepthMode.DISABLED
            })
            sess.resume()   // throws CameraNotAvailableException if permission not granted
            session = sess
            mainHandler.post { onSessionReady() }
        } catch (e: UnavailableException) {
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
        // Force UV recompute on next frame after a surface resize.
        texCoordsReady = false
    }

    override fun onDrawFrame(gl: GL10?) {
        GLES20.glClear(GLES20.GL_COLOR_BUFFER_BIT or GLES20.GL_DEPTH_BUFFER_BIT)
        val sess = session ?: return
        // update() is the sole caller for this session — always on the GL thread.
        val frame = runCatching { sess.update() }.getOrNull() ?: return

        // ── Draw camera background ─────────────────────────────────────────────
        // Must happen BEFORE any 3D content so it forms the background layer.
        drawCameraBackground(frame)

        // ── Tap → hit-test ─────────────────────────────────────────────────────
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

    // ── Camera background rendering ────────────────────────────────────────────

    /**
     * Draws the live camera feed as a full-screen background quad.
     *
     * [Frame.transformCoordinates2d] converts NDC vertex positions → texture UVs
     * accounting for camera orientation, device rotation, and aspect ratio crop.
     * It is called only when display geometry changes to avoid unnecessary work.
     */
    private fun drawCameraBackground(frame: Frame) {
        // Recompute texture UVs whenever display geometry changes (first frame,
        // orientation change, surface resize).
        if (frame.hasDisplayGeometryChanged() || !texCoordsReady) {
            quadPosBuf.rewind()
            quadTexBuf.rewind()
            frame.transformCoordinates2d(
                Coordinates2d.OPENGL_NORMALIZED_DEVICE_COORDINATES, quadPosBuf,
                Coordinates2d.TEXTURE_NORMALIZED,                   quadTexBuf,
            )
            quadPosBuf.rewind()
            quadTexBuf.rewind()
            texCoordsReady = true
        }

        // Render without depth writes so the background sits behind everything.
        GLES20.glDisable(GLES20.GL_DEPTH_TEST)
        GLES20.glDepthMask(false)

        GLES20.glUseProgram(bgProgram)

        GLES20.glActiveTexture(GLES20.GL_TEXTURE0)
        GLES20.glBindTexture(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, cameraTextureId)
        GLES20.glUniform1i(bgTextureUniform, 0)

        quadPosBuf.rewind()
        GLES20.glVertexAttribPointer(bgPositionAttr, 2, GLES20.GL_FLOAT, false, 0, quadPosBuf)
        GLES20.glEnableVertexAttribArray(bgPositionAttr)

        quadTexBuf.rewind()
        GLES20.glVertexAttribPointer(bgTexCoordAttr, 2, GLES20.GL_FLOAT, false, 0, quadTexBuf)
        GLES20.glEnableVertexAttribArray(bgTexCoordAttr)

        GLES20.glDrawArrays(GLES20.GL_TRIANGLE_STRIP, 0, 4)

        GLES20.glDisableVertexAttribArray(bgPositionAttr)
        GLES20.glDisableVertexAttribArray(bgTexCoordAttr)
        GLES20.glBindTexture(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, 0)

        GLES20.glDepthMask(true)
        GLES20.glEnable(GLES20.GL_DEPTH_TEST)
    }

    // ── Lifecycle ──────────────────────────────────────────────────────────────

    /** Dispatches [Session.close] to the GL thread via [GLSurfaceView.queueEvent]. */
    fun closeOnGlThread() {
        glView.queueEvent {
            session?.close()
            session = null
        }
    }
}

// ── Background shaders ─────────────────────────────────────────────────────────

/**
 * Passes NDC vertex positions through unchanged and forwards texture coordinates.
 */
private const val BG_VERT_SRC = """
    attribute vec4 a_Position;
    attribute vec2 a_TexCoord;
    varying vec2 v_TexCoord;
    void main() {
        gl_Position = a_Position;
        v_TexCoord  = a_TexCoord;
    }
"""

/**
 * Samples the external OES camera texture.
 * The #extension directive is required to use [GLES11Ext.GL_TEXTURE_EXTERNAL_OES].
 */
private const val BG_FRAG_SRC = """
    #extension GL_OES_EGL_image_external : require
    precision mediump float;
    varying vec2 v_TexCoord;
    uniform samplerExternalOES sTexture;
    void main() {
        gl_FragColor = texture2D(sTexture, v_TexCoord);
    }
"""

// ── GL helpers ─────────────────────────────────────────────────────────────────

private fun compileShader(type: Int, src: String): Int {
    val id = GLES20.glCreateShader(type)
    GLES20.glShaderSource(id, src)
    GLES20.glCompileShader(id)
    return id
}

private fun compileProgram(vertSrc: String, fragSrc: String): Int {
    val vert = compileShader(GLES20.GL_VERTEX_SHADER,   vertSrc)
    val frag = compileShader(GLES20.GL_FRAGMENT_SHADER, fragSrc)
    val prog = GLES20.glCreateProgram()
    GLES20.glAttachShader(prog, vert)
    GLES20.glAttachShader(prog, frag)
    GLES20.glLinkProgram(prog)
    GLES20.glDeleteShader(vert)   // detached from prog, safe to delete
    GLES20.glDeleteShader(frag)
    return prog
}

// ── Geometry helpers ───────────────────────────────────────────────────────────

private fun dist(a: FloatArray, b: FloatArray): Double {
    val dx = (a[0] - b[0]).toDouble()
    val dy = (a[1] - b[1]).toDouble()
    val dz = (a[2] - b[2]).toDouble()
    return sqrt(dx * dx + dy * dy + dz * dz)
}

private fun round1(v: Double): Double = (v * 10).toLong() / 10.0
