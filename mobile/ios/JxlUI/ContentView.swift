import SwiftUI
import UniformTypeIdentifiers

// Theme colors matching desktop jxl-ui
extension Color {
    static let bgBase = Color(red: 17/255, green: 17/255, blue: 19/255)
    static let bgElevated = Color(red: 24/255, green: 24/255, blue: 27/255)
    static let bgSurface = Color(red: 32/255, green: 32/255, blue: 36/255)
    static let textPrimary = Color(red: 250/255, green: 250/255, blue: 250/255)
    static let textSecondary = Color(red: 161/255, green: 161/255, blue: 170/255)
    static let textMuted = Color(red: 113/255, green: 113/255, blue: 122/255)
    static let accent = Color(red: 99/255, green: 102/255, blue: 241/255)
}

struct ContentView: View {
    @State private var image: UIImage?
    @State private var imageInfo: ImageInfo?
    @State private var isLoading = false
    @State private var errorMessage: String?
    @State private var showFilePicker = false
    @State private var showInfo = false

    // Zoom & pan
    @State private var scale: CGFloat = 1
    @State private var lastScale: CGFloat = 1
    @State private var offset: CGSize = .zero
    @State private var lastOffset: CGSize = .zero

    struct ImageInfo {
        let width: Int
        let height: Int
        let decodeTimeMs: Int
        let fileSizeBytes: Int
        let fileName: String
    }

