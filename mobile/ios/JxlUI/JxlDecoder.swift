import UIKit

/// Swift wrapper around the jxl-mobile-core C FFI.
final class JxlDecoder {
    struct DecodedImage {
        let image: UIImage
        let width: Int
        let height: Int
        let decodeTimeMs: Int
    }

    struct ProgressiveUpdate {
        let image: UIImage?
        let width: Int
        let height: Int
        let completedPasses: Int
        let progressPct: Int
        let isFinal: Bool
    }

    struct AnimFrame {
        let image: UIImage
        let durationMs: Int
    }

    struct DecodedAnimation {
        let frames: [AnimFrame]
        let width: Int
        let height: Int
        let loopCount: Int
        let decodeTimeMs: Int
    }

    struct DecoderSettings {
        // Mirrors Android/Desktop values
        // color: Auto=0, Rgb=1, Rgba=2, Bgr=3, Bgra=4, Grayscale=5, GrayscaleAlpha=6
        // data:  F32=0, U8=1, U16=2, F16=3
        var colorType: UInt8 = 0
        var dataType: UInt8 = 0
        var premultiplyAlpha: Bool = true
        var linearOutput: Bool = false
        var highPrecision: Bool = false
        var simulateSlow: Bool = false
        var slowChunkPct: Float = 1.0
        var slowDelayMs: UInt64 = 50
    }

    fileprivate final class ProgressBox {
        let handler: (ProgressiveUpdate) -> Void
        init(handler: @escaping (ProgressiveUpdate) -> Void) {
            self.handler = handler
        }
    }

    /// Decode JXL data into a UIImage using settings.
    static func decode(_ data: Data, settings: DecoderSettings = DecoderSettings()) -> DecodedImage? {
        let start = CFAbsoluteTimeGetCurrent()

        let result: UnsafeMutablePointer<JxlImage>? = data.withUnsafeBytes { rawBuffer in
            guard let ptr = rawBuffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return nil
            }
            return jxl_decode_with_settings(
                ptr,
                rawBuffer.count,
                settings.colorType,
                settings.dataType,
                settings.premultiplyAlpha ? 1 : 0,
                settings.linearOutput ? 1 : 0,
                settings.highPrecision ? 1 : 0
            )
        }

        guard let img = result else { return nil }
        defer { jxl_image_free(img) }

        let width = Int(img.pointee.width)
        let height = Int(img.pointee.height)
        guard let uiImage = makeUIImageRGBA(
            pixels: img.pointee.pixels,
            pixelsLen: Int(img.pointee.pixels_len),
            width: width,
            height: height
        ) else {
            return nil
        }

