package io.logisticos.driver.feature.pod.ui

import android.graphics.Bitmap
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

private val SignatureCyan = Color(0xFF00E5FF)
private val SignatureGlass = Color(0x0AFFFFFF)

/**
 * Renders all completed stroke paths into a Bitmap using the Android Canvas API.
 * Extracted so both [SignatureCanvas]'s auto-save and any explicit re-render
 * share the same drawing logic without duplication.
 */
private fun renderSignatureBitmap(paths: List<List<Offset>>, width: Int, height: Int): Bitmap {
    val bmp = Bitmap.createBitmap(width.coerceAtLeast(1), height.coerceAtLeast(1), Bitmap.Config.ARGB_8888)
    val canvas = android.graphics.Canvas(bmp)
    val paint = android.graphics.Paint().apply {
        isAntiAlias = true
        color = android.graphics.Color.parseColor("#00E5FF")
        style = android.graphics.Paint.Style.STROKE
        strokeWidth = 6f
        strokeCap = android.graphics.Paint.Cap.ROUND
        strokeJoin = android.graphics.Paint.Join.ROUND
    }
    paths.forEach { path ->
        if (path.size > 1) {
            val p = android.graphics.Path()
            p.moveTo(path.first().x, path.first().y)
            path.drop(1).forEach { point -> p.lineTo(point.x, point.y) }
            canvas.drawPath(p, paint)
        }
    }
    return bmp
}

/**
 * Freehand signature canvas.
 *
 * Auto-saves after every completed stroke (pen-up) so the driver doesn't
 * need to tap a separate "Confirm" button — the Submit button unlocks as
 * soon as the first stroke is drawn, exactly how OTP auto-confirms on the
 * 6th digit. A "Clear" button lets the driver redo if unsatisfied.
 */
@Composable
fun SignatureCanvas(
    onSigned: (Bitmap) -> Unit,
    modifier: Modifier = Modifier
) {
    var paths by remember { mutableStateOf(listOf<List<Offset>>()) }
    var currentPath by remember { mutableStateOf(listOf<Offset>()) }
    var canvasSize by remember { mutableStateOf(IntSize.Zero) }

    Column(modifier = modifier) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(240.dp)
                .background(SignatureGlass)
        ) {
            Canvas(
                modifier = Modifier
                    .fillMaxSize()
                    .pointerInput(Unit) {
                        detectDragGestures(
                            onDragStart = { offset -> currentPath = listOf(offset) },
                            onDrag = { change, _ -> currentPath = currentPath + change.position },
                            onDragEnd = {
                                val updatedPaths = paths + listOf(currentPath)
                                paths = updatedPaths
                                currentPath = emptyList()
                                // Auto-save as soon as the driver lifts their pen.
                                // Mirrors the OTP auto-confirm pattern — no explicit
                                // "Confirm" tap required. The driver can still clear
                                // and redo using the button below.
                                if (canvasSize != IntSize.Zero) {
                                    onSigned(renderSignatureBitmap(updatedPaths, canvasSize.width, canvasSize.height))
                                }
                            }
                        )
                    }
            ) {
                canvasSize = IntSize(size.width.toInt(), size.height.toInt())
                paths.forEach { path ->
                    if (path.size > 1) {
                        val p = Path()
                        p.moveTo(path.first().x, path.first().y)
                        path.drop(1).forEach { p.lineTo(it.x, it.y) }
                        drawPath(p, color = SignatureCyan, style = Stroke(width = 3f, cap = StrokeCap.Round, join = StrokeJoin.Round))
                    }
                }
                if (currentPath.size > 1) {
                    val p = Path()
                    p.moveTo(currentPath.first().x, currentPath.first().y)
                    currentPath.drop(1).forEach { p.lineTo(it.x, it.y) }
                    drawPath(p, color = SignatureCyan, style = Stroke(width = 3f, cap = StrokeCap.Round, join = StrokeJoin.Round))
                }
            }
            if (paths.isEmpty() && currentPath.isEmpty()) {
                Text(
                    "Sign here",
                    color = Color.White.copy(alpha = 0.2f),
                    fontSize = 14.sp,
                    modifier = Modifier.align(Alignment.Center)
                )
            }
        }

        // Single "Clear" button — no "Confirm" needed since auto-save fires on pen-up.
        Button(
            onClick = {
                paths = emptyList()
                currentPath = emptyList()
            },
            enabled = paths.isNotEmpty() || currentPath.isNotEmpty(),
            colors = ButtonDefaults.buttonColors(containerColor = Color.White.copy(alpha = 0.1f)),
            modifier = Modifier
                .fillMaxWidth()
                .padding(top = 8.dp)
        ) {
            Text("Clear & Redo", color = Color.White)
        }
    }
}
