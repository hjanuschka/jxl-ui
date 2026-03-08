package com.jxlui

import android.app.Application
import android.graphics.Bitmap
import android.net.Uri
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import java.util.concurrent.atomic.AtomicLong

data class ImageState(
    val bitmap: Bitmap? = null,
    val fileName: String? = null,
    val width: Int = 0,
    val height: Int = 0,
    val decodeTimeMs: Long = 0,
    val fileSizeBytes: Long = 0,
    val isLoading: Boolean = false,
    val error: String? = null,
    // Animation
    val isAnimation: Boolean = false,
    val frames: List<Pair<Bitmap, Int>>? = null,
    val frameCount: Int = 0,
    val currentFrame: Int = 0,
    val isPlaying: Boolean = false,
    // Progressive
    val progressPct: Int = 0,
    val completedPasses: Int = 0,
    val isProgressive: Boolean = false,
)

/** Output color format -- mirrors desktop OutputColorType */
enum class OutputColorType(val value: Int, val label: String) {
    Auto(0, "Auto"),
    Rgb(1, "RGB"),
    Rgba(2, "RGBA"),
    Bgr(3, "BGR"),
    Bgra(4, "BGRA"),
    Grayscale(5, "Grayscale"),
    GrayscaleAlpha(6, "Grayscale + Alpha"),
}

/** Output data format -- mirrors desktop OutputDataType */
enum class OutputDataType(val value: Int, val label: String) {
    F32(0, "Float32 (f32)"),
    U8(1, "Unsigned 8-bit (u8)"),
    U16(2, "Unsigned 16-bit (u16)"),
    F16(3, "Float16 (f16)"),
}

data class DecoderSettings(
    // Output format
    val colorType: Int = OutputColorType.Auto.value,
    val dataType: Int = OutputDataType.F32.value,
    // Options
    val premultiplyAlpha: Boolean = true,
    val linearOutput: Boolean = false,
    val highPrecision: Boolean = false,
    // Progressive demo
    val simulateSlow: Boolean = false,
    val slowChunkPct: Float = 1.0f,
    val slowDelayMs: Long = 50,
)

class JxlViewModel(application: Application) : AndroidViewModel(application) {
    companion object {
        private const val TAG = "JxlDecode"
    }

    private val _state = MutableStateFlow(ImageState())
    val state: StateFlow<ImageState> = _state

    val settings = MutableStateFlow(DecoderSettings())

    private var animationJob: Job? = null
    private var decodeJob: Job? = null
    private val decodeGeneration = AtomicLong(0)

    private var lastLoadedData: ByteArray? = null
    private var lastLoadedName: String? = null
    private var reusableProgressiveBitmap: Bitmap? = null

    val sampleFiles: List<String> by lazy {
        try {
            application.assets.list("samples")
                ?.filter { it.endsWith(".jxl") }
                ?.sorted()
                ?: emptyList()
        } catch (e: Exception) {
            emptyList()
        }
    }

    private fun startNewDecode(fileName: String?): Long {
        decodeJob?.cancel()
        val generation = decodeGeneration.incrementAndGet()
        stopAnimation()
        reusableProgressiveBitmap = null
        _state.value = ImageState(isLoading = true, fileName = fileName)
        return generation
    }

    private fun isCurrentDecode(generation: Long): Boolean {
        return decodeGeneration.get() == generation
    }

    fun loadFromUri(uri: Uri, fileName: String? = null) {
        val generation = startNewDecode(fileName)
        decodeJob = viewModelScope.launch(Dispatchers.IO) {
            try {
                val context = getApplication<Application>()
                val inputStream = context.contentResolver.openInputStream(uri)
                    ?: throw Exception("Cannot open file")
                val data = inputStream.readBytes()
                inputStream.close()
                if (!isCurrentDecode(generation)) return@launch

                Log.i(TAG, "loadFromUri: name=${fileName ?: "image.jxl"}, bytes=${data.size}")
                decodeAndEmit(data, fileName ?: "image.jxl", generation)
            } catch (_: CancellationException) {
                Log.i(TAG, "loadFromUri cancelled")
            } catch (e: Exception) {
                if (!isCurrentDecode(generation)) return@launch
                Log.e(TAG, "loadFromUri failed", e)
                _state.value = ImageState(error = e.message ?: "Unknown error")
            }
        }
    }

    fun loadSample(name: String) {
        val generation = startNewDecode(name)
        decodeJob = viewModelScope.launch(Dispatchers.IO) {
            try {
                val context = getApplication<Application>()
                val data = context.assets.open("samples/$name").readBytes()
                if (!isCurrentDecode(generation)) return@launch

                Log.i(TAG, "loadSample: name=$name, bytes=${data.size}")
                decodeAndEmit(data, name, generation)
            } catch (_: CancellationException) {
                Log.i(TAG, "loadSample cancelled: $name")
            } catch (e: Exception) {
                if (!isCurrentDecode(generation)) return@launch
                Log.e(TAG, "loadSample failed for $name", e)
                _state.value = ImageState(error = e.message ?: "Unknown error")
            }
        }
    }

