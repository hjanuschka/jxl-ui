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

// Decode JXL data. Returns NULL on error.
// Caller must free with jxl_image_free().
JxlImage *jxl_decode(const uint8_t *data, size_t data_len);

// Decode with error reporting. error_buf receives null-terminated error string.
JxlImage *jxl_decode_with_error(const uint8_t *data, size_t data_len,
                                 uint8_t *error_buf, size_t error_buf_len);

// Free a decoded image.
void jxl_image_free(JxlImage *img);

#endif
