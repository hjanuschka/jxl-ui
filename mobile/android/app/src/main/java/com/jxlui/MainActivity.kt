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
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.roundToInt

// Black matte theme -- matching januschka.com default
private val Bg = Color(0xFF121212)
private val BgLight = Color(0xFF1E1E1E)
private val BgLighter = Color(0xFF333333)
private val Text_ = Color(0xFFBEBEBE)
private val TextDim = Color(0xFF8A8A8D)
private val Accent = Color(0xFFFFC107)       // amber/gold
private val AccentDim = Color(0xFFE68E0D)
private val Border = Color(0xFF333333)
private val Orange = Color(0xFFD35F5F)

class MainActivity : ComponentActivity() {
    private val viewModel: JxlViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        intent?.data?.let { uri ->
            viewModel.loadFromUri(uri, getFileName(uri))
        }

        setContent {
            MaterialTheme(
                colorScheme = darkColorScheme(
                    primary = Accent,
                    surface = Bg,
                    background = Bg,
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
    val settings by viewModel.settings.collectAsState()
    var showInfo by remember { mutableStateOf(false) }
    var showSettings by remember { mutableStateOf(false) }

    val filePicker = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocument()
    ) { uri: Uri? ->
        uri?.let { viewModel.loadFromUri(it) }
    }

    Box(
        modifier = Modifier.fillMaxSize().background(Bg)
    ) {
        // Main content
        when {
            state.isLoading && !state.isProgressive -> {
                Column(
                    modifier = Modifier.align(Alignment.Center),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    CircularProgressIndicator(color = Accent, strokeWidth = 2.dp)
                    Spacer(Modifier.height(16.dp))
                    Text("Decoding...", color = TextDim, fontSize = 13.sp, fontFamily = FontFamily.Monospace)
                }
            }

            state.error != null -> {
                Column(
                    modifier = Modifier.align(Alignment.Center).padding(32.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Text("Error", color = Orange, fontSize = 18.sp, fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace)
                    Spacer(Modifier.height(8.dp))
                    Text(state.error ?: "", color = TextDim, fontSize = 12.sp, textAlign = TextAlign.Center, fontFamily = FontFamily.Monospace)
                    Spacer(Modifier.height(24.dp))
                    OutlinedButton(
                        onClick = { viewModel.clearImage() },
                        border = androidx.compose.foundation.BorderStroke(2.dp, Border),
                        shape = RoundedCornerShape(8.dp),
                    ) { Text("< back", color = Text_, fontFamily = FontFamily.Monospace) }
                }
            }

            state.bitmap != null -> {
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
                                onDoubleTap = { scale = 1f; offsetX = 0f; offsetY = 0f },
                                onTap = { showInfo = !showInfo }
                            )
                        }
                        .graphicsLayer(scaleX = scale, scaleY = scale, translationX = offsetX, translationY = offsetY)
                )

                // Progressive overlay
                if (state.isProgressive && state.progressPct < 100) {
                    Box(
                        modifier = Modifier
                            .align(Alignment.TopCenter)
                            .padding(top = 110.dp)
                            .border(2.dp, Accent.copy(alpha = 0.5f), RoundedCornerShape(8.dp))
                            .background(Bg.copy(alpha = 0.92f), RoundedCornerShape(8.dp))
                            .padding(horizontal = 20.dp, vertical = 12.dp)
                    ) {
                        Column(horizontalAlignment = Alignment.CenterHorizontally) {
                            Text(
                                "progressive decode",
                                color = Accent, fontSize = 11.sp, fontFamily = FontFamily.Monospace,
                            )
                            Spacer(Modifier.height(4.dp))
                            Text(
                                "${state.progressPct}%  pass ${state.completedPasses}",
                                color = Text_, fontSize = 12.sp, fontFamily = FontFamily.Monospace,
                            )
                            Spacer(Modifier.height(6.dp))
                            LinearProgressIndicator(
                                progress = { state.progressPct / 100f },
                                modifier = Modifier.width(180.dp).height(2.dp),
                                color = Accent,
                                trackColor = BgLighter,
                            )
                        }
                    }
                }
            }

            else -> {
                SampleGallery(
                    samples = viewModel.sampleFiles,
                    onSelect = { viewModel.loadSample(it) },
                    onOpenFile = { filePicker.launch(arrayOf("*/*")) },
                    modifier = Modifier
                        .fillMaxSize()
                        .statusBarsPadding()
                        .padding(top = 52.dp)
                        .navigationBarsPadding()
                )
            }
        }

        // Top bar
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .background(BgLight.copy(alpha = 0.97f))
                .statusBarsPadding()
                .padding(horizontal = 16.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (state.bitmap != null) {
                Box(
                    modifier = Modifier
                        .size(32.dp)
                        .clip(RoundedCornerShape(6.dp))
                        .border(1.dp, Border, RoundedCornerShape(6.dp))
                        .background(Bg)
                        .clickable { viewModel.clearImage(); showInfo = false; showSettings = false },
                    contentAlignment = Alignment.Center,
                ) { Text("<", color = Accent, fontSize = 14.sp, fontFamily = FontFamily.Monospace) }
                Spacer(Modifier.width(12.dp))
            }

            Text(
                text = state.fileName ?: "jxl-viewer",
                color = if (state.bitmap != null) Text_ else Accent,
                fontSize = 14.sp,
                fontWeight = FontWeight.Bold,
                fontFamily = FontFamily.Monospace,
                maxLines = 1, overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )

            // Settings
            Box(
                modifier = Modifier
                    .size(32.dp)
                    .clip(RoundedCornerShape(6.dp))
                    .border(
                        width = if (showSettings) 2.dp else 1.dp,
                        color = if (showSettings) Accent else Border,
                        shape = RoundedCornerShape(6.dp),
                    )
                    .background(if (showSettings) Accent.copy(alpha = 0.1f) else Bg)
                    .clickable { showSettings = !showSettings },
                contentAlignment = Alignment.Center,
            ) { Text("\u2699", color = if (showSettings) Accent else TextDim, fontSize = 14.sp) }

            Spacer(Modifier.width(8.dp))

            // Open file
            Box(
                modifier = Modifier
                    .size(32.dp)
                    .clip(RoundedCornerShape(6.dp))
                    .border(1.dp, Border, RoundedCornerShape(6.dp))
                    .background(Bg)
                    .clickable { filePicker.launch(arrayOf("*/*")) },
                contentAlignment = Alignment.Center,
            ) { Text("+", color = TextDim, fontSize = 16.sp, fontFamily = FontFamily.Monospace) }
        }

        // Info overlay
        AnimatedVisibility(
            visible = showInfo && state.bitmap != null,
            enter = fadeIn(), exit = fadeOut(),
            modifier = Modifier
                .align(Alignment.BottomStart)
                .padding(12.dp)
                .padding(bottom = if (state.isAnimation && state.frameCount > 1) 100.dp else 44.dp)
                .navigationBarsPadding(),
        ) {
            Box(
                modifier = Modifier
                    .border(2.dp, Border, RoundedCornerShape(8.dp))
                    .background(Bg.copy(alpha = 0.95f), RoundedCornerShape(8.dp))
                    .padding(16.dp)
            ) {
                Column {
                    Text("// image info", color = Accent, fontSize = 11.sp, fontFamily = FontFamily.Monospace)
                    Spacer(Modifier.height(8.dp))
                    InfoRow("size", "${state.width} x ${state.height}")
                    InfoRow("mpx", "%.2f".format(state.width.toLong() * state.height / 1_000_000.0))
                    InfoRow("decode", "${state.decodeTimeMs} ms")
                    if (state.fileSizeBytes > 0) InfoRow("file", "%.1f KB".format(state.fileSizeBytes / 1024.0))
                    if (state.decodeTimeMs > 0) {
                        val mpps = (state.width.toLong() * state.height) / (state.decodeTimeMs / 1000.0) / 1_000_000.0
                        InfoRow("speed", "%.1f MP/s".format(mpps))
                    }
                    if (state.isAnimation) InfoRow("frames", "${state.frameCount}")
                    val ct = OutputColorType.entries.find { it.value == settings.colorType } ?: OutputColorType.Auto
                    val dt = OutputDataType.entries.find { it.value == settings.dataType } ?: OutputDataType.F32
                    InfoRow("color", ct.label)
                    InfoRow("data", dt.label)
                }
            }
        }

        // Bottom bar
        if (state.bitmap != null) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .align(Alignment.BottomCenter)
                    .background(BgLight.copy(alpha = 0.95f))
                    .navigationBarsPadding(),
            ) {
                // Thin accent line at top
                Box(Modifier.fillMaxWidth().height(1.dp).background(Accent.copy(alpha = 0.3f)))

                // Animation controls
                if (state.isAnimation && state.frameCount > 1) {
                    Row(
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Box(
                            modifier = Modifier
                                .size(28.dp)
                                .clip(RoundedCornerShape(4.dp))
                                .border(1.dp, Border, RoundedCornerShape(4.dp))
                                .background(Bg)
                                .clickable { viewModel.togglePlayPause() },
                            contentAlignment = Alignment.Center,
                        ) {
                            Text(
                                if (state.isPlaying) "\u23F8" else "\u25B6",
                                color = Accent, fontSize = 12.sp,
                            )
                        }
                        Text(
                            "${state.currentFrame + 1}/${state.frameCount}",
                            color = TextDim, fontSize = 11.sp, fontFamily = FontFamily.Monospace,
                            modifier = Modifier.padding(horizontal = 8.dp),
                        )
                        Slider(
                            value = state.currentFrame.toFloat(),
                            onValueChange = { viewModel.seekFrame(it.toInt()) },
                            valueRange = 0f..(state.frameCount - 1).toFloat().coerceAtLeast(0f),
                            modifier = Modifier.weight(1f),
                            colors = SliderDefaults.colors(
                                thumbColor = Accent,
                                activeTrackColor = Accent,
                                inactiveTrackColor = BgLighter,
                            ),
                        )
                    }
                }

                // Status row
                Row(
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp),
                    horizontalArrangement = Arrangement.spacedBy(16.dp),
                ) {
                    Text("${state.width}x${state.height}", color = TextDim, fontSize = 11.sp, fontFamily = FontFamily.Monospace)
                    Text(
                        if (state.decodeTimeMs > 0) "${state.decodeTimeMs}ms" else "...",
                        color = TextDim, fontSize = 11.sp, fontFamily = FontFamily.Monospace,
                    )
                    if (state.isAnimation) Text("${state.frameCount}f", color = TextDim, fontSize = 11.sp, fontFamily = FontFamily.Monospace)
                    if (settings.simulateSlow) Text("progressive", color = Accent.copy(alpha = 0.7f), fontSize = 11.sp, fontFamily = FontFamily.Monospace)
                    Spacer(Modifier.weight(1f))
                    Text("jxl-rs", color = Accent, fontSize = 11.sp, fontFamily = FontFamily.Monospace)
                }
            }
        }

        // Settings panel
        AnimatedVisibility(
            visible = showSettings,
            enter = fadeIn(), exit = fadeOut(),
            modifier = Modifier
                .align(Alignment.TopEnd)
                .statusBarsPadding()
                .padding(top = 56.dp, end = 8.dp, bottom = 80.dp)
                .widthIn(max = 280.dp),
        ) {
            Box(
                modifier = Modifier
                    .border(2.dp, Border, RoundedCornerShape(12.dp))
                    .background(BgLight, RoundedCornerShape(12.dp))
            ) {
                SettingsPanel(
                    settings = settings,
                    hasImage = state.bitmap != null && !state.isAnimation,
                    onSettingsChanged = { newSettings ->
                        viewModel.settings.value = newSettings
                        if (state.bitmap != null && !state.isAnimation) {
                            val formatChanged = newSettings.colorType != settings.colorType
                                || newSettings.dataType != settings.dataType
                                || newSettings.premultiplyAlpha != settings.premultiplyAlpha
                                || newSettings.linearOutput != settings.linearOutput
                                || newSettings.highPrecision != settings.highPrecision
                            if (formatChanged) viewModel.reload()
                        }
                    },
                    onReload = { showSettings = false; viewModel.reload() },
                    onClose = { showSettings = false },
                )
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Settings Panel
// ---------------------------------------------------------------------------

@Composable
fun SettingsPanel(
    settings: DecoderSettings,
    hasImage: Boolean,
    onSettingsChanged: (DecoderSettings) -> Unit,
    onReload: () -> Unit,
    onClose: () -> Unit,
) {
    Column(
        modifier = Modifier.padding(16.dp).verticalScroll(rememberScrollState())
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("// settings", color = Accent, fontSize = 13.sp, fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace)
            Box(
                modifier = Modifier
                    .size(24.dp)
                    .clip(RoundedCornerShape(4.dp))
                    .border(1.dp, Border, RoundedCornerShape(4.dp))
                    .clickable(onClick = onClose),
                contentAlignment = Alignment.Center,
            ) { Text("\u2715", color = TextDim, fontSize = 11.sp) }
        }

        SettingsDivider()

        SectionLabel("color format")
        Spacer(Modifier.height(4.dp))
        OutputColorType.entries.forEach { ct ->
            SettingsRadioRow(
                label = ct.label,
                selected = settings.colorType == ct.value,
                onClick = { onSettingsChanged(settings.copy(colorType = ct.value)) },
            )
        }

        SettingsDivider()

        SectionLabel("data format")
        Spacer(Modifier.height(4.dp))
        OutputDataType.entries.forEach { dt ->
            SettingsRadioRow(
                label = dt.label,
                selected = settings.dataType == dt.value,
                onClick = { onSettingsChanged(settings.copy(dataType = dt.value)) },
            )
        }

        SettingsDivider()

        SectionLabel("options")
        Spacer(Modifier.height(8.dp))
        SettingsCheckRow("premultiply alpha", settings.premultiplyAlpha) { onSettingsChanged(settings.copy(premultiplyAlpha = it)) }
        SettingsCheckRow("linear output (xyb)", settings.linearOutput) { onSettingsChanged(settings.copy(linearOutput = it)) }
        SettingsCheckRow("high precision", settings.highPrecision) { onSettingsChanged(settings.copy(highPrecision = it)) }

        SettingsDivider()

        SectionLabel("progressive demo")
        Spacer(Modifier.height(8.dp))
        SettingsCheckRow("slow loading", settings.simulateSlow) { onSettingsChanged(settings.copy(simulateSlow = it)) }

        if (settings.simulateSlow) {
            Spacer(Modifier.height(12.dp))

            Text("chunk %:", color = TextDim, fontSize = 10.sp, fontFamily = FontFamily.Monospace, modifier = Modifier.padding(start = 28.dp))
            Spacer(Modifier.height(2.dp))
            Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(start = 28.dp)) {
                Slider(
                    value = settings.slowChunkPct,
                    onValueChange = { onSettingsChanged(settings.copy(slowChunkPct = (it * 10).roundToInt() / 10f)) },
                    valueRange = 0.1f..10f,
                    modifier = Modifier.weight(1f),
                    colors = SliderDefaults.colors(thumbColor = Accent, activeTrackColor = Accent, inactiveTrackColor = BgLighter),
                )
                Spacer(Modifier.width(8.dp))
                Text("%.1f".format(settings.slowChunkPct), color = Accent, fontSize = 10.sp, fontFamily = FontFamily.Monospace, modifier = Modifier.width(30.dp))
            }

            Spacer(Modifier.height(4.dp))
            Text("delay ms:", color = TextDim, fontSize = 10.sp, fontFamily = FontFamily.Monospace, modifier = Modifier.padding(start = 28.dp))
            Spacer(Modifier.height(2.dp))
            Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(start = 28.dp)) {
                Slider(
                    value = settings.slowDelayMs.toFloat(),
                    onValueChange = { onSettingsChanged(settings.copy(slowDelayMs = it.toLong())) },
                    valueRange = 1f..500f,
                    modifier = Modifier.weight(1f),
                    colors = SliderDefaults.colors(thumbColor = Accent, activeTrackColor = Accent, inactiveTrackColor = BgLighter),
                )
                Spacer(Modifier.width(8.dp))
                Text("${settings.slowDelayMs}", color = Accent, fontSize = 10.sp, fontFamily = FontFamily.Monospace, modifier = Modifier.width(30.dp))
            }

            val pctPerSec = (1000.0 / settings.slowDelayMs) * settings.slowChunkPct
            Text(
                "~%.0f%%/s  %.1fs total".format(pctPerSec, if (pctPerSec > 0) 100.0 / pctPerSec else 0.0),
                color = TextDim, fontSize = 9.sp, fontFamily = FontFamily.Monospace,
                modifier = Modifier.padding(start = 28.dp),
            )
        }

        Spacer(Modifier.height(20.dp))

        if (hasImage) {
            OutlinedButton(
                onClick = onReload,
                border = androidx.compose.foundation.BorderStroke(2.dp, Accent),
                shape = RoundedCornerShape(8.dp),
                modifier = Modifier.fillMaxWidth(),
            ) { Text("apply & reload", color = Accent, fontSize = 12.sp, fontFamily = FontFamily.Monospace) }
        } else {
            Text(
                "load an image first",
                color = TextDim, fontSize = 10.sp, fontFamily = FontFamily.Monospace,
                textAlign = TextAlign.Center,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Setting row components
// ---------------------------------------------------------------------------

@Composable
fun SettingsRadioRow(label: String, selected: Boolean, onClick: () -> Unit) {
    val textColor = if (selected) Accent else TextDim
    val bgColor = if (selected) Accent.copy(alpha = 0.08f) else Color.Transparent
    val borderColor = if (selected) Accent.copy(alpha = 0.3f) else Color.Transparent
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 1.dp)
            .border(1.dp, borderColor, RoundedCornerShape(6.dp))
            .background(bgColor, RoundedCornerShape(6.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 10.dp, vertical = 6.dp),
    ) {
        Text(
            text = if (selected) "> $label" else "  $label",
            color = textColor, fontSize = 11.sp, fontFamily = FontFamily.Monospace,
        )
    }
}

@Composable
fun SettingsCheckRow(label: String, checked: Boolean, onCheckedChange: (Boolean) -> Unit) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .padding(vertical = 2.dp)
            .clickable { onCheckedChange(!checked) },
    ) {
        Checkbox(
            checked = checked,
            onCheckedChange = onCheckedChange,
            colors = CheckboxDefaults.colors(
                checkedColor = Accent,
                uncheckedColor = BgLighter,
                checkmarkColor = Bg,
            ),
            modifier = Modifier.size(18.dp),
        )
        Spacer(Modifier.width(8.dp))
        Text(label, color = if (checked) Text_ else TextDim, fontSize = 11.sp, fontFamily = FontFamily.Monospace)
    }
}

