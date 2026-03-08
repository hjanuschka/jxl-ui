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
    @State private var showGallery = true

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

    /// Bundled sample JXL files
    var sampleFiles: [String] {
        guard let path = Bundle.main.resourcePath else { return [] }
        let samplesPath = (path as NSString).appendingPathComponent("Samples")
        guard let files = try? FileManager.default.contentsOfDirectory(atPath: samplesPath) else { return [] }
        return files.filter { $0.hasSuffix(".jxl") }.sorted()
    }

    var body: some View {
        ZStack {
            Color.bgBase.ignoresSafeArea()

            // Main content
            if showGallery && image == nil && !isLoading {
                galleryView
            } else if isLoading {
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
                    Spacer().frame(height: 16)
                    Button("Back to gallery") {
                        errorMessage = nil
                        showGallery = true
                    }
                    .foregroundColor(.textSecondary)
                    .padding(.horizontal, 20)
                    .padding(.vertical, 10)
                    .background(Color.bgSurface)
                    .clipShape(RoundedRectangle(cornerRadius: 12))
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
            }

            // Top bar
            VStack(spacing: 0) {
                HStack {
                    if image != nil {
                        Button(action: {
                            image = nil
                            imageInfo = nil
                            showGallery = true
                            showInfo = false
                        }) {
                            Image(systemName: "chevron.left")
                                .foregroundColor(.textPrimary)
                                .frame(width: 40, height: 40)
                                .background(Color.bgSurface)
                                .clipShape(Circle())
                        }
                    }

                    Text(imageInfo?.fileName ?? "JXL Viewer")
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
                .padding(.bottom, 8)
                .background(Color.bgElevated.opacity(0.95))

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
                if image != nil {
                    HStack(spacing: 12) {
                        if let info = imageInfo {
                            Text("\(info.width)x\(info.height)")
                            Text("\(info.decodeTimeMs)ms")
                        }
                        Text("jxl-rs")
                            .foregroundColor(.accent)
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
                    showGallery = false
                    loadFile(url: url)
                }
            case .failure(let error):
                errorMessage = error.localizedDescription
            }
        }
    }

    // MARK: - Gallery View

    private var galleryView: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Text("JXL Viewer")
                    .foregroundColor(.textPrimary)
                    .font(.system(size: 24, weight: .bold))
                Text("Powered by jxl-rs")
                    .foregroundColor(.textMuted)
                    .font(.system(size: 13))

                // Open from device
                Button(action: { showFilePicker = true }) {
                    Text("Open from Device")
                        .font(.system(size: 15, weight: .medium))
                        .foregroundColor(.white)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 14)
                        .background(Color.accent)
                        .clipShape(RoundedRectangle(cornerRadius: 12))
                }

                Spacer().frame(height: 4)

                Text("SAMPLE IMAGES (\(sampleFiles.count))")
                    .foregroundColor(.textMuted)
                    .font(.system(size: 10, weight: .bold))

                LazyVGrid(columns: [
                    GridItem(.flexible(), spacing: 8),
                    GridItem(.flexible(), spacing: 8),
                ], spacing: 8) {
                    ForEach(sampleFiles, id: \.self) { name in
                        Button(action: {
                            showGallery = false
                            loadSample(name: name)
                        }) {
                            VStack(alignment: .leading, spacing: 4) {
                                Text("JXL")
                                    .foregroundColor(.accent)
                                    .font(.system(size: 11, weight: .bold))
                                Text(name.replacingOccurrences(of: ".jxl", with: ""))
                                    .foregroundColor(.textPrimary)
                                    .font(.system(size: 13))
                                    .lineLimit(2)
                                    .multilineTextAlignment(.leading)
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(12)
                            .background(Color.bgSurface)
                            .clipShape(RoundedRectangle(cornerRadius: 8))
                        }
                    }
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 64) // below top bar
        }
    }

    // MARK: - Loading

    private func loadFile(url: URL) {
        isLoading = true
        errorMessage = nil
        image = nil
        imageInfo = nil
        resetZoom()

        DispatchQueue.global(qos: .userInitiated).async {
            let accessing = url.startAccessingSecurityScopedResource()
            defer { if accessing { url.stopAccessingSecurityScopedResource() } }

            do {
                let data = try Data(contentsOf: url)
                decodeAndShow(data: data, name: url.lastPathComponent)
            } catch {
                DispatchQueue.main.async {
                    self.errorMessage = error.localizedDescription
                    self.isLoading = false
                }
            }
        }
    }

    private func loadSample(name: String) {
        isLoading = true
        errorMessage = nil
        image = nil
        imageInfo = nil
        resetZoom()

        DispatchQueue.global(qos: .userInitiated).async {
            guard let path = Bundle.main.path(forResource: name, ofType: nil, inDirectory: "Samples"),
                  let data = try? Data(contentsOf: URL(fileURLWithPath: path)) else {
                DispatchQueue.main.async {
                    self.errorMessage = "Cannot read sample file"
                    self.isLoading = false
                }
                return
            }
            decodeAndShow(data: data, name: name)
        }
    }

    private func decodeAndShow(data: Data, name: String) {
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
                fileName: name
            )
            self.isLoading = false
        }
    }

    private func resetZoom() {
        scale = 1
        lastScale = 1
        offset = .zero
        lastOffset = .zero
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
