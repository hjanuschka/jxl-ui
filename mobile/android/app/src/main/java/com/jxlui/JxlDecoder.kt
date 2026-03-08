package com.jxlui

import android.graphics.Bitmap
import java.util.ArrayList

object JxlDecoder {
    init {
        System.loadLibrary("jxl_mobile_core")
    }

    /** Decode raw JXL bytes to a DecodedImage (called via JNI). */
    private external fun nativeDecode(data: ByteArray): DecodedImage?

    /** Check if data is an animation. */
    private external fun nativeIsAnimation(data: ByteArray): Byte

    /** Decode animation frames. Returns ArrayList<AnimFrame>. */
    private external fun nativeDecodeAnimation(data: ByteArray): ArrayList<AnimFrame>?

    /** Decode JXL bytes into an Android Bitmap. Returns null on error. */
    fun decode(data: ByteArray): Bitmap? {
        val decoded = nativeDecode(data) ?: return null
        return pixelsToBitmap(decoded.pixels, decoded.width, decoded.height)
    }

    /** Check if JXL data contains an animation. */
    fun isAnimation(data: ByteArray): Boolean {
        return nativeIsAnimation(data) != 0.toByte()
    }

    /** Decode animation. Returns list of (Bitmap, durationMs) pairs, or null. */
    fun decodeAnimation(data: ByteArray): List<Pair<Bitmap, Int>>? {
        val frames = nativeDecodeAnimation(data) ?: return null
        return frames.map { frame ->
            val bmp = pixelsToBitmap(frame.pixels, frame.width, frame.height)
                ?: return null
            Pair(bmp, frame.durationMs)
        }
    }

    /** Convert RGBA8 byte array to Android Bitmap. */
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