@Composable
fun SettingsDivider() {
    Spacer(Modifier.height(12.dp))
    Box(Modifier.fillMaxWidth().height(1.dp).background(Border))
    Spacer(Modifier.height(12.dp))
}

// ---------------------------------------------------------------------------
// Gallery
// ---------------------------------------------------------------------------

@Composable
fun SampleGallery(
    samples: List<String>,
    onSelect: (String) -> Unit,
    onOpenFile: () -> Unit,
    modifier: Modifier = Modifier,
) {
    LazyVerticalGrid(
        columns = GridCells.Fixed(2),
        modifier = modifier.padding(horizontal = 16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        contentPadding = PaddingValues(bottom = 24.dp),
    ) {
        // Header
        item(span = { GridItemSpan(2) }) {
            Column {
                Text("jxl-viewer", color = Accent, fontSize = 22.sp, fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace)
                Text("powered by jxl-rs", color = TextDim, fontSize = 12.sp, fontFamily = FontFamily.Monospace)
                Spacer(Modifier.height(16.dp))
                OutlinedButton(
                    onClick = onOpenFile,
                    border = androidx.compose.foundation.BorderStroke(2.dp, Accent),
                    shape = RoundedCornerShape(8.dp),
                    modifier = Modifier.fillMaxWidth().height(48.dp),
                ) { Text("open from device", color = Accent, fontSize = 13.sp, fontFamily = FontFamily.Monospace) }
                Spacer(Modifier.height(20.dp))
                Text("samples (${samples.size})", color = TextDim, fontSize = 10.sp, fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace)
                Spacer(Modifier.height(8.dp))
            }
        }

        items(samples) { name ->
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .border(1.dp, Border, RoundedCornerShape(8.dp))
                    .background(
                        Brush.linearGradient(listOf(BgLight, BgLighter.copy(alpha = 0.3f))),
                        RoundedCornerShape(8.dp),
                    )
                    .clickable { onSelect(name) }
                    .padding(14.dp),
            ) {
                Text(
                    name.removeSuffix(".jxl"),
                    color = Text_,
                    fontSize = 12.sp,
                    fontFamily = FontFamily.Monospace,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

@Composable
fun SectionLabel(text: String) {
    Text(text, color = TextDim, fontSize = 9.sp, fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace,
        letterSpacing = 1.sp)
}

@Composable
fun InfoRow(label: String, value: String) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(vertical = 1.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(label, color = TextDim, fontSize = 11.sp, fontFamily = FontFamily.Monospace)
        Text(value, color = Text_, fontSize = 11.sp, fontFamily = FontFamily.Monospace)
    }
}
