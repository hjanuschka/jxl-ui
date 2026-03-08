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
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.material3.SliderDefaults
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
import androidx.compose.ui.text.style.TextOverflow
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

@Composable
fun JxlViewerScreen(viewModel: JxlViewModel) {
    val state by viewModel.state.collectAsState()
    var showInfo by remember { mutableStateOf(false) }
    val filePicker = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocument()
    ) { uri: Uri? ->
        uri?.let {
            viewModel.loadFromUri(it)
        }
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(BgBase)
    ) {
        // Main content
        when {
            state.isLoading -> {
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
                    Spacer(Modifier.height(24.dp))
                    Button(
                        onClick = {
                            viewModel.clearImage()
                        },
                        colors = ButtonDefaults.buttonColors(containerColor = BgSurface),
                        shape = RoundedCornerShape(12.dp),
                    ) {
                        Text("Back to gallery", color = TextSecondary)
                    }
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
                // Gallery / empty state
                SampleGallery(
                    samples = viewModel.sampleFiles,
                    onSelect = { name ->
                        viewModel.loadSample(name)
                    },
                    onOpenFile = { filePicker.launch(arrayOf("*/*")) },
                    modifier = Modifier
                        .fillMaxSize()
                        .statusBarsPadding()
                        .padding(top = 56.dp)
                )
            }
        }

        // Top bar
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .background(BgElevated.copy(alpha = 0.95f))
                .statusBarsPadding()
                .padding(horizontal = 16.dp, vertical = 8.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (state.bitmap != null) {
                // Back to gallery
                IconButton(
                    onClick = {
                        viewModel.clearImage()
                        showInfo = false
                    },
                    modifier = Modifier
                        .size(40.dp)
                        .clip(CircleShape)
                        .background(BgSurface),
                ) {
                    Text("<", color = TextPrimary, fontSize = 18.sp)
                }
                Spacer(Modifier.width(12.dp))
            }

            Text(
                text = state.fileName ?: "JXL Viewer",
                color = TextPrimary,
                fontSize = 16.sp,
                fontWeight = FontWeight.Medium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )

            // Open file button
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
                        InfoRow("File size", "%.1f KB".format(state.fileSizeBytes / 1024.0))
                    }
                    if (state.decodeTimeMs > 0) {
                        val mpps = (state.width.toLong() * state.height) / (state.decodeTimeMs / 1000.0) / 1_000_000.0
                        InfoRow("Speed", "%.1f MP/s".format(mpps))
                    }
                }
            }
        }

        // Bottom bar: status + animation controls
        if (state.bitmap != null) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .align(Alignment.BottomCenter)
                    .background(BgElevated.copy(alpha = 0.9f))
                    .navigationBarsPadding(),
            ) {
                // Animation controls
                if (state.isAnimation && state.frameCount > 1) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(horizontal = 16.dp, vertical = 4.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        // Play/pause
                        IconButton(
                            onClick = { viewModel.togglePlayPause() },
                            modifier = Modifier.size(36.dp),
                        ) {
                            Text(
                                if (state.isPlaying) "\u23F8" else "\u25B6",
                                color = Accent,
                                fontSize = 16.sp,
                            )
                        }

                        // Frame counter
                        Text(
                            "${state.currentFrame + 1}/${state.frameCount}",
                            color = TextSecondary,
                            fontSize = 12.sp,
                            modifier = Modifier.padding(horizontal = 8.dp),
                        )

                        // Seek slider
                        Slider(
                            value = state.currentFrame.toFloat(),
                            onValueChange = { viewModel.seekFrame(it.toInt()) },
                            valueRange = 0f..(state.frameCount - 1).toFloat().coerceAtLeast(0f),
                            modifier = Modifier.weight(1f),
                            colors = SliderDefaults.colors(
                                thumbColor = Accent,
                                activeTrackColor = Accent,
                                inactiveTrackColor = BgSurface,
                            ),
                        )
                    }
                }

                // Status row
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 16.dp, vertical = 6.dp),
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Text("${state.width}x${state.height}", color = TextMuted, fontSize = 12.sp)
                    Text("${state.decodeTimeMs}ms", color = TextMuted, fontSize = 12.sp)
                    if (state.isAnimation) {
                        Text("${state.frameCount} frames", color = TextMuted, fontSize = 12.sp)
                    }
                    Spacer(Modifier.weight(1f))
                    Text("jxl-rs", color = Accent, fontSize = 12.sp)
                }
            }
        }
    }
}

@Composable
fun SampleGallery(
    samples: List<String>,
    onSelect: (String) -> Unit,
    onOpenFile: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.padding(horizontal = 16.dp)) {
        Text(
            "JXL Viewer",
            color = TextPrimary,
            fontSize = 24.sp,
            fontWeight = FontWeight.Bold,
        )
        Text(
            "Powered by jxl-rs",
            color = TextMuted,
            fontSize = 13.sp,
        )
        Spacer(Modifier.height(16.dp))

        // Open from device button
        Button(
            onClick = onOpenFile,
            colors = ButtonDefaults.buttonColors(containerColor = Accent),
            shape = RoundedCornerShape(12.dp),
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("Open from Device", fontSize = 15.sp)
        }

        Spacer(Modifier.height(20.dp))

        Text(
            "SAMPLE IMAGES (${samples.size})",
            color = TextMuted,
            fontSize = 10.sp,
            fontWeight = FontWeight.Bold,
        )
        Spacer(Modifier.height(8.dp))

        LazyVerticalGrid(
            columns = GridCells.Fixed(2),
            verticalArrangement = Arrangement.spacedBy(8.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            items(samples) { name ->
                Surface(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable { onSelect(name) },
                    color = BgSurface,
                    shape = RoundedCornerShape(8.dp),
                ) {
                    Column(modifier = Modifier.padding(12.dp)) {
                        Text(
                            "JXL",
                            color = Accent,
                            fontSize = 11.sp,
                            fontWeight = FontWeight.Bold,
                        )
                        Spacer(Modifier.height(4.dp))
                        Text(
                            name.removeSuffix(".jxl"),
                            color = TextPrimary,
                            fontSize = 13.sp,
                            maxLines = 2,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
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
