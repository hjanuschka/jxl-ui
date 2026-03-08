package com.jxlui

import android.net.Uri
import android.os.Bundle
import android.provider.OpenableColumns
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

// Dark theme colors matching desktop jxl-ui
private val BgBase = Color(0xFF111113)
private val BgElevated = Color(0xFF18181B)
private val BgSurface = Color(0xFF202024)
private val TextPrimary = Color(0xFFFAFAFA)
private val TextSecondary = Color(0xFFA1A1AA)
private val TextMuted = Color(0xFF71717A)
private val Accent = Color(0xFF6366F1)

class MainActivity : ComponentActivity() {
    private val viewModel: JxlViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // Handle intent (opened from file manager)
        intent?.data?.let { uri ->
            viewModel.loadFromUri(uri, getFileName(uri))
        }

        setContent {
            MaterialTheme(
                colorScheme = darkColorScheme(
                    primary = Accent,
                    surface = BgBase,
                    background = BgBase,
                )
            ) {
                JxlViewerScreen(viewModel)
            }
        }
    }

    private fun getFileName(uri: Uri): String? {
        contentResolver.query(uri, null, null, null, null)?.use { cursor ->
            val nameIndex = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
            if (cursor.moveToFirst() && nameIndex >= 0) {
                return cursor.getString(nameIndex)
            }
        }
        return uri.lastPathSegment
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun JxlViewerScreen(viewModel: JxlViewModel) {
    val state by viewModel.state.collectAsState()
    var showInfo by remember { mutableStateOf(false) }

    val filePicker = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocument()
    ) { uri: Uri? ->
        uri?.let { viewModel.loadFromUri(it) }
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(BgBase)
    ) {
        // Main content
        when {
            state.isLoading -> {
                // Loading
                Column(
                    modifier = Modifier.align(Alignment.Center),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    CircularProgressIndicator(color = Accent)
                    Spacer(Modifier.height(16.dp))
                    Text("Decoding...", color = TextMuted, fontSize = 14.sp)
                }
            }

            state.error != null -> {
                // Error
                Column(
                    modifier = Modifier
                        .align(Alignment.Center)
                        .padding(32.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Text("Failed to load", color = TextPrimary, fontSize = 18.sp)
                    Spacer(Modifier.height(8.dp))
                    Text(
                        state.error ?: "",
                        color = TextMuted,
                        fontSize = 13.sp,
                        textAlign = TextAlign.Center,
                    )
                }
            }

            state.bitmap != null -> {
                // Image with pinch-to-zoom and pan
                var scale by remember { mutableFloatStateOf(1f) }
                var offsetX by remember { mutableFloatStateOf(0f) }
                var offsetY by remember { mutableFloatStateOf(0f) }

                Image(
                    bitmap = state.bitmap!!.asImageBitmap(),
                    contentDescription = state.fileName,
                    contentScale = ContentScale.Fit,
                    modifier = Modifier
                        .fillMaxSize()
                        .pointerInput(Unit) {
                            detectTransformGestures { _, pan, zoom, _ ->
                                scale = (scale * zoom).coerceIn(0.5f, 20f)
                                offsetX += pan.x
                                offsetY += pan.y
                            }
                        }
                        .pointerInput(Unit) {
                            detectTapGestures(
                                onDoubleTap = {
                                    // Double-tap to reset
                                    scale = 1f
                                    offsetX = 0f
                                    offsetY = 0f
                                },
                                onTap = {
                                    showInfo = !showInfo
                                }
                            )
                        }
                        .graphicsLayer(
                            scaleX = scale,
                            scaleY = scale,
                            translationX = offsetX,
                            translationY = offsetY,
                        )
                )
            }

            else -> {
                // Empty state
                Column(
                    modifier = Modifier.align(Alignment.Center),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Text("JXL Viewer", color = TextPrimary, fontSize = 24.sp, fontWeight = FontWeight.Bold)
                    Spacer(Modifier.height(8.dp))
                    Text("Powered by jxl-rs", color = TextMuted, fontSize = 13.sp)
                    Spacer(Modifier.height(32.dp))
                    Button(
                        onClick = { filePicker.launch(arrayOf("*/*")) },
                        colors = ButtonDefaults.buttonColors(containerColor = Accent),
                        shape = RoundedCornerShape(12.dp),
                    ) {
                        Text("Open JXL File", fontSize = 16.sp)
                    }
                }
            }
        }

        // Top bar
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .statusBarsPadding()
                .padding(horizontal = 16.dp, vertical = 8.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // File name
            Text(
                text = state.fileName ?: "JXL-UI",
                color = TextPrimary,
                fontSize = 16.sp,
                fontWeight = FontWeight.Medium,
                maxLines = 1,
            )

            // Open button
            IconButton(
                onClick = { filePicker.launch(arrayOf("*/*")) },
                modifier = Modifier
                    .size(40.dp)
                    .clip(CircleShape)
                    .background(BgSurface),
            ) {
                Text("+", color = TextPrimary, fontSize = 20.sp)
            }
        }

        // Info overlay
        AnimatedVisibility(
            visible = showInfo && state.bitmap != null,
            enter = fadeIn(),
            exit = fadeOut(),
            modifier = Modifier
                .align(Alignment.BottomStart)
                .padding(16.dp)
                .navigationBarsPadding(),
        ) {
            Surface(
                color = BgElevated.copy(alpha = 0.9f),
                shape = RoundedCornerShape(12.dp),
            ) {
                Column(modifier = Modifier.padding(16.dp)) {
                    Text("Image Info", color = TextPrimary, fontSize = 14.sp, fontWeight = FontWeight.Bold)
                    Spacer(Modifier.height(8.dp))

                    InfoRow("Size", "${state.width} x ${state.height}")
                    InfoRow("Megapixels", "%.2f MP".format(state.width.toLong() * state.height / 1_000_000.0))
                    InfoRow("Decode time", "${state.decodeTimeMs} ms")
                    if (state.fileSizeBytes > 0) {
                        val kb = state.fileSizeBytes / 1024.0
                        InfoRow("File size", "%.1f KB".format(kb))
                    }
                    if (state.decodeTimeMs > 0) {
                        val mpps = (state.width.toLong() * state.height) / (state.decodeTimeMs / 1000.0) / 1_000_000.0
                        InfoRow("Speed", "%.1f MP/s".format(mpps))
                    }
                }
            }
        }

        // Bottom status bar
        if (state.bitmap != null) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .align(Alignment.BottomCenter)
                    .background(BgElevated.copy(alpha = 0.8f))
                    .navigationBarsPadding()
                    .padding(horizontal = 16.dp, vertical = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text("${state.width}x${state.height}", color = TextMuted, fontSize = 12.sp)
                Text("${state.decodeTimeMs}ms", color = TextMuted, fontSize = 12.sp)
            }
        }
    }
}

@Composable
fun InfoRow(label: String, value: String) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 2.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(label, color = TextMuted, fontSize = 12.sp)
        Text(value, color = TextSecondary, fontSize = 12.sp)
    }
}
