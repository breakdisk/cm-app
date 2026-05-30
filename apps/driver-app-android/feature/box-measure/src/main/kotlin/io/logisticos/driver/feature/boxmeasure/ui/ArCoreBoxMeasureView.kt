package io.logisticos.driver.feature.boxmeasure.ui

import android.content.Context
import android.graphics.PixelFormat
import android.opengl.GLES11Ext
import android.opengl.GLES20
import android.opengl.GLSurfaceView
import android.opengl.Matrix
import android.os.Handler
import android.os.Looper
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
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
 * ── Visual feedback ─────────────────────────────────────────────────────────────
 * Each confirmed tap renders a colored sphere dot (GL_POINTS with round discard).
 * Consecutive dots are connected by a colored line (GL_LINES):
 *   Tap 1→2 (length edge) : CYAN
 *   Tap 2→3 (width  edge) : GREEN
 *   Tap 3→4 (height edge) : PURPLE
 *
 * ── Threading contract ──────────────────────────────────────────────────────────
 * [Session.update] and [Frame.hitTest] are called ONLY inside [onDrawFrame] on the
 * GL thread. Tap events are enqueued via [ConcurrentLinkedQueue] from the UI thread.
 * [Session.close] is dispatched via [GLSurfaceView.queueEvent].
 * ViewModel callbacks are posted to the main thread via [Handler].
 */
@Composable
fun ArCoreBoxMeasureView(
    modifier: Modifier = Modifier,
    viewModel: BoxMeasureViewModel,
) {
    val tapQueue   = remember { ConcurrentLinkedQueue<Pair<Float, Float>>() }
    val rendererRef = remember { mutableStateOf<ArRenderer?>(null) }
    val resetToken = viewModel.uiState.collectAsState().value.resetToken

    DisposableEffect(Unit) {
        onDispose { rendererRef.value?.closeOnGlThread() }
    }

    // Re-measure: when the ViewModel bumps the reset token, drop the renderer's
    // captured tap points and any queued taps so the next tap begins a fresh box.
    LaunchedEffect(resetToken) {
        if (resetToken > 0) {
            tapQueue.clear()
            rendererRef.value?.clearPoints()
        }
    }

    AndroidView(
        // Taps are detected at the Compose layer rather than via the GLSurfaceView's
        // own OnTouchListener. Interop touch dispatch to the embedded view is
        // unreliable when the view is re-hosted — e.g. when the viewport is moved
        // into the full-screen layer — which left tap-to-measure dead in full screen.
        // A Compose tap detector on the node works identically in every layout.
        // `offset` is in this node's local pixels, matching the GL display geometry
        // passed to `frame.hitTest`.
        modifier = modifier.pointerInput(Unit) {
            detectTapGestures { offset ->
                if (viewModel.uiState.value.tapCount < 4) {
                    tapQueue.offer(Pair(offset.x, offset.y))
                }
            }
        },
        factory = { ctx ->
            ArRenderer(
                ctx                  = ctx,
                tapQueue             = tapQueue,
                onSessionReady       = { viewModel.onArSessionReady() },
                onSessionError       = { msg -> viewModel.onArSessionError(msg) },
                onMeasurementPoint   = { idx, x, y, z -> viewModel.onMeasurementPoint(idx, x, y, z) },
                onMeasurementComplete = { l, w, h, conf -> viewModel.onMeasurementComplete(l, w, h, conf) },
            ).also { rendererRef.value = it }.glView
        },
    )
}

// ── Renderer ───────────────────────────────────────────────────────────────────

