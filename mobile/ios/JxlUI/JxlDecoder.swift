import UIKit

/// Swift wrapper around the jxl-mobile-core C FFI.
class JxlDecoder {
    struct DecodedImage {
        let image: UIImage
        let width: Int
        let height: Int
        let decodeTimeMs: Int
    }

    /// Decode JXL data into a UIImage.
    static func decode(_ data: Data) -> DecodedImage? {
        let start = CFAbsoluteTimeGetCurrent()

        let result: UnsafeMutablePointer<JxlImage>? = data.withUnsafeBytes { rawBuffer in
            guard let ptr = rawBuffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return nil
            }
            return jxl_decode(ptr, rawBuffer.count)
        }

        guard let img = result else { return nil }
        defer { jxl_image_free(img) }

        let width = Int(img.pointee.width)
        let height = Int(img.pointee.height)
        let pixelCount = width * height * 4

        guard let pixels = img.pointee.pixels else { return nil }

        // Create CGImage from RGBA8 data
        let colorSpace = CGColorSpaceCreateDeviceRGB()
        let bitmapInfo = CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue)

        guard let provider = CGDataProvider(dataInfo: nil,
                                             data: pixels,
                                             size: pixelCount,
                                             releaseData: { _, _, _ in }) else { return nil }

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
            shouldInterpolate: true,
            intent: .defaultIntent
        ) else { return nil }

        let elapsed = Int((CFAbsoluteTimeGetCurrent() - start) * 1000)

        return DecodedImage(
            image: UIImage(cgImage: cgImage),
            width: width,
            height: height,
            decodeTimeMs: elapsed
        )
    }
}
