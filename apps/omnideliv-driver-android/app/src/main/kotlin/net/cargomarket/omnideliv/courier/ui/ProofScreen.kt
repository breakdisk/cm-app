package net.cargomarket.omnideliv.courier.ui

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageCapture
import androidx.camera.core.ImageCaptureException
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalLifecycleOwner
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import java.io.File

/**
 * The proof photo.
 *
 * The rule this screen exists to honour: the courier advances when the payload
 * is **enqueued**, not when it is uploaded. A delivery in a basement must
 * complete, and the queue is what carries it out later.
 */
@Composable
fun ProofScreen(
    onCaptured: (File) -> Unit,
    onSkip: () -> Unit,
    label: String = "Photo of the delivery",
) {
    val context = LocalContext.current
    var granted by remember { mutableStateOf(hasCamera(context)) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }

    val ask = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted = it }

    LaunchedEffect(Unit) { if (!granted) ask.launch(Manifest.permission.CAMERA) }

    if (!granted) {
        PermissionPrompt(
            onAsk = { ask.launch(Manifest.permission.CAMERA) },
            // A refused permission must not trap a courier mid-delivery. The
            // proof is evidence, and evidence missing is a worse outcome than a
            // delivery that cannot be completed at all — so this is a way out,
            // recorded as a delivery without a photo rather than a delivery
            // that never happened.
            onSkip = onSkip,
        )
        return
    }

    val lifecycleOwner = LocalLifecycleOwner.current
    val capture = remember { ImageCapture.Builder().build() }

    Box(Modifier.fillMaxSize().background(Tokens.Base)) {
        AndroidView(
            modifier = Modifier.fillMaxSize(),
            factory = { ctx ->
                PreviewView(ctx).also { view ->
                    val future = ProcessCameraProvider.getInstance(ctx)
                    future.addListener({
                        val provider = future.get()
                        val preview = Preview.Builder().build()
                            .also { it.setSurfaceProvider(view.surfaceProvider) }
                        provider.unbindAll()
                        provider.bindToLifecycle(
                            lifecycleOwner,
                            CameraSelector.DEFAULT_BACK_CAMERA,
                            preview,
                            capture,
                        )
                    }, ContextCompat.getMainExecutor(ctx))
                }
            },
        )

        Column(
            Modifier
                .align(Alignment.BottomCenter)
                .fillMaxWidth()
                .background(Tokens.Base)
                .padding(16.dp),
        ) {
            Text(label, color = Tokens.Text, fontSize = 15.sp, fontWeight = FontWeight.Bold)
            error?.let {
                Spacer(Modifier.height(6.dp))
                Text(it, color = Tokens.Amber, fontSize = 12.sp)
            }
            Spacer(Modifier.height(10.dp))

            Button(
                onClick = {
                    if (busy) return@Button
                    busy = true
                    error = null
                    // The shutter is the physical event. The file lands, the
                    // caller encodes and enqueues, and only then does the
                    // courier move on.
                    takePhoto(
                        context = context,
                        capture = capture,
                        onOk = { file -> busy = false; onCaptured(file) },
                        onErr = {
                            busy = false
                            error = "Could not take the photo. Try again."
                        },
                    )
                },
                enabled = !busy,
                colors = ButtonDefaults.buttonColors(
                    containerColor = Tokens.Signal,
                    contentColor = Tokens.SignalInk,
                    disabledContainerColor = Tokens.SurfaceRaised,
                    disabledContentColor = Tokens.TextMuted,
                ),
                modifier = Modifier.fillMaxWidth().heightIn(min = Tokens.MinTarget),
            ) {
                if (busy) {
                    CircularProgressIndicator(
                        color = Tokens.SignalInk,
                        strokeWidth = 2.dp,
                        modifier = Modifier.height(20.dp),
                    )
                } else {
                    Text("Take photo", fontSize = 16.sp, fontWeight = FontWeight.Bold)
                }
            }
        }
    }
}

@Composable
private fun PermissionPrompt(onAsk: () -> Unit, onSkip: () -> Unit) {
    Box(Modifier.fillMaxSize().background(Tokens.Base), contentAlignment = Alignment.Center) {
        Column(Modifier.padding(24.dp)) {
            Text(
                "Camera access is needed for delivery proof",
                color = Tokens.Text,
                fontWeight = FontWeight.Bold,
                fontSize = 17.sp,
            )
            Spacer(Modifier.height(6.dp))
            Text(
                "A photo at the door is what settles a dispute about whether an " +
                    "order arrived.",
                color = Tokens.TextMuted,
                fontSize = 13.sp,
                lineHeight = 18.sp,
            )
            Spacer(Modifier.height(18.dp))
            Button(
                onClick = onAsk,
                colors = ButtonDefaults.buttonColors(
                    containerColor = Tokens.Signal,
                    contentColor = Tokens.SignalInk,
                ),
                modifier = Modifier.fillMaxWidth().heightIn(min = Tokens.MinTarget),
            ) {
                Text("Allow camera", fontSize = 15.sp, fontWeight = FontWeight.Bold)
            }
            Spacer(Modifier.height(8.dp))
            Button(
                onClick = onSkip,
                colors = ButtonDefaults.buttonColors(
                    containerColor = Tokens.SurfaceRaised,
                    contentColor = Tokens.Text,
                ),
                modifier = Modifier.fillMaxWidth().heightIn(min = 44.dp),
            ) {
                Text("Continue without a photo", fontSize = 13.sp)
            }
        }
    }
}

private fun hasCamera(context: Context) =
    ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) ==
        PackageManager.PERMISSION_GRANTED

private fun takePhoto(
    context: Context,
    capture: ImageCapture,
    onOk: (File) -> Unit,
    onErr: (Throwable?) -> Unit,
) {
    // Written into the app's own cache: the proof is transient evidence bound
    // for the server, not something to publish into shared storage where a
    // gallery would index a stranger's doorstep.
    val file = File(context.cacheDir, "proof-${System.nanoTime()}.jpg")
    val options = ImageCapture.OutputFileOptions.Builder(file).build()

    capture.takePicture(
        options,
        ContextCompat.getMainExecutor(context),
        object : ImageCapture.OnImageSavedCallback {
            override fun onImageSaved(result: ImageCapture.OutputFileResults) = onOk(file)
            override fun onError(exception: ImageCaptureException) = onErr(exception)
        },
    )
}
