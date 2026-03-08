package com.jxlui

import android.graphics.Bitmap
import java.util.ArrayList

object JxlDecoder {
    init {
        System.loadLibrary("jxl_mobile_core")
    }

    private external fun nativeDecode(data: ByteArray): DecodedImage?
    private external fun nativeDecodeWithSettings(
        data: ByteArray,
        colorType: Int,
        dataType: Int,
        premultiplyAlpha: Byte,
        linearOutput: Byte,
        highPrecision: Byte,
    ): DecodedImage?
    private external fun nativeIsAnimation(data: ByteArray): Byte
    private external fun nativeDecodeAnimation(data: ByteArray): ArrayList<AnimFrame>?
    private external fun nativeDecodeProgressive(
        data: ByteArray,
        colorType: Int,
        dataType: Int,
        premultiplyAlpha: Byte,
        linearOutput: Byte,
        highPrecision: Byte,
        slowChunkPct: Float,
        slowDelayMs: Long,
        listener: ProgressListener,
    ): DecodedImage?

    /** Callback interface for progressive decode updates. */
    interface ProgressListener {
        fun onProgress(pixels: ByteArray, width: Int, height: Int, passes: Int, progressPct: Int)
    }

    fun decode(data: ByteArray): Bitmap? {
        val decoded = nativeDecode(data) ?: return null
        return pixelsToBitmap(decoded.pixels, decoded.width, decoded.height)
    }

    fun decodeWithSettings(data: ByteArray, settings: DecoderSettings): Bitmap? {
        val decoded = nativeDecodeWithSettings(
            data,
            settings.colorType,
            settings.dataType,
            if (settings.premultiplyAlpha) 1 else 0,
            if (settings.linearOutput) 1 else 0,
            if (settings.highPrecision) 1 else 0,
        ) ?: return null
        return pixelsToBitmap(decoded.pixels, decoded.width, decoded.height)
    }

    fun isAnimation(data: ByteArray): Boolean {
        return nativeIsAnimation(data) != 0.toByte()
    }

    fun decodeAnimation(data: ByteArray): List<Pair<Bitmap, Int>>? {
        val frames = nativeDecodeAnimation(data) ?: return null
        return frames.map { frame ->
            val bmp = pixelsToBitmap(frame.pixels, frame.width, frame.height) ?: return null
            Pair(bmp, frame.durationMs)
        }
    }

    /**
     * Progressive decode with callback for each partial update.
     */
    fun decodeProgressive(
        data: ByteArray,
        settings: DecoderSettings,
        onProgress: (pixels: ByteArray, width: Int, height: Int, passes: Int, pct: Int) -> Unit,
    ): Bitmap? {
        val decoded = nativeDecodeProgressive(
            data,
            settings.colorType,
            settings.dataType,
            if (settings.premultiplyAlpha) 1 else 0,
            if (settings.linearOutput) 1 else 0,
            if (settings.highPrecision) 1 else 0,
            settings.slowChunkPct,
            settings.slowDelayMs,
            object : ProgressListener {
                override fun onProgress(pixels: ByteArray, width: Int, height: Int, passes: Int, progressPct: Int) {
                    onProgress(pixels, width, height, passes, progressPct)
                }
            },
        ) ?: return null
        return pixelsToBitmap(decoded.pixels, decoded.width, decoded.height)
    }

    /** Public version for ViewModel use. */
    fun pixelsToBitmapPublic(pixels: ByteArray, width: Int, height: Int): Bitmap? {
        return pixelsToBitmap(pixels, width, height)
    }

    private fun pixelsToBitmap(pixels: ByteArray, width: Int, height: Int): Bitmap? {
        if (pixels.size < width * height * 4) return null
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        val intPixels = IntArray(width * height)
        for (i in intPixels.indices) {
            val offset = i * 4
            val r = pixels[offset].toInt() and 0xFF
            val g = pixels[offset + 1].toInt() and 0xFF
            val b = pixels[offset + 2].toInt() and 0xFF
            val a = pixels[offset + 3].toInt() and 0xFF
            intPixels[i] = (a shl 24) or (r shl 16) or (g shl 8) or b
        }
        bitmap.setPixels(intPixels, 0, width, 0, 0, width, height)
        return bitmap
    }
}
