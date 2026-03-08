package com.jxlui

import android.app.Application
import android.graphics.Bitmap
import android.net.Uri
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
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
)

class JxlViewModel(application: Application) : AndroidViewModel(application) {
    private val _state = MutableStateFlow(ImageState())
    val state: StateFlow<ImageState> = _state

    fun loadFromUri(uri: Uri, fileName: String? = null) {
        viewModelScope.launch(Dispatchers.IO) {
            _state.value = ImageState(isLoading = true, fileName = fileName)

            try {
                val context = getApplication<Application>()
                val inputStream = context.contentResolver.openInputStream(uri)
                    ?: throw Exception("Cannot open file")
                val data = inputStream.readBytes()
                inputStream.close()

                val startTime = System.nanoTime()
                val bitmap = JxlDecoder.decode(data)
                    ?: throw Exception("Failed to decode JXL image")
                val elapsed = (System.nanoTime() - startTime) / 1_000_000

                _state.value = ImageState(
                    bitmap = bitmap,
                    fileName = fileName ?: "image.jxl",
                    width = bitmap.width,
                    height = bitmap.height,
                    decodeTimeMs = elapsed,
                    fileSizeBytes = data.size.toLong(),
                )
            } catch (e: Exception) {
                _state.value = ImageState(error = e.message ?: "Unknown error")
            }
        }
    }
}