        let elapsed = Int((CFAbsoluteTimeGetCurrent() - start) * 1000)
        return DecodedImage(image: uiImage, width: width, height: height, decodeTimeMs: elapsed)
    }

    static func isAnimation(_ data: Data) -> Bool {
        let result: UInt8 = data.withUnsafeBytes { rawBuffer in
            guard let ptr = rawBuffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return 0
            }
            return jxl_is_animation(ptr, rawBuffer.count)
        }
        return result != 0
    }

    static func decodeAnimation(_ data: Data) -> DecodedAnimation? {
        let start = CFAbsoluteTimeGetCurrent()

        let result: UnsafeMutablePointer<JxlAnimationResult>? = data.withUnsafeBytes { rawBuffer in
            guard let ptr = rawBuffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return nil
            }
            return jxl_decode_animation(ptr, rawBuffer.count)
        }

        guard let anim = result else { return nil }
        defer { jxl_animation_free(anim) }

        let count = Int(anim.pointee.frame_count)
        guard count > 0 else { return nil }

        var frames: [AnimFrame] = []
        frames.reserveCapacity(count)

        for i in 0..<count {
            let frame = anim.pointee.frames.advanced(by: i).pointee
            guard let image = makeUIImageRGBA(
                pixels: frame.pixels,
                pixelsLen: Int(frame.pixels_len),
                width: Int(frame.width),
                height: Int(frame.height)
            ) else {
                continue
            }
            frames.append(AnimFrame(image: image, durationMs: Int(frame.duration_ms)))
        }

        guard !frames.isEmpty else { return nil }

        let elapsed = Int((CFAbsoluteTimeGetCurrent() - start) * 1000)
        return DecodedAnimation(
            frames: frames,
            width: Int(anim.pointee.width),
            height: Int(anim.pointee.height),
            loopCount: Int(anim.pointee.loop_count),
            decodeTimeMs: elapsed
        )
    }

    /// Progressive decode with callback updates. Returns final decoded image.
    static func decodeProgressive(
        _ data: Data,
        settings: DecoderSettings,
        onProgress: @escaping (ProgressiveUpdate) -> Void
    ) -> DecodedImage? {
        let start = CFAbsoluteTimeGetCurrent()

        let box = ProgressBox(handler: onProgress)
        let retained = Unmanaged.passRetained(box)
        defer { retained.release() }

        let result: UnsafeMutablePointer<JxlImage>? = data.withUnsafeBytes { rawBuffer in
            guard let ptr = rawBuffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return nil
            }

            return jxl_decode_progressive(
                ptr,
                rawBuffer.count,
                settings.colorType,
                settings.dataType,
                settings.premultiplyAlpha ? 1 : 0,
                settings.linearOutput ? 1 : 0,
                settings.highPrecision ? 1 : 0,
                settings.simulateSlow ? 1 : 0,
                settings.slowChunkPct,
                settings.slowDelayMs,
                jxlProgressCallback,
                retained.toOpaque()
            )
        }

        guard let img = result else { return nil }
        defer { jxl_image_free(img) }

        let width = Int(img.pointee.width)
        let height = Int(img.pointee.height)
        guard let uiImage = makeUIImageRGBA(
            pixels: img.pointee.pixels,
            pixelsLen: Int(img.pointee.pixels_len),
            width: width,
            height: height
        ) else {
            return nil
        }

        let elapsed = Int((CFAbsoluteTimeGetCurrent() - start) * 1000)
        return DecodedImage(image: uiImage, width: width, height: height, decodeTimeMs: elapsed)
    }

    fileprivate static func makeUIImageRGBA(
        pixels: UnsafePointer<UInt8>?,
        pixelsLen: Int,
        width: Int,
        height: Int
    ) -> UIImage? {
        guard let pixels, pixelsLen >= width * height * 4 else { return nil }

        // Copy pixel memory so Swift-side image is independent from Rust allocation lifetime.
        let data = Data(bytes: pixels, count: width * height * 4) as CFData
        guard let provider = CGDataProvider(data: data) else { return nil }

        let colorSpace = CGColorSpaceCreateDeviceRGB()
        let bitmapInfo = CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue)

        guard let cgImage = CGImage(
            width: width,
            height: height,
            bitsPerComponent: 8,
            bitsPerPixel: 32,
            bytesPerRow: width * 4,
            space: colorSpace,
            bitmapInfo: bitmapInfo,
            provider: provider,
            decode: nil,
            shouldInterpolate: false,
            intent: .defaultIntent
        ) else {
            return nil
        }

        return UIImage(cgImage: cgImage)
    }
}

private func jxlProgressCallback(
    _ pixels: UnsafePointer<UInt8>?,
    _ pixelsLen: UInt32,
    _ width: UInt32,
    _ height: UInt32,
    _ completedPasses: UInt32,
    _ progressPct: UInt32,
    _ isFinal: UInt8,
    _ userData: UnsafeMutableRawPointer?
) {
    guard let userData else { return }
    let box = Unmanaged<JxlDecoder.ProgressBox>.fromOpaque(userData).takeUnretainedValue()

    let w = Int(width)
    let h = Int(height)
    let p = Int(progressPct)
    let passes = Int(completedPasses)

    var image: UIImage? = nil
    if let pixels, pixelsLen > 0 {
        image = JxlDecoder.makeUIImageRGBA(
            pixels: pixels,
            pixelsLen: Int(pixelsLen),
            width: w,
            height: h
        )
    }

    box.handler(JxlDecoder.ProgressiveUpdate(
        image: image,
        width: w,
        height: h,
        completedPasses: passes,
        progressPct: p,
        isFinal: isFinal != 0
    ))
}