private class ArRenderer(
    private val ctx: Context,
    private val tapQueue: ConcurrentLinkedQueue<Pair<Float, Float>>,
    private val onSessionReady: () -> Unit,
    private val onSessionError: (String) -> Unit,
    private val onMeasurementPoint: (index: Int, x: Float, y: Float, z: Float) -> Unit,
    private val onMeasurementComplete: (l: Double, w: Double, h: Double, confidence: Double) -> Unit,
) : GLSurfaceView.Renderer {

    val glView: GLSurfaceView = GLSurfaceView(ctx).also { surface ->
        surface.setEGLContextClientVersion(2)
        surface.setEGLConfigChooser(8, 8, 8, 8, 16, 0)
        surface.holder.setFormat(PixelFormat.TRANSLUCENT)
        surface.setRenderer(this)
        surface.renderMode = GLSurfaceView.RENDERMODE_CONTINUOUSLY
    }

    private var session: Session? = null
    private val mainHandler = Handler(Looper.getMainLooper())

    // World-space tap points: p0..p3 owned exclusively by the GL thread.
    private val worldPts = mutableListOf<FloatArray>()

    // ── Camera background ──────────────────────────────────────────────────────

    private var cameraTextureId   = -1
    private var bgProgram         = 0
    private var bgPositionAttr    = 0
    private var bgTexCoordAttr    = 0
    private var bgTextureUniform  = 0

    private val quadPosBuf: FloatBuffer = nativeFloatBuffer(8).also {
        it.put(floatArrayOf(-1f, -1f,  1f, -1f,  -1f, 1f,  1f, 1f)); it.rewind()
    }
    private val quadTexBuf: FloatBuffer = nativeFloatBuffer(8)
    private var texCoordsReady = false

    // ── Marker / line rendering ────────────────────────────────────────────────

    private var markerProgram       = 0
    private var markerMvpUniform    = 0
    private var markerColorUniform  = 0
    private var markerPositionAttr  = 0

    // Reusable scratch buffer: avoids allocating per-draw on the GL thread.
    // Max usage: 6 floats (2 points × 3 components) for a line segment.
    private val scratchBuf: FloatBuffer = nativeFloatBuffer(6)

    private val projMatrix = FloatArray(16)
    private val viewMatrix = FloatArray(16)
    private val mvpMatrix  = FloatArray(16)

    // ── GLSurfaceView.Renderer ─────────────────────────────────────────────────

    override fun onSurfaceCreated(gl: GL10?, cfg: EGLConfig?) {
        GLES20.glClearColor(0f, 0f, 0f, 1f)

        // Camera External OES texture
        val texIds = IntArray(1)
        GLES20.glGenTextures(1, texIds, 0)
        cameraTextureId = texIds[0]
        GLES20.glBindTexture(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, cameraTextureId)
        GLES20.glTexParameteri(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, GLES20.GL_TEXTURE_MIN_FILTER, GLES20.GL_LINEAR)
        GLES20.glTexParameteri(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, GLES20.GL_TEXTURE_MAG_FILTER, GLES20.GL_LINEAR)
        GLES20.glTexParameteri(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, GLES20.GL_TEXTURE_WRAP_S, GLES20.GL_CLAMP_TO_EDGE)
        GLES20.glTexParameteri(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, GLES20.GL_TEXTURE_WRAP_T, GLES20.GL_CLAMP_TO_EDGE)
        GLES20.glBindTexture(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, 0)

        // Background quad shader
        bgProgram        = compileProgram(BG_VERT_SRC, BG_FRAG_SRC)
        bgPositionAttr   = GLES20.glGetAttribLocation(bgProgram,  "a_Position")
        bgTexCoordAttr   = GLES20.glGetAttribLocation(bgProgram,  "a_TexCoord")
        bgTextureUniform = GLES20.glGetUniformLocation(bgProgram, "sTexture")

        // Marker / line shader
        markerProgram      = compileProgram(MARKER_VERT_SRC, MARKER_FRAG_SRC)
        markerMvpUniform   = GLES20.glGetUniformLocation(markerProgram, "u_MVP")
        markerColorUniform = GLES20.glGetUniformLocation(markerProgram, "u_Color")
        markerPositionAttr = GLES20.glGetAttribLocation(markerProgram,  "a_Position")

        // ARCore session
        try {
            val availability = ArCoreApk.getInstance().checkAvailability(ctx)
            if (!availability.isSupported) {
                val msg = when (availability) {
                    ArCoreApk.Availability.UNSUPPORTED_DEVICE_NOT_CAPABLE ->
                        "AR not supported on this device — use manual entry."
                    else -> "ARCore not available ($availability) — use manual entry."
                }
                mainHandler.post { onSessionError(msg) }
                return
            }

            val sess = Session(ctx)
            sess.setCameraTextureName(cameraTextureId)          // must precede resume()
            sess.configure(Config(sess).apply {
                planeFindingMode = Config.PlaneFindingMode.HORIZONTAL_AND_VERTICAL
                depthMode = if (sess.isDepthModeSupported(Config.DepthMode.AUTOMATIC))
                    Config.DepthMode.AUTOMATIC else Config.DepthMode.DISABLED
            })
            sess.resume()
            session = sess
            mainHandler.post { onSessionReady() }
        } catch (e: UnavailableException) {
            mainHandler.post { onSessionError("AR unavailable: ${e.javaClass.simpleName} — use manual entry.") }
        } catch (e: Exception) {
            mainHandler.post { onSessionError("AR failed: ${e.message ?: e.javaClass.simpleName} — use manual entry.") }
        }
    }

    override fun onSurfaceChanged(gl: GL10?, w: Int, h: Int) {
        GLES20.glViewport(0, 0, w, h)
        session?.setDisplayGeometry(0, w, h)
        texCoordsReady = false
    }

    override fun onDrawFrame(gl: GL10?) {
        GLES20.glClear(GLES20.GL_COLOR_BUFFER_BIT or GLES20.GL_DEPTH_BUFFER_BIT)
        val sess  = session ?: return
        val frame = runCatching { sess.update() }.getOrNull() ?: return

        drawCameraBackground(frame)

        if (frame.camera.trackingState == TrackingState.TRACKING) {
            drawMarkersAndLines(frame)
        }

        // Hit-test for the next pending tap
        val tap = tapQueue.poll() ?: return
        if (worldPts.size >= 4) return

        val hits = frame.hitTest(tap.first, tap.second)
        val hit = hits.firstOrNull { h ->
            h.trackable is Plane && (h.trackable as Plane).isPoseInPolygon(h.hitPose)
        } ?: hits.firstOrNull() ?: return

        val p = hit.hitPose
        worldPts.add(floatArrayOf(p.tx(), p.ty(), p.tz()))
        val idx = worldPts.size
        val wx = p.tx(); val wy = p.ty(); val wz = p.tz()
        mainHandler.post { onMeasurementPoint(idx, wx, wy, wz) }

        if (worldPts.size == 4) {
            val (p0, p1, p2, p3) = worldPts
            val l   = dist(p0, p1) * 100
            val w   = dist(p1, p2) * 100
            val hh  = abs((p3[1] - p2[1]).toDouble()) * 100
            val conf = if (frame.camera.trackingState == TrackingState.TRACKING) 0.92 else 0.6
            mainHandler.post { onMeasurementComplete(round1(l), round1(w), round1(hh), conf) }
        }
    }

    // ── Camera background ──────────────────────────────────────────────────────

    private fun drawCameraBackground(frame: Frame) {
        if (frame.hasDisplayGeometryChanged() || !texCoordsReady) {
            quadPosBuf.rewind(); quadTexBuf.rewind()
            frame.transformCoordinates2d(
                Coordinates2d.OPENGL_NORMALIZED_DEVICE_COORDINATES, quadPosBuf,
                Coordinates2d.TEXTURE_NORMALIZED,                   quadTexBuf,
            )
            quadPosBuf.rewind(); quadTexBuf.rewind()
            texCoordsReady = true
        }

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

    // ── Markers & lines ────────────────────────────────────────────────────────

    /**
     * Draws a colored dot at each confirmed world-space tap point and a colored
     * line between consecutive pairs. Colors encode the measurement dimension:
     *   Cyan   — length edge (points 0, 1 and segment 0→1)
     *   Green  — width  edge (point  2 and segment 1→2)
     *   Purple — height edge (point  3 and segment 2→3)
     *
     * MVP = projMatrix × viewMatrix (world points need no additional model transform).
     * gl_PointSize scales with inverse clip-W so dots stay a consistent physical size.
     */
    private fun drawMarkersAndLines(frame: Frame) {
        if (worldPts.isEmpty()) return

        frame.camera.getProjectionMatrix(projMatrix, 0, 0.1f, 100f)
        frame.camera.getViewMatrix(viewMatrix, 0)
        Matrix.multiplyMM(mvpMatrix, 0, projMatrix, 0, viewMatrix, 0)

        GLES20.glUseProgram(markerProgram)
        GLES20.glUniformMatrix4fv(markerMvpUniform, 1, false, mvpMatrix, 0)

        // ── Dots ──────────────────────────────────────────────────────────────
        worldPts.forEachIndexed { idx, pt ->
            GLES20.glUniform4fv(markerColorUniform, 1, DOT_COLORS[idx.coerceAtMost(3)], 0)
            scratchBuf.rewind()
            scratchBuf.put(pt[0]); scratchBuf.put(pt[1]); scratchBuf.put(pt[2])
            scratchBuf.rewind()
            GLES20.glVertexAttribPointer(markerPositionAttr, 3, GLES20.GL_FLOAT, false, 0, scratchBuf)
            GLES20.glEnableVertexAttribArray(markerPositionAttr)
            GLES20.glDrawArrays(GLES20.GL_POINTS, 0, 1)
        }

        // ── Lines between consecutive dots ────────────────────────────────────
        if (worldPts.size >= 2) {
            GLES20.glLineWidth(6f)
            for (i in 0 until worldPts.size - 1) {
                val p0 = worldPts[i]; val p1 = worldPts[i + 1]
                GLES20.glUniform4fv(markerColorUniform, 1, LINE_COLORS[i.coerceAtMost(2)], 0)
                scratchBuf.rewind()
                scratchBuf.put(p0[0]); scratchBuf.put(p0[1]); scratchBuf.put(p0[2])
                scratchBuf.put(p1[0]); scratchBuf.put(p1[1]); scratchBuf.put(p1[2])
                scratchBuf.rewind()
                GLES20.glVertexAttribPointer(markerPositionAttr, 3, GLES20.GL_FLOAT, false, 0, scratchBuf)
                GLES20.glEnableVertexAttribArray(markerPositionAttr)
                GLES20.glDrawArrays(GLES20.GL_LINES, 0, 2)
            }
        }

        GLES20.glDisableVertexAttribArray(markerPositionAttr)
    }

    // ── Lifecycle ──────────────────────────────────────────────────────────────

    fun closeOnGlThread() {
        glView.queueEvent { session?.close(); session = null }
    }

    /** Drops captured world-space tap points on the GL thread (re-measure). */
    fun clearPoints() {
        glView.queueEvent { worldPts.clear() }
    }
}

// ── Shader sources ─────────────────────────────────────────────────────────────

private const val BG_VERT_SRC = """
    attribute vec4 a_Position;
    attribute vec2 a_TexCoord;
    varying vec2 v_TexCoord;
    void main() {
        gl_Position = a_Position;
        v_TexCoord  = a_TexCoord;
    }
"""

private const val BG_FRAG_SRC = """
    #extension GL_OES_EGL_image_external : require
    precision mediump float;
    varying vec2 v_TexCoord;
    uniform samplerExternalOES sTexture;
    void main() {
        gl_FragColor = texture2D(sTexture, v_TexCoord);
    }
"""

/**
 * Marker vertex shader.
 * gl_PointSize uses inverse clip-W so dots maintain a consistent perceived size
 * regardless of how far the tap point is from the camera.
 */
private const val MARKER_VERT_SRC = """
    uniform mat4  u_MVP;
    attribute vec3 a_Position;
    void main() {
        gl_Position  = u_MVP * vec4(a_Position, 1.0);
        gl_PointSize = clamp(70.0 / gl_Position.w, 12.0, 70.0);
    }
"""

/**
 * Marker fragment shader.
 * Discards corners of the GL_POINTS square to produce a circular dot.
 */
private const val MARKER_FRAG_SRC = """
    precision mediump float;
    uniform vec4 u_Color;
    void main() {
        vec2 coord = gl_PointCoord - vec2(0.5);
        if (length(coord) > 0.5) discard;
        gl_FragColor = u_Color;
    }
"""

// ── Measurement dimension color palette ───────────────────────────────────────

/** Dot fill colors indexed by tap point (0–3). */
private val DOT_COLORS = arrayOf(
    floatArrayOf(0.000f, 0.898f, 1.000f, 1f),   // [0] Cyan   — length start
    floatArrayOf(0.000f, 0.898f, 1.000f, 1f),   // [1] Cyan   — length end
    floatArrayOf(0.000f, 1.000f, 0.533f, 1f),   // [2] Green  — width end
    floatArrayOf(0.659f, 0.333f, 0.969f, 1f),   // [3] Purple — height top
)

/** Line colors indexed by segment (0 = length, 1 = width, 2 = height). */
private val LINE_COLORS = arrayOf(
    floatArrayOf(0.000f, 0.898f, 1.000f, 0.85f), // [0] Cyan
    floatArrayOf(0.000f, 1.000f, 0.533f, 0.85f), // [1] Green
    floatArrayOf(0.659f, 0.333f, 0.969f, 0.85f), // [2] Purple
)

// ── GL helpers ─────────────────────────────────────────────────────────────────

private fun nativeFloatBuffer(capacity: Int): FloatBuffer =
    ByteBuffer.allocateDirect(capacity * 4).order(ByteOrder.nativeOrder()).asFloatBuffer()

private fun compileShader(type: Int, src: String): Int =
    GLES20.glCreateShader(type).also { id ->
        GLES20.glShaderSource(id, src)
        GLES20.glCompileShader(id)
    }

private fun compileProgram(vertSrc: String, fragSrc: String): Int {
    val vert = compileShader(GLES20.GL_VERTEX_SHADER,   vertSrc)
    val frag = compileShader(GLES20.GL_FRAGMENT_SHADER, fragSrc)
    return GLES20.glCreateProgram().also { prog ->
        GLES20.glAttachShader(prog, vert)
        GLES20.glAttachShader(prog, frag)
        GLES20.glLinkProgram(prog)
        GLES20.glDeleteShader(vert)
        GLES20.glDeleteShader(frag)
    }
}

// ── Geometry helpers ───────────────────────────────────────────────────────────

private fun dist(a: FloatArray, b: FloatArray): Double {
    val dx = (a[0] - b[0]).toDouble()
    val dy = (a[1] - b[1]).toDouble()
    val dz = (a[2] - b[2]).toDouble()
    return sqrt(dx * dx + dy * dy + dz * dz)
}

private fun round1(v: Double): Double = (v * 10).toLong() / 10.0
