package com.jxlui

import android.app.Application
import android.graphics.Bitmap
import android.net.Uri
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

data class ImageState(
    val bitmap: Bitmap? = null,
    val fileName: String? = null,
    val width: Int = 0,
    val height: Int = 0,
    val decodeTimeMs: Long = 0,
    val fileSizeBytes: Long = 0,
    val isLoading: Boolean = false,
    val error: String? = null,
    // Animation state
    val isAnimation: Boolean = false,
    val frames: List<Pair<Bitmap, Int>>? = null, // (bitmap, durationMs)
    val frameCount: Int = 0,
    val currentFrame: Int = 0,
    val isPlaying: Boolean = false,
)

class JxlViewModel(application: Application) : AndroidViewModel(application) {
    private val _state = MutableStateFlow(ImageState())
    val state: StateFlow<ImageState> = _state

    private var animationJob: Job? = null

    /** List of bundled sample JXL files from assets/samples/ */
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

    fun loadFromUri(uri: Uri, fileName: String? = null) {
        viewModelScope.launch(Dispatchers.IO) {
            _state.value = ImageState(isLoading = true, fileName = fileName)

            try {
                val context = getApplication<Application>()
                val inputStream = context.contentResolver.openInputStream(uri)
                    ?: throw Exception("Cannot open file")
                val data = inputStream.readBytes()
                inputStream.close()

                decodeAndEmit(data, fileName ?: "image.jxl")
            } catch (e: Exception) {
                _state.value = ImageState(error = e.message ?: "Unknown error")
            }
        }
    }

    fun loadSample(name: String) {
        viewModelScope.launch(Dispatchers.IO) {
            _state.value = ImageState(isLoading = true, fileName = name)

            try {
                val context = getApplication<Application>()
                val data = context.assets.open("samples/$name").readBytes()
                decodeAndEmit(data, name)
            } catch (e: Exception) {
                _state.value = ImageState(error = e.message ?: "Unknown error")
            }
        }
    }

    fun clearImage() {
        stopAnimation()
        _state.value = ImageState()
    }

    fun togglePlayPause() {
        val s = _state.value
        if (!s.isAnimation || s.frames == null) return

        if (s.isPlaying) {
            stopAnimation()
        } else {
            startAnimation()
        }
    }

    fun seekFrame(index: Int) {
        val s = _state.value
        val frames = s.frames ?: return
        if (index < 0 || index >= frames.size) return
        _state.value = s.copy(
            currentFrame = index,
            bitmap = frames[index].first,
        )
    }

    private fun startAnimation() {
        val frames = _state.value.frames ?: return
        if (frames.size < 2) return

        _state.value = _state.value.copy(isPlaying = true)

        animationJob = viewModelScope.launch(Dispatchers.Main) {
            while (isActive) {
                val s = _state.value
                val nextFrame = (s.currentFrame + 1) % frames.size
                val durationMs = frames[s.currentFrame].second.toLong()

                delay(durationMs.coerceAtLeast(16))

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

    private fun decodeAndEmit(data: ByteArray, name: String) {
        val startTime = System.nanoTime()

        // Check if animation
        val isAnim = JxlDecoder.isAnimation(data)

        if (isAnim) {
            val frames = JxlDecoder.decodeAnimation(data)
                ?: throw Exception("Failed to decode JXL animation")
            val elapsed = (System.nanoTime() - startTime) / 1_000_000

            if (frames.isEmpty()) throw Exception("Animation has no frames")

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

            // Auto-play animations
            startAnimation()
        } else {
            val bitmap = JxlDecoder.decode(data)
                ?: throw Exception("Failed to decode JXL image")
            val elapsed = (System.nanoTime() - startTime) / 1_000_000

            _state.value = ImageState(
                bitmap = bitmap,
                fileName = name,
                width = bitmap.width,
                height = bitmap.height,
                decodeTimeMs = elapsed,
                fileSizeBytes = data.size.toLong(),
            )
        }
    }
}
