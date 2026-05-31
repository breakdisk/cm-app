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
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.viewinterop.AndroidView
import com.google.ar.core.*
import com.google.ar.core.exceptions.*
import io.logisticos.driver.feature.boxmeasure.presentation.BoxMeasureViewModel
import io.logisticos.driver.feature.boxmeasure.presentation.DimAxis
import io.logisticos.driver.feature.boxmeasure.presentation.DimLabel
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

    Box(modifier = modifier) {
        AndroidView(
            modifier = Modifier.fillMaxSize(),
            factory = { ctx ->
                ArRenderer(
                    ctx                  = ctx,
                    tapQueue             = tapQueue,
                    onSessionReady       = { viewModel.onArSessionReady() },
                    onSessionError       = { msg -> viewModel.onArSessionError(msg) },
                    onMeasurementPoint   = { idx, x, y, z -> viewModel.onMeasurementPoint(idx, x, y, z) },
                    onMeasurementComplete = { l, w, h, conf -> viewModel.onMeasurementComplete(l, w, h, conf) },
                    onLiveDistance       = { cm -> viewModel.onLiveDistance(cm) },
                    onDimLabels          = { labels -> viewModel.onDimLabels(labels) },
                ).also { rendererRef.value = it }.glView
            },
        )

        // Transparent tap-capture layer ON TOP of the GLSurfaceView. A dedicated
        // Compose node reliably receives gestures in every layout — unlike
        // `pointerInput` placed on the AndroidView itself, where interop touch
        // dispatch to the embedded GLSurfaceView is unreliable and left tap-to-place
        // dead. `detectTapGestures` ignores vertical drags, so the parent scroll
        // still works when the viewport is inline. `offset` is in local pixels,
        // matching the GL display geometry passed to `frame.hitTest`.
        Box(
            modifier = Modifier
                .fillMaxSize()
                .pointerInput(Unit) {
                    detectTapGestures { offset ->
                        if (viewModel.uiState.value.tapCount < 4) {
                            tapQueue.offer(Pair(offset.x, offset.y))
                        }
                    }
                }
        )
    }
}

// ── Renderer ───────────────────────────────────────────────────────────────────

