package com.jxlui

import android.graphics.Bitmap

object JxlDecoder {
    init {
        System.loadLibrary("jxl_mobile_core")
    }

    /** Decode raw JXL bytes to a DecodedImage (called via JNI). */
    private external fun nativeDecode(data: ByteArray): DecodedImage?

    /** Decode JXL bytes into an Android Bitmap. */
    fun decode(data: ByteArray): Bitmap? {
        val decoded = nativeDecode(data) ?: return null

        val bitmap = Bitmap.createBitmap(decoded.width, decoded.height, Bitmap.Config.ARGB_8888)

        // Convert RGBA to ARGB (Android's native format) via IntArray
        val pixels = IntArray(decoded.width * decoded.height)
        for (i in pixels.indices) {
            val offset = i * 4
            val r = decoded.pixels[offset].toInt() and 0xFF
            val g = decoded.pixels[offset + 1].toInt() and 0xFF
            val b = decoded.pixels[offset + 2].toInt() and 0xFF
            val a = decoded.pixels[offset + 3].toInt() and 0xFF
            pixels[i] = (a shl 24) or (r shl 16) or (g shl 8) or b
        }
        bitmap.setPixels(pixels, 0, decoded.width, 0, 0, decoded.width, decoded.height)

        return bitmap
    }
}
