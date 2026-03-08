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
        val expected = width * height * 4
        if (pixels.size < expected) return null
        // RGBA -> ARGB_8888 in-place swizzle (ARGB_8888 is actually RGBA in little-endian)
        // Android Bitmap.Config.ARGB_8888 stores pixels as RGBA in ByteBuffer on little-endian
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        val buffer = java.nio.ByteBuffer.wrap(pixels, 0, expected)
        bitmap.copyPixelsFromBuffer(buffer)
        return bitmap
    }
}
