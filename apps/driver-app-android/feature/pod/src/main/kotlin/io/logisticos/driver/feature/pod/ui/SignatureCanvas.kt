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

@Composable
fun SignatureCanvas(
    onSigned: (Bitmap) -> Unit,
    modifier: Modifier = Modifier
) {
    var paths by remember { mutableStateOf(listOf<List<Offset>>()) }
    var currentPath by remember { mutableStateOf(listOf<Offset>()) }
    var canvasSize by remember { mutableStateOf(IntSize.Zero) }

    val cyan = Color(0xFF00E5FF)
    val glass = Color(0x0AFFFFFF)

    Column(modifier = modifier) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(240.dp)
                .background(glass)
        ) {
            Canvas(
                modifier = Modifier
                    .fillMaxSize()
                    .pointerInput(Unit) {
                        detectDragGestures(
                            onDragStart = { offset -> currentPath = listOf(offset) },
                            onDrag = { change, _ -> currentPath = currentPath + change.position },
                            onDragEnd = {
                                paths = paths + listOf(currentPath)
                                currentPath = emptyList()
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
                        drawPath(
                            p,
                            color = cyan,
                            style = Stroke(width = 3f, cap = StrokeCap.Round, join = StrokeJoin.Round)
                        )
                    }
                }
                if (currentPath.size > 1) {
                    val p = Path()
                    p.moveTo(currentPath.first().x, currentPath.first().y)
                    currentPath.drop(1).forEach { p.lineTo(it.x, it.y) }
                    drawPath(
                        p,
                        color = cyan,
                        style = Stroke(width = 3f, cap = StrokeCap.Round, join = StrokeJoin.Round)
                    )
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

        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(top = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            Button(
                onClick = {
                    paths = emptyList()
                    currentPath = emptyList()
                },
                colors = ButtonDefaults.buttonColors(containerColor = Color.White.copy(alpha = 0.1f)),
                modifier = Modifier.weight(1f)
            ) {
                Text("Clear", color = Color.White)
            }

            Button(
                onClick = {
                    val bmp = Bitmap.createBitmap(
                        canvasSize.width.coerceAtLeast(1),
                        canvasSize.height.coerceAtLeast(1),
                        Bitmap.Config.ARGB_8888
                    )
                    onSigned(bmp)
                },
                enabled = paths.isNotEmpty(),
                colors = ButtonDefaults.buttonColors(containerColor = cyan),
                modifier = Modifier.weight(1f)
            ) {
                Text("Confirm", color = Color(0xFF050810))
            }
        }
    }
}
