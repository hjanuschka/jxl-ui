// jxl_mobile_core.h - C FFI for JXL decoding (used by iOS/Swift)
#ifndef JXL_MOBILE_CORE_H
#define JXL_MOBILE_CORE_H

#include <stdint.h>
#include <stddef.h>

typedef struct {
    uint8_t *pixels;    // RGBA8 pixel data
    uint32_t width;
    uint32_t height;
    uint32_t pixels_len;
} JxlImage;

typedef struct {
    uint8_t *pixels;
    uint32_t pixels_len;
    uint32_t width;
    uint32_t height;
    uint32_t duration_ms;
} JxlAnimFrame;

typedef struct {
    JxlAnimFrame *frames;
    uint32_t frame_count;
    uint32_t width;
    uint32_t height;
    uint32_t loop_count;
} JxlAnimationResult;

typedef void (*JxlProgressCallback)(
    const uint8_t *pixels,
    uint32_t pixels_len,
    uint32_t width,
    uint32_t height,
    uint32_t completed_passes,
    uint32_t progress_pct,
    uint8_t is_final,
    void *user_data
);

// Decode JXL data. Returns NULL on error.
// Caller must free with jxl_image_free().
JxlImage *jxl_decode(const uint8_t *data, size_t data_len);

// Decode JXL with full settings.
JxlImage *jxl_decode_with_settings(
    const uint8_t *data,
    size_t data_len,
    uint8_t color_type,
    uint8_t data_type,
    uint8_t premultiply_alpha,
    uint8_t linear_output,
    uint8_t high_precision
);

// Progressive decode with callback updates.
JxlImage *jxl_decode_progressive(
    const uint8_t *data,
    size_t data_len,
    uint8_t color_type,
    uint8_t data_type,
    uint8_t premultiply_alpha,
    uint8_t linear_output,
    uint8_t high_precision,
    uint8_t simulate_slow,
    float slow_chunk_pct,
    uint64_t slow_delay_ms,
    JxlProgressCallback callback,
    void *user_data
);

// Returns 1 if this JXL has animation frames, otherwise 0.
uint8_t jxl_is_animation(const uint8_t *data, size_t data_len);

// Decode animation frames.
JxlAnimationResult *jxl_decode_animation(const uint8_t *data, size_t data_len);

// Decode with error reporting. error_buf receives null-terminated error string.
JxlImage *jxl_decode_with_error(const uint8_t *data, size_t data_len,
                                 uint8_t *error_buf, size_t error_buf_len);

// Free a decoded image.
void jxl_image_free(JxlImage *img);

// Free decoded animation frames.
void jxl_animation_free(JxlAnimationResult *anim);

#endif