    var body: some View {
        ZStack {
            Color.bgBase.ignoresSafeArea()

            // Main content
            if isLoading {
                VStack(spacing: 16) {
                    ProgressView()
                        .tint(.accent)
                        .scaleEffect(1.5)
                    Text("Decoding...")
                        .foregroundColor(.textMuted)
                        .font(.system(size: 14))
                }
            } else if let error = errorMessage {
                VStack(spacing: 8) {
                    Text("Failed to load")
                        .foregroundColor(.textPrimary)
                        .font(.system(size: 18))
                    Text(error)
                        .foregroundColor(.textMuted)
                        .font(.system(size: 13))
                        .multilineTextAlignment(.center)
                        .padding(.horizontal, 32)
                }
            } else if let image = image {
                Image(uiImage: image)
                    .resizable()
                    .aspectRatio(contentMode: .fit)
                    .scaleEffect(scale)
                    .offset(offset)
                    .gesture(
                        MagnificationGesture()
                            .onChanged { value in
                                scale = lastScale * value
                            }
                            .onEnded { _ in
                                lastScale = scale
                                if scale < 1 {
                                    withAnimation(.spring()) {
                                        scale = 1
                                        lastScale = 1
                                        offset = .zero
                                        lastOffset = .zero
                                    }
                                }
                            }
                    )
                    .simultaneousGesture(
                        DragGesture()
                            .onChanged { value in
                                offset = CGSize(
                                    width: lastOffset.width + value.translation.width,
                                    height: lastOffset.height + value.translation.height
                                )
                            }
                            .onEnded { _ in
                                lastOffset = offset
                            }
                    )
                    .onTapGesture(count: 2) {
                        withAnimation(.spring()) {
                            scale = 1
                            lastScale = 1
                            offset = .zero
                            lastOffset = .zero
                        }
                    }
                    .onTapGesture(count: 1) {
                        withAnimation { showInfo.toggle() }
                    }
            } else {
                // Empty state
                VStack(spacing: 8) {
                    Text("JXL Viewer")
                        .foregroundColor(.textPrimary)
                        .font(.system(size: 24, weight: .bold))
                    Text("Powered by jxl-rs")
                        .foregroundColor(.textMuted)
                        .font(.system(size: 13))
                    Spacer().frame(height: 24)
                    Button(action: { showFilePicker = true }) {
                        Text("Open JXL File")
                            .font(.system(size: 16, weight: .medium))
                            .foregroundColor(.white)
                            .padding(.horizontal, 24)
                            .padding(.vertical, 12)
                            .background(.accent)
                            .clipShape(RoundedRectangle(cornerRadius: 12))
                    }
                }
            }

            // Top bar
            VStack {
                HStack {
                    Text(imageInfo?.fileName ?? "JXL-UI")
                        .foregroundColor(.textPrimary)
                        .font(.system(size: 16, weight: .medium))
                        .lineLimit(1)

                    Spacer()

                    Button(action: { showFilePicker = true }) {
                        Image(systemName: "plus")
                            .foregroundColor(.textPrimary)
                            .frame(width: 40, height: 40)
                            .background(Color.bgSurface)
                            .clipShape(Circle())
                    }
                }
                .padding(.horizontal, 16)
                .padding(.top, 8)

                Spacer()

                // Info overlay
                if showInfo, let info = imageInfo {
                    HStack {
                        VStack(alignment: .leading, spacing: 4) {
                            Text("Image Info")
                                .foregroundColor(.textPrimary)
                                .font(.system(size: 14, weight: .bold))

                            InfoRow(label: "Size", value: "\(info.width) x \(info.height)")
                            InfoRow(label: "Megapixels", value: String(format: "%.2f MP", Double(info.width) * Double(info.height) / 1_000_000))
                            InfoRow(label: "Decode time", value: "\(info.decodeTimeMs) ms")
                            if info.fileSizeBytes > 0 {
                                InfoRow(label: "File size", value: String(format: "%.1f KB", Double(info.fileSizeBytes) / 1024))
                            }
                        }
                        .padding(16)
                        Spacer()
                    }
                    .background(Color.bgElevated.opacity(0.9))
                    .clipShape(RoundedRectangle(cornerRadius: 12))
                    .padding(16)
                    .transition(.opacity)
                }

                // Bottom status
                if image != nil, let info = imageInfo {
                    HStack(spacing: 12) {
                        Text("\(info.width)x\(info.height)")
                        Text("\(info.decodeTimeMs)ms")
                    }
                    .foregroundColor(.textMuted)
                    .font(.system(size: 12))
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 8)
                    .background(Color.bgElevated.opacity(0.8))
                }
            }
        }
        .fileImporter(
            isPresented: $showFilePicker,
            allowedContentTypes: [.data],
            allowsMultipleSelection: false
        ) { result in
            switch result {
            case .success(let urls):
                if let url = urls.first {
                    loadFile(url: url)
                }
            case .failure(let error):
                errorMessage = error.localizedDescription
            }
        }
    }

    private func loadFile(url: URL) {
        isLoading = true
        errorMessage = nil
        image = nil
        imageInfo = nil

        // Reset zoom
        scale = 1
        lastScale = 1
        offset = .zero
        lastOffset = .zero

        DispatchQueue.global(qos: .userInitiated).async {
            let accessing = url.startAccessingSecurityScopedResource()
            defer { if accessing { url.stopAccessingSecurityScopedResource() } }

            do {
                let data = try Data(contentsOf: url)
                guard let decoded = JxlDecoder.decode(data) else {
                    DispatchQueue.main.async {
                        self.errorMessage = "Failed to decode JXL image"
                        self.isLoading = false
                    }
                    return
                }

                DispatchQueue.main.async {
                    self.image = decoded.image
                    self.imageInfo = ImageInfo(
                        width: decoded.width,
                        height: decoded.height,
                        decodeTimeMs: decoded.decodeTimeMs,
                        fileSizeBytes: data.count,
                        fileName: url.lastPathComponent
                    )
                    self.isLoading = false
                }
            } catch {
                DispatchQueue.main.async {
                    self.errorMessage = error.localizedDescription
                    self.isLoading = false
                }
            }
        }
    }
}

struct InfoRow: View {
    let label: String
    let value: String

    var body: some View {
        HStack {
            Text(label)
                .foregroundColor(.textMuted)
                .font(.system(size: 12))
            Spacer()
            Text(value)
                .foregroundColor(.textSecondary)
                .font(.system(size: 12))
        }
    }
}