private class ArRenderer(
    private val ctx: Context,
    private val tapQueue: ConcurrentLinkedQueue<Pair<Float, Float>>,
    private val onSessionReady: () -> Unit,
    private val onSessionError: (String) -> Unit,
    private val onMeasurementPoint: (index: Int, x: Float, y: Float, z: Float) -> Unit,
    private val onMeasurementComplete: (l: Double, w: Double, h: Double, confidence: Double) -> Unit,
    /** Euclidean distance in cm from the last placed anchor to the current aim point.
     *  Null when no anchor is placed yet or when the centre hit-test misses a plane. */
    private val onLiveDistance: (cm: Double?) -> Unit,
    private val onDimLabels: (labels: List<DimLabel>) -> Unit,
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

    // Viewport size (set in onSurfaceChanged) + reticle throttle. Used to hit-test
    // the screen centre each frame and report live X/Y/Z under the reticle.
    private var viewW = 0
    private var viewH = 0
    private var lastReticleMs = 0L

    // Dimension-label projection: scratch vectors + throttle for posting the 2D
    // screen positions of the cuboid edge midpoints to Compose (the floating chips).
    private val worldTmp = FloatArray(4)
    private val clipTmp  = FloatArray(4)
    private var lastLabelMs = 0L

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

    // ── Face (semi-transparent surface) shader ─────────────────────────────
    // Separate from markerProgram — no gl_PointCoord discard so GL_TRIANGLE_FAN
    // fragments are never culled by the circular-dot clip in the marker shader.
    private var faceProgram         = 0
    private var faceMvpUniform      = 0
    private var faceColorUniform    = 0
    private var facePositionAttr    = 0

    // ── Grid (spatial-mapping dots) shader ─────────────────────────────────
    // Fixed 3-px point size — the adaptive size from markerProgram would make
    // distant grid dots unreadably large or nearby ones bloated.
    private var gridProgram         = 0
    private var gridMvpUniform      = 0
    private var gridColorUniform    = 0
    private var gridPositionAttr    = 0

    // Reusable scratch buffer: avoids allocating per-draw on the GL thread.
    // Max usage: 6 floats (2 points × 3 components) for a line segment.
    private val scratchBuf: FloatBuffer = nativeFloatBuffer(6)

    // Face quad buffer: 4 vertices × 3 components for GL_TRIANGLE_FAN face draws.
    private val faceBuf: FloatBuffer = nativeFloatBuffer(12)

    // Floor grid buffer: 21 × 21 = 441 dots × 3 components each.
    private val gridBuf: FloatBuffer = nativeFloatBuffer(1323)

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

        // Face shader — identical vertex transform but no gl_PointCoord discard in frag.
        faceProgram      = compileProgram(MARKER_VERT_SRC, FACE_FRAG_SRC)
        faceMvpUniform   = GLES20.glGetUniformLocation(faceProgram, "u_MVP")
        faceColorUniform = GLES20.glGetUniformLocation(faceProgram, "u_Color")
        facePositionAttr = GLES20.glGetAttribLocation(faceProgram,  "a_Position")

        // Grid shader — fixed 3-px points for the spatial-mapping dot matrix.
        gridProgram      = compileProgram(GRID_VERT_SRC, MARKER_FRAG_SRC)
        gridMvpUniform   = GLES20.glGetUniformLocation(gridProgram, "u_MVP")
        gridColorUniform = GLES20.glGetUniformLocation(gridProgram, "u_Color")
        gridPositionAttr = GLES20.glGetAttribLocation(gridProgram,  "a_Position")

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
        viewW = w
        viewH = h
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

        // Live reticle: hit-test the screen centre and report world X/Y/Z (throttled
        // to ~6 Hz to keep recomposition cheap). Drives the on-screen coordinate
        // readout that replaces the static bullseye. Must run before the tap-poll
        // early-return below, which fires every frame there is no pending tap.
        val nowMs = System.currentTimeMillis()
        if (nowMs - lastReticleMs >= 150 && worldPts.size < 4) {
            lastReticleMs = nowMs
            if (frame.camera.trackingState == TrackingState.TRACKING && viewW > 0) {
                val cHits = frame.hitTest(viewW / 2f, viewH / 2f)
                val cHit = cHits.firstOrNull { h ->
                    h.trackable is Plane && (h.trackable as Plane).isPoseInPolygon(h.hitPose)
                } ?: cHits.firstOrNull()
                // Compute Euclidean distance from the last placed anchor to the aim point
                // and convert to cm.  If no anchor is placed yet (worldPts empty) post null
                // so the reticle shows the "Tap to place anchor" hint instead of a value.
                val liveCm: Double? = if (cHit != null && worldPts.isNotEmpty()) {
                    val cp   = cHit.hitPose
                    val last = worldPts.last()
                    val dx   = (cp.tx() - last[0]).toDouble()
                    val dy   = (cp.ty() - last[1]).toDouble()
                    val dz   = (cp.tz() - last[2]).toDouble()
                    sqrt(dx * dx + dy * dy + dz * dz) * 100.0
                } else null
                mainHandler.post { onLiveDistance(liveCm) }
            }
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

        // ── Spatial floor grid (drawn first — behind all markers and edges) ───
        drawFloorGrid()

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

        // ── Edges ─────────────────────────────────────────────────────────────
        GLES20.glLineWidth(6f)
        val corners = cuboidCorners()
        if (corners == null) {
            // Progressive feedback: colored polyline between consecutive taps.
            for (i in 0 until worldPts.size - 1) {
                drawSolidEdge(worldPts[i], worldPts[i + 1], LINE_COLORS[i.coerceAtMost(2)])
            }
            // Y guide: vertical green reference line + tick-ruler when the driver
            // is about to place tap 4 (height measurement).
            if (worldPts.size == 3) drawYGuide()
        } else {
            // Full AR dimensioning cuboid: semi-transparent faces first (depth no-write),
            // then opaque colored wireframe on top.
            drawBoxFaces(corners)
            drawBoxWireframe(corners)
            postDimLabels(corners)
        }

        GLES20.glDisableVertexAttribArray(markerPositionAttr)
    }

    /**
     * Builds the 8 cuboid corners from the 4 measured taps, or null until 4 exist.
     * worldPts = [p0 length-start, p1 length-end, p2 width-end, p3 height-top].
     * Base rectangle is anchored at p1: length = p1→p0, width = p1→p2; the box is
     * extruded vertically by the measured height |p3.y − p2.y|.
     * Returns [r0, r1, r2, r3, r0t, r1t, r2t, r3t] (base CCW, then the top face).
     */
    private fun cuboidCorners(): Array<FloatArray>? {
        if (worldPts.size < 4) return null
        val p0 = worldPts[0]; val p1 = worldPts[1]; val p2 = worldPts[2]; val p3 = worldPts[3]
        val lx = p0[0] - p1[0]; val ly = p0[1] - p1[1]; val lz = p0[2] - p1[2]   // length vec p1→p0
        val wx = p2[0] - p1[0]; val wy = p2[1] - p1[1]; val wz = p2[2] - p1[2]   // width  vec p1→p2
        val h  = abs(p3[1] - p2[1])                                              // height (vertical)
        val r0 = floatArrayOf(p1[0],           p1[1],           p1[2])
        val r1 = floatArrayOf(p1[0] + lx,      p1[1] + ly,      p1[2] + lz)      // = p0
        val r2 = floatArrayOf(p1[0] + lx + wx, p1[1] + ly + wy, p1[2] + lz + wz)
        val r3 = floatArrayOf(p1[0] + wx,      p1[1] + wy,      p1[2] + wz)      // = p2
        fun up(p: FloatArray) = floatArrayOf(p[0], p[1] + h, p[2])
        return arrayOf(r0, r1, r2, r3, up(r0), up(r1), up(r2), up(r3))
    }

    private fun drawBoxWireframe(c: Array<FloatArray>) {
        val r0 = c[0]; val r1 = c[1]; val r2 = c[2]; val r3 = c[3]
        val t0 = c[4]; val t1 = c[5]; val t2 = c[6]; val t3 = c[7]
        // Measured edges — solid + colored.
        drawSolidEdge(r0, r1, COL_LENGTH)   // length
        drawSolidEdge(r3, r0, COL_WIDTH)    // width
        drawSolidEdge(r3, t3, COL_HEIGHT)   // height (vertical at the width-end corner)
        // Remaining 9 edges — dashed white silhouette.
        drawDashedEdge(r1, r2); drawDashedEdge(r2, r3)
        drawDashedEdge(t0, t1); drawDashedEdge(t1, t2); drawDashedEdge(t2, t3); drawDashedEdge(t3, t0)
        drawDashedEdge(r0, t0); drawDashedEdge(r1, t1); drawDashedEdge(r2, t2)
    }

    private fun drawSolidEdge(a: FloatArray, b: FloatArray, color: FloatArray) {
        GLES20.glUniform4fv(markerColorUniform, 1, color, 0)
        scratchBuf.rewind()
        scratchBuf.put(a[0]); scratchBuf.put(a[1]); scratchBuf.put(a[2])
        scratchBuf.put(b[0]); scratchBuf.put(b[1]); scratchBuf.put(b[2])
        scratchBuf.rewind()
        GLES20.glVertexAttribPointer(markerPositionAttr, 3, GLES20.GL_FLOAT, false, 0, scratchBuf)
        GLES20.glEnableVertexAttribArray(markerPositionAttr)
        GLES20.glDrawArrays(GLES20.GL_LINES, 0, 2)
    }

    /** Dashed edge: draws alternate sub-segments along a→b in [color] (default: dashed white). */
    private fun drawDashedEdge(a: FloatArray, b: FloatArray, color: FloatArray = COL_DASH) {
        GLES20.glUniform4fv(markerColorUniform, 1, color, 0)
        val dashes = 11
        var i = 0
        while (i < dashes) {
            val s = i.toFloat() / dashes
            val e = (i + 1).toFloat() / dashes
            scratchBuf.rewind()
            scratchBuf.put(a[0] + (b[0] - a[0]) * s); scratchBuf.put(a[1] + (b[1] - a[1]) * s); scratchBuf.put(a[2] + (b[2] - a[2]) * s)
            scratchBuf.put(a[0] + (b[0] - a[0]) * e); scratchBuf.put(a[1] + (b[1] - a[1]) * e); scratchBuf.put(a[2] + (b[2] - a[2]) * e)
            scratchBuf.rewind()
            GLES20.glVertexAttribPointer(markerPositionAttr, 3, GLES20.GL_FLOAT, false, 0, scratchBuf)
            GLES20.glEnableVertexAttribArray(markerPositionAttr)
            GLES20.glDrawArrays(GLES20.GL_LINES, 0, 2)
            i += 2
        }
    }

    // ── Spatial floor grid ────────────────────────────────────────────────────

    /**
     * Renders a 21×21 dot-matrix spatial-mapping grid on the floor plane.
     * The floor Y is taken from the first placed anchor, spacing is 7 cm.
     * Uses [gridProgram] (fixed 3-px point size) so the dots stay small and
     * non-intrusive regardless of camera distance.
     */
    private fun drawFloorGrid() {
        if (worldPts.isEmpty()) return
        val origin  = worldPts[0]
        val groundY = origin[1]
        val half    = 10
        val spacing = 0.07f   // 7 cm between grid dots
        var count   = 0
        gridBuf.rewind()
        for (i in -half..half) {
            for (j in -half..half) {
                gridBuf.put(origin[0] + i * spacing)
                gridBuf.put(groundY)
                gridBuf.put(origin[2] + j * spacing)
                count++
            }
        }
        gridBuf.rewind()
        GLES20.glUseProgram(gridProgram)
        GLES20.glUniformMatrix4fv(gridMvpUniform, 1, false, mvpMatrix, 0)
        GLES20.glUniform4fv(gridColorUniform, 1, COL_GRID, 0)
        GLES20.glVertexAttribPointer(gridPositionAttr, 3, GLES20.GL_FLOAT, false, 0, gridBuf)
        GLES20.glEnableVertexAttribArray(gridPositionAttr)
        GLES20.glDrawArrays(GLES20.GL_POINTS, 0, count)
        GLES20.glDisableVertexAttribArray(gridPositionAttr)
        // Restore marker program so subsequent draws (dots, edges) work correctly.
        GLES20.glUseProgram(markerProgram)
        GLES20.glUniformMatrix4fv(markerMvpUniform, 1, false, mvpMatrix, 0)
    }

    // ── Volumetric box faces ──────────────────────────────────────────────────

    /**
     * Renders 6 semi-transparent face quads over the completed bounding box using
     * alpha blending.  Face colour is keyed to the adjacent measured axis:
     *   Blue faces  — length axis (front + back, parallel to the blue edge)
     *   Red  faces  — width  axis (left  + right, parallel to the red edge)
     *   Neutral     — top / bottom
     *
     * Must be called BEFORE [drawBoxWireframe] so the opaque wireframe renders
     * on top of the transparent tint.  Uses [faceProgram] (no gl_PointCoord discard)
     * so GL_TRIANGLE_FAN fragments are never culled.
     */
    private fun drawBoxFaces(c: Array<FloatArray>) {
        val r0 = c[0]; val r1 = c[1]; val r2 = c[2]; val r3 = c[3]
        val t0 = c[4]; val t1 = c[5]; val t2 = c[6]; val t3 = c[7]
        GLES20.glUseProgram(faceProgram)
        GLES20.glUniformMatrix4fv(faceMvpUniform, 1, false, mvpMatrix, 0)
        GLES20.glEnable(GLES20.GL_BLEND)
        GLES20.glBlendFunc(GLES20.GL_SRC_ALPHA, GLES20.GL_ONE_MINUS_SRC_ALPHA)
        GLES20.glDepthMask(false)                       // don't write to depth for transparent geometry
        // Top / bottom — neutral
        drawQuadFace(r0, r1, r2, r3, COL_FACE_TOP)
        drawQuadFace(t0, t1, t2, t3, COL_FACE_TOP)
        // Length-axis faces (r0↔r1 = blue edge; front + back) — blue tint
        drawQuadFace(r0, r1, t1, t0, COL_FACE_LEN)
        drawQuadFace(r2, r3, t3, t2, COL_FACE_LEN)
        // Width-axis faces (r1↔r2 = red edge; left + right) — red tint
        drawQuadFace(r1, r2, t2, t1, COL_FACE_WID)
        drawQuadFace(r3, r0, t0, t3, COL_FACE_WID)
        GLES20.glDisableVertexAttribArray(facePositionAttr)
        GLES20.glDepthMask(true)
        GLES20.glDisable(GLES20.GL_BLEND)
        // Restore marker program for the wireframe draw that follows.
        GLES20.glUseProgram(markerProgram)
        GLES20.glUniformMatrix4fv(markerMvpUniform, 1, false, mvpMatrix, 0)
    }

    private fun drawQuadFace(a: FloatArray, b: FloatArray, c: FloatArray, d: FloatArray, col: FloatArray) {
        GLES20.glUniform4fv(faceColorUniform, 1, col, 0)
        faceBuf.rewind()
        faceBuf.put(a[0]); faceBuf.put(a[1]); faceBuf.put(a[2])
        faceBuf.put(b[0]); faceBuf.put(b[1]); faceBuf.put(b[2])
        faceBuf.put(c[0]); faceBuf.put(c[1]); faceBuf.put(c[2])
        faceBuf.put(d[0]); faceBuf.put(d[1]); faceBuf.put(d[2])
        faceBuf.rewind()
        GLES20.glVertexAttribPointer(facePositionAttr, 3, GLES20.GL_FLOAT, false, 0, faceBuf)
        GLES20.glEnableVertexAttribArray(facePositionAttr)
        GLES20.glDrawArrays(GLES20.GL_TRIANGLE_FAN, 0, 4)
    }

    // ── Y-axis measurement guide ──────────────────────────────────────────────

    /**
     * Y guide: shown when [worldPts].size == 3 (driver is about to tap box top).
     * Renders a dashed green vertical line rising 1.5 m from the width-end anchor
     * (worldPts[2]) plus horizontal tick marks every 12 cm — a precision ruler the
     * driver can use to align the camera crosshair to the exact box-top height.
     */
    private fun drawYGuide() {
        val p        = worldPts[2]
        val guideTop = floatArrayOf(p[0], p[1] + 1.5f, p[2])
        // Dashed vertical guide line in green
        drawDashedEdge(p, guideTop, COL_HEIGHT)
        // Horizontal tick marks at 12 cm intervals (8 ticks = 0 → 96 cm)
        val hw = 0.030f   // 3 cm half-width each side of the guide axis
        for (step in 1..8) {
            val ty = p[1] + step * 0.12f
            val ta = floatArrayOf(p[0] - hw, ty, p[2])
            val tb = floatArrayOf(p[0] + hw, ty, p[2])
            drawSolidEdge(ta, tb, COL_Y_TICK)
        }
    }

    /**
     * Projects the 3 measured edge midpoints to screen pixels and posts them (with
     * their cm value) to Compose for the floating dimension chips. Throttled to keep
     * recomposition cheap; midpoints behind the camera are dropped.
     */
    private fun postDimLabels(c: Array<FloatArray>) {
        val now = System.currentTimeMillis()
        if (now - lastLabelMs < 60) return
        lastLabelMs = now
        val p0 = worldPts[0]; val p1 = worldPts[1]; val p2 = worldPts[2]; val p3 = worldPts[3]
        val lenCm = dist(p0, p1) * 100
        val widCm = dist(p1, p2) * 100
        val hgtCm = abs((p3[1] - p2[1]).toDouble()) * 100
        val labels = listOfNotNull(
            projectMidLabel(c[0], c[1], DimAxis.LENGTH, lenCm),  // length mid(r0, r1)
            projectMidLabel(c[3], c[0], DimAxis.WIDTH,  widCm),  // width  mid(r3, r0)
            projectMidLabel(c[3], c[7], DimAxis.HEIGHT, hgtCm),  // height mid(r3, t3)
        )
        mainHandler.post { onDimLabels(labels) }
    }

    private fun projectMidLabel(a: FloatArray, b: FloatArray, axis: DimAxis, cm: Double): DimLabel? {
        worldTmp[0] = (a[0] + b[0]) * 0.5f
        worldTmp[1] = (a[1] + b[1]) * 0.5f
        worldTmp[2] = (a[2] + b[2]) * 0.5f
        worldTmp[3] = 1f
        Matrix.multiplyMV(clipTmp, 0, mvpMatrix, 0, worldTmp, 0)
        val w = clipTmp[3]
        if (w <= 0.0001f || viewW == 0) return null   // behind camera / not laid out
        val ndcX = clipTmp[0] / w
        val ndcY = clipTmp[1] / w
        val sx = (ndcX * 0.5f + 0.5f) * viewW
        val sy = (1f - (ndcY * 0.5f + 0.5f)) * viewH
        return DimLabel(axis, sx, sy, cm)
    }

    // ── Lifecycle ──────────────────────────────────────────────────────────────

    fun closeOnGlThread() {
        glView.queueEvent { session?.close(); session = null }
    }

    /** Drops captured world-space tap points on the GL thread (re-measure). */
    fun clearPoints() {
        glView.queueEvent {
            worldPts.clear()
            mainHandler.post { onDimLabels(emptyList()) }
        }
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
 * (Used for point sprites only — not for triangles or lines.)
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

/**
 * Face fragment shader — same colour output but NO gl_PointCoord discard.
 * Used for GL_TRIANGLE_FAN face quads where gl_PointCoord is undefined; the
 * discard in MARKER_FRAG_SRC would cull triangle fragments on most GPUs.
 */
private const val FACE_FRAG_SRC = """
    precision mediump float;
    uniform vec4 u_Color;
    void main() { gl_FragColor = u_Color; }
"""

/**
 * Grid vertex shader — fixed 3-px point size for the spatial-mapping dot matrix.
 * The adaptive `clamp(70/w, ...)` in MARKER_VERT_SRC makes distant grid dots huge;
 * a tiny fixed size keeps the grid subtle and non-intrusive.
 */
private const val GRID_VERT_SRC = """
    uniform mat4  u_MVP;
    attribute vec3 a_Position;
    void main() {
        gl_Position  = u_MVP * vec4(a_Position, 1.0);
        gl_PointSize = 3.0;
    }
"""

// ── Measurement dimension color palette ───────────────────────────────────────
// Industrial axis palette (matches Compose DimChip tokens exactly):
//   Length → #2196F3 blue    Width → #F44336 red    Height → #00FF88 green
// Non-measured cuboid edges render as dashed white.

private val COL_LENGTH = floatArrayOf(0.129f, 0.588f, 0.953f, 1f)   // #2196F3 blue
private val COL_WIDTH  = floatArrayOf(0.957f, 0.263f, 0.212f, 1f)   // #F44336 red
private val COL_HEIGHT = floatArrayOf(0.000f, 1.000f, 0.533f, 1f)   // #00FF88 green
private val COL_DASH   = floatArrayOf(1.000f, 1.000f, 1.000f, 0.75f) // dashed white silhouette

// Semi-transparent face tints — alpha ~0.09 for volumetric depth cue
private val COL_FACE_LEN = floatArrayOf(0.129f, 0.588f, 0.953f, 0.09f)  // blue face tint
private val COL_FACE_WID = floatArrayOf(0.957f, 0.263f, 0.212f, 0.09f)  // red  face tint
private val COL_FACE_TOP = floatArrayOf(0.800f, 0.900f, 1.000f, 0.05f)  // neutral top/bottom tint

// Spatial-mapping floor grid: faint blue dots
private val COL_GRID   = floatArrayOf(0.129f, 0.588f, 0.953f, 0.22f)

// Y guide tick marks: semi-transparent green
private val COL_Y_TICK = floatArrayOf(0.000f, 1.000f, 0.533f, 0.42f)

/** Dot fill colors indexed by tap point (0–3): length start/end, width end, height top. */
private val DOT_COLORS = arrayOf(COL_LENGTH, COL_LENGTH, COL_WIDTH, COL_HEIGHT)

/** Progressive polyline colors indexed by segment (0 = length, 1 = width, 2 = height). */
private val LINE_COLORS = arrayOf(COL_LENGTH, COL_WIDTH, COL_HEIGHT)

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