    /** Reload current image with current settings. */
    fun reload() {
        val data = lastLoadedData ?: return
        val name = lastLoadedName ?: return
        val generation = startNewDecode(name)
        decodeJob = viewModelScope.launch(Dispatchers.IO) {
            try {
                decodeAndEmit(data, name, generation)
            } catch (_: CancellationException) {
                Log.i(TAG, "reload cancelled: $name")
            } catch (e: Exception) {
                if (!isCurrentDecode(generation)) return@launch
                _state.value = ImageState(error = e.message ?: "Unknown error")
            }
        }
    }

    fun clearImage() {
        decodeJob?.cancel()
        decodeJob = null
        decodeGeneration.incrementAndGet()
        stopAnimation()
        reusableProgressiveBitmap = null
        _state.value = ImageState()
        Log.i(TAG, "clearImage: cancelled active decode")
    }

    fun togglePlayPause() {
        val s = _state.value
        if (!s.isAnimation || s.frames == null) return
        if (s.isPlaying) stopAnimation() else startAnimation()
    }

    fun seekFrame(index: Int) {
        val s = _state.value
        val frames = s.frames ?: return
        if (index < 0 || index >= frames.size) return
        _state.value = s.copy(currentFrame = index, bitmap = frames[index].first)
    }

    private fun startAnimation() {
        val frames = _state.value.frames ?: return
        if (frames.size < 2) return
        _state.value = _state.value.copy(isPlaying = true)

        animationJob = viewModelScope.launch(Dispatchers.Main) {
            while (isActive) {
                val s = _state.value
                val nextFrame = (s.currentFrame + 1) % frames.size
                delay(frames[s.currentFrame].second.toLong().coerceAtLeast(16))
                _state.value = _state.value.copy(
                    currentFrame = nextFrame,
                    bitmap = frames[nextFrame].first,
                )
            }
        }
    }

    private fun stopAnimation() {
        animationJob?.cancel()
        animationJob = null
        _state.value = _state.value.copy(isPlaying = false)
    }

    private fun decodeAndEmit(data: ByteArray, name: String, generation: Long) {
        if (!isCurrentDecode(generation)) return

        lastLoadedData = data
        lastLoadedName = name

        val startTime = System.nanoTime()
        val s = settings.value
        reusableProgressiveBitmap = null

        // Check if animation
        val isAnim = JxlDecoder.isAnimation(data)

        if (isAnim) {
            Log.i(TAG, "decode start: animation name=$name")
            val frames = JxlDecoder.decodeAnimation(data)
                ?: throw Exception("Failed to decode JXL animation")
            val elapsed = (System.nanoTime() - startTime) / 1_000_000
            if (frames.isEmpty()) throw Exception("Animation has no frames")

            if (!isCurrentDecode(generation)) return
            _state.value = ImageState(
                bitmap = frames[0].first,
                fileName = name,
                width = frames[0].first.width,
                height = frames[0].first.height,
                decodeTimeMs = elapsed,
                fileSizeBytes = data.size.toLong(),
                isAnimation = true,
                frames = frames,
                frameCount = frames.size,
                currentFrame = 0,
                isPlaying = false,
            )
            Log.i(TAG, "decode done: animation frames=${frames.size}, elapsed=${elapsed}ms")
            startAnimation()
        } else {
            // Always progressive for non-animation images.
            // simulateSlow=false => no artificial delay, larger flush interval (fast progressive)
            // simulateSlow=true  => user-controlled chunk/delay (demo mode)
            val decodeSettings = if (s.simulateSlow) {
                s
            } else {
                s.copy(simulateSlow = false, slowChunkPct = 1.0f, slowDelayMs = 0)
            }

            Log.i(
                TAG,
                "decode start: progressive name=$name simulateSlow=${decodeSettings.simulateSlow} chunkPct=${decodeSettings.slowChunkPct} delayMs=${decodeSettings.slowDelayMs}"
            )

            if (!isCurrentDecode(generation)) return
            _state.value = ImageState(
                isLoading = false,
                fileName = name,
                isProgressive = true,
                progressPct = 0,
            )

            var lastLoggedPct = -10
            JxlDecoder.decodeProgressive(data, decodeSettings) { pixels, w, h, passes, pct ->
                if (!isCurrentDecode(generation)) return@decodeProgressive
                val current = _state.value
                val bmp = if (pixels.isNotEmpty()) {
                    JxlDecoder.pixelsToBitmapPublic(pixels, w, h, reusableProgressiveBitmap)
                } else {
                    current.bitmap
                }

                if (bmp != null) {
                    reusableProgressiveBitmap = bmp
                }

                _state.value = current.copy(
                    bitmap = bmp ?: current.bitmap,
                    width = w,
                    height = h,
                    completedPasses = passes,
                    progressPct = pct,
                    isLoading = false,
                    isProgressive = pct < 100,
                    fileName = name,
                    fileSizeBytes = data.size.toLong(),
                )

                if (pct >= lastLoggedPct + 10 || pct == 100) {
                    Log.i(TAG, "progress: name=$name pct=$pct passes=$passes size=${w}x${h} pixels=${pixels.size}")
                    lastLoggedPct = pct
                }
            }

            val elapsed = (System.nanoTime() - startTime) / 1_000_000
            _state.value = _state.value.copy(
                decodeTimeMs = elapsed,
                isProgressive = false,
                progressPct = 100,
            )
            Log.i(TAG, "decode done: progressive elapsed=${elapsed}ms")
        }
    }
}
