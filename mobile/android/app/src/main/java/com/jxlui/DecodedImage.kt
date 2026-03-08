package com.jxlui

/** Data class returned from native JXL decoder via JNI. */
data class DecodedImage(
    val pixels: ByteArray,  // RGBA8
    val width: Int,
    val height: Int,
)
