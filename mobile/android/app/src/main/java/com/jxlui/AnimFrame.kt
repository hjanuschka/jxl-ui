package com.jxlui

/**
 * A single animation frame from JXL decoding.
 */
data class AnimFrame(
    val pixels: ByteArray,
    val width: Int,
    val height: Int,
    val durationMs: Int,
)
