import SwiftUI
import UniformTypeIdentifiers

// Black matte theme matching Android/Desktop
extension Color {
    static let bgBase = Color(red: 18/255, green: 18/255, blue: 18/255)
    static let bgElevated = Color(red: 24/255, green: 24/255, blue: 24/255)
    static let bgSurface = Color(red: 30/255, green: 30/255, blue: 30/255)
    static let textPrimary = Color(red: 190/255, green: 190/255, blue: 190/255)
    static let textSecondary = Color(red: 161/255, green: 161/255, blue: 170/255)
    static let textMuted = Color(red: 138/255, green: 138/255, blue: 141/255)
    static let accent = Color(red: 255/255, green: 193/255, blue: 7/255)
    static let border = Color(red: 51/255, green: 51/255, blue: 51/255)
}

struct ContentView: View {
    @State private var image: UIImage?
    @State private var imageInfo: ImageInfo?
    @State private var isLoading = false
    @State private var errorMessage: String?
    @State private var showFilePicker = false
    @State private var showInfo = false
    @State private var showGallery = true
    @State private var showSettings = false

    // Progressive state
    @State private var progressPct: Int = 0
    @State private var completedPasses: Int = 0
    @State private var isProgressive = false

    // Animation state
    @State private var isAnimation = false
    @State private var animationFrames: [JxlDecoder.AnimFrame] = []
    @State private var currentFrameIndex: Int = 0
    @State private var animationTimer: Timer?

    // Decoder settings
    @State private var settings = JxlDecoder.DecoderSettings()

    // Reload and stale-callback guard
    @State private var decodeGeneration: Int = 0
    @State private var lastLoadedData: Data?
    @State private var lastLoadedName: String?

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

    var sampleFiles: [String] {
        guard let path = Bundle.main.resourcePath else { return [] }
        let samplesPath = (path as NSString).appendingPathComponent("Samples")
        guard let files = try? FileManager.default.contentsOfDirectory(atPath: samplesPath) else { return [] }
        return files.filter { $0.hasSuffix(".jxl") }.sorted()
    }

    var body: some View {
        GeometryReader { geo in
            ZStack {
            Color.bgBase.ignoresSafeArea()

            if showGallery && image == nil && !isLoading {
                galleryView
            } else if isLoading && !isProgressive {
                VStack(spacing: 16) {
                    ProgressView()
                        .tint(.accent)
                        .scaleEffect(1.4)
                    Text("Decoding...")
                        .foregroundColor(.textMuted)
                        .font(.system(size: 14, design: .monospaced))
                }
            } else if let error = errorMessage {
                VStack(spacing: 8) {
                    Text("Failed to load")
                        .foregroundColor(.textPrimary)
                        .font(.system(size: 18, design: .monospaced))
                    Text(error)
                        .foregroundColor(.textMuted)
                        .font(.system(size: 13, design: .monospaced))
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
                    .clipShape(RoundedRectangle(cornerRadius: 10))
                }
            } else if let image {
                GeometryReader { proxy in
                    Image(uiImage: image)
                        .resizable()
                        .interpolation(.none)
                        .antialiased(false)
                        .aspectRatio(contentMode: .fit)
                        .frame(width: proxy.size.width, height: proxy.size.height)
                        .ignoresSafeArea(edges: [.top, .bottom])
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
                                            resetZoom()
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
                                resetZoom()
                            }
                        }
                        .onTapGesture(count: 1) {
                            withAnimation { showInfo.toggle() }
                        }
                }
                .ignoresSafeArea(edges: [.top, .bottom])
            }

            if showInfo, let info = imageInfo {
                VStack {
                    Spacer()
                    HStack {
                        VStack(alignment: .leading, spacing: 4) {
                            Text("// image info")
                                .foregroundColor(.accent)
                                .font(.system(size: 13, design: .monospaced))
                            InfoRow(label: "size", value: "\(info.width) x \(info.height)")
                            InfoRow(label: "decode", value: "\(info.decodeTimeMs) ms")
                            InfoRow(label: "file", value: String(format: "%.1f KB", Double(info.fileSizeBytes) / 1024.0))
                            InfoRow(label: "color", value: colorLabel(settings.colorType))
                            InfoRow(label: "data", value: dataLabel(settings.dataType))
                        }
                        .padding(14)
                        Spacer()
                    }
                    .background(Color.bgElevated.opacity(0.95))
                    .overlay(RoundedRectangle(cornerRadius: 10).stroke(Color.border, lineWidth: 1))
                    .clipShape(RoundedRectangle(cornerRadius: 10))
                    .padding(.horizontal, 12)
                    .padding(.bottom, 68)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .transition(.opacity)
            }

            if isProgressive && progressPct < 100 && image != nil {
                VStack {
                    Spacer().frame(height: 96)
                    VStack(spacing: 6) {
                        Text("progressive decode")
                            .foregroundColor(.accent)
                            .font(.system(size: 12, design: .monospaced))
                        Text("\(progressPct)%  pass \(completedPasses)")
                            .foregroundColor(.textPrimary)
                            .font(.system(size: 12, design: .monospaced))
                        ProgressView(value: Double(progressPct), total: 100)
                            .tint(.accent)
                            .frame(width: 180)
                    }
                    .padding(.horizontal, 20)
                    .padding(.vertical, 12)
                    .background(Color.bgBase.opacity(0.94))
                    .overlay(RoundedRectangle(cornerRadius: 12).stroke(Color.accent.opacity(0.5), lineWidth: 1))
                    .clipShape(RoundedRectangle(cornerRadius: 12))
                    Spacer()
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            }

            if image != nil {
                VStack {
                    Spacer()
                    VStack(spacing: 0) {
                        Rectangle().fill(Color.accent.opacity(0.35)).frame(height: 1)
                        HStack(spacing: 12) {
                            if let info = imageInfo {
                                Text("\(info.width)x\(info.height)")
                                Text(info.decodeTimeMs > 0 ? "\(info.decodeTimeMs)ms" : "...")
                            }
                            if settings.simulateSlow { Text("progressive").foregroundColor(.accent.opacity(0.8)) }
                            Spacer()
                            Text("jxl-rs").foregroundColor(.accent)
                        }
                        .foregroundColor(.textMuted)
                        .font(.system(size: 12, design: .monospaced))
                        .padding(.horizontal, 16)
                        .padding(.vertical, 8)
                        .background(Color.bgElevated.opacity(0.92))
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }

            if showSettings {
                VStack {
                    HStack {
                        Spacer()
                        settingsPanel
                            .padding(.top, 72)
                            .padding(.trailing, 10)
                    }
                    Spacer()
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topTrailing)
                .transition(.opacity)
            }

            }
            .frame(width: geo.size.width, height: geo.size.height, alignment: .top)
            .safeAreaInset(edge: .top, spacing: 0) {
                topBar
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
            .onDisappear {
                stopAnimationPlayback()
            }
        }
    }

    private var topBar: some View {
        HStack(spacing: 8) {
            if image != nil {
                squareButton("<") {
                    clearImage()
                }
            }

            Text(imageInfo?.fileName ?? "jxl-viewer")
                .foregroundColor(.textPrimary)
                .font(.system(size: 16, weight: .medium, design: .monospaced))
                .lineLimit(1)

            Spacer()

            squareButton("⚙", highlighted: showSettings) {
                withAnimation { showSettings.toggle() }
            }

            if image != nil {
                squareButton("R") {
                    reloadImage()
                }
            }

            squareButton("+") {
                showFilePicker = true
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
        .background(Color.bgElevated.opacity(0.96))
    }

    private func squareButton(_ label: String, highlighted: Bool = false, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(label)
                .foregroundColor(highlighted ? .accent : .textSecondary)
                .font(.system(size: 14, design: .monospaced))
                .frame(width: 40, height: 40)
                .background(highlighted ? Color.accent.opacity(0.12) : Color.bgBase)
                .overlay(RoundedRectangle(cornerRadius: 8).stroke(highlighted ? Color.accent : Color.border, lineWidth: highlighted ? 2 : 1))
                .clipShape(RoundedRectangle(cornerRadius: 8))
        }
        .buttonStyle(.plain)
    }

    private var settingsPanel: some View {
        ScrollView(showsIndicators: true) {
            VStack(alignment: .leading, spacing: 10) {
                Text("// settings")
                .foregroundColor(.accent)
                .font(.system(size: 13, design: .monospaced))

            Text("color format")
                .foregroundColor(.textMuted)
                .font(.system(size: 11, design: .monospaced))

            ForEach([0, 1, 2, 3, 4, 5, 6], id: \.self) { ct in
                Button(action: {
                    var s = settings
                    s.colorType = UInt8(ct)
                    updateSettings(s)
                }) {
                    HStack {
                        Text(settings.colorType == UInt8(ct) ? ">" : " ")
                            .foregroundColor(.accent)
                            .frame(width: 10)
                        Text(colorLabel(UInt8(ct)))
                            .foregroundColor(settings.colorType == UInt8(ct) ? .textPrimary : .textSecondary)
                            .font(.system(size: 12, design: .monospaced))
                        Spacer()
                    }
                    .padding(.vertical, 3)
                }
                .buttonStyle(.plain)
            }

            Divider().overlay(Color.border)

            Text("data format")
                .foregroundColor(.textMuted)
                .font(.system(size: 11, design: .monospaced))

            ForEach([0, 1, 2, 3], id: \.self) { dt in
                Button(action: {
                    var s = settings
                    s.dataType = UInt8(dt)
                    updateSettings(s)
                }) {
                    HStack {
                        Text(settings.dataType == UInt8(dt) ? ">" : " ")
                            .foregroundColor(.accent)
                            .frame(width: 10)
                        Text(dataLabel(UInt8(dt)))
                            .foregroundColor(settings.dataType == UInt8(dt) ? .textPrimary : .textSecondary)
                            .font(.system(size: 12, design: .monospaced))
                        Spacer()
                    }
                    .padding(.vertical, 3)
                }
                .buttonStyle(.plain)
            }

            Divider().overlay(Color.border)

            Text("options")
                .foregroundColor(.textMuted)
                .font(.system(size: 11, design: .monospaced))

            Toggle(isOn: Binding(get: { settings.premultiplyAlpha }, set: { value in
                var s = settings
                s.premultiplyAlpha = value
                updateSettings(s)
            })) {
                Text("premultiply alpha")
                    .foregroundColor(.textSecondary)
                    .font(.system(size: 12, design: .monospaced))
            }
            .tint(.accent)

            Toggle(isOn: Binding(get: { settings.linearOutput }, set: { value in
                var s = settings
                s.linearOutput = value
                updateSettings(s)
            })) {
                Text("linear output (xyb)")
                    .foregroundColor(.textSecondary)
                    .font(.system(size: 12, design: .monospaced))
            }
            .tint(.accent)

            Toggle(isOn: Binding(get: { settings.highPrecision }, set: { value in
                var s = settings
                s.highPrecision = value
                updateSettings(s)
            })) {
                Text("high precision")
                    .foregroundColor(.textSecondary)
                    .font(.system(size: 12, design: .monospaced))
            }
            .tint(.accent)

            Divider().overlay(Color.border)

            Text("progressive demo")
                .foregroundColor(.textMuted)
                .font(.system(size: 11, design: .monospaced))

            Toggle(isOn: Binding(get: { settings.simulateSlow }, set: { value in
                var s = settings
                s.simulateSlow = value
                updateSettings(s)
            })) {
                Text("slow loading")
                    .foregroundColor(.textSecondary)
                    .font(.system(size: 12, design: .monospaced))
            }
            .tint(.accent)

            if settings.simulateSlow {
                Text("chunk: \(String(format: "%.1f", settings.slowChunkPct))%")
                    .foregroundColor(.textMuted)
                    .font(.system(size: 11, design: .monospaced))
                Slider(value: Binding(get: {
                    settings.slowChunkPct
                }, set: { value in
                    var s = settings
                    s.slowChunkPct = value
                    updateSettings(s)
                }), in: 0.1...10.0)
                .tint(.accent)

                Text("delay: \(settings.slowDelayMs)ms")
                    .foregroundColor(.textMuted)
                    .font(.system(size: 11, design: .monospaced))
                Slider(value: Binding(get: {
                    Double(settings.slowDelayMs)
                }, set: { value in
                    var s = settings
                    s.slowDelayMs = UInt64(value)
                    updateSettings(s)
                }), in: 1...500, step: 1)
                .tint(.accent)

                let pctPerSec = (1000.0 / Double(max(1, settings.slowDelayMs))) * Double(settings.slowChunkPct)
                Text(String(format: "~%.0f%%/s  %.1fs total", pctPerSec, pctPerSec > 0 ? (100.0 / pctPerSec) : 0.0))
                    .foregroundColor(.textMuted)
                    .font(.system(size: 10, design: .monospaced))
            }

            if image != nil {
                Button("apply & reload") {
                    reloadImage()
                    withAnimation { showSettings = false }
                }
                .foregroundColor(.accent)
                .font(.system(size: 12, design: .monospaced))
                .padding(.top, 6)
            } else {
                Text("load an image first")
                    .foregroundColor(.textMuted)
                    .font(.system(size: 11, design: .monospaced))
                    .padding(.top, 6)
            }
            }
            .padding(14)
            .frame(width: 280, alignment: .leading)
        }
        .frame(width: 280)
        .frame(maxHeight: UIScreen.main.bounds.height * 0.72)
        .background(Color.bgElevated)
        .overlay(RoundedRectangle(cornerRadius: 12).stroke(Color.border, lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: 12))
    }

    private var galleryView: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Text("jxl-viewer")
                    .foregroundColor(.accent)
                    .font(.system(size: 24, weight: .bold, design: .monospaced))
                Text("powered by jxl-rs")
                    .foregroundColor(.textMuted)
                    .font(.system(size: 13, design: .monospaced))

                Button(action: { showFilePicker = true }) {
                    Text("open from device")
                        .foregroundColor(.accent)
                        .font(.system(size: 13, weight: .medium, design: .monospaced))
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 12)
                        .overlay(RoundedRectangle(cornerRadius: 8).stroke(Color.accent, lineWidth: 2))
                }
                .buttonStyle(.plain)

                Text("samples (\(sampleFiles.count))")
                    .foregroundColor(.textMuted)
                    .font(.system(size: 10, weight: .bold, design: .monospaced))

                LazyVGrid(columns: [
                    GridItem(.flexible(), spacing: 8),
                    GridItem(.flexible(), spacing: 8)
                ], spacing: 8) {
                    ForEach(sampleFiles, id: \.self) { name in
                        Button(action: {
                            showGallery = false
                            loadSample(name: name)
                        }) {
                            Text(name.replacingOccurrences(of: ".jxl", with: ""))
                                .foregroundColor(.textPrimary)
                                .font(.system(size: 13, design: .monospaced))
                                .lineLimit(1)
                                .truncationMode(.tail)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .frame(height: 72, alignment: .center)
                                .padding(.horizontal, 12)
                                .background(LinearGradient(colors: [.bgSurface, .bgElevated.opacity(0.7)], startPoint: .topLeading, endPoint: .bottomTrailing))
                                .overlay(RoundedRectangle(cornerRadius: 8).stroke(Color.border, lineWidth: 1))
                                .clipShape(RoundedRectangle(cornerRadius: 8))
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 8)
            .padding(.bottom, 24)
        }
    }

    private func clearImage() {
        decodeGeneration += 1
        stopAnimationPlayback()
        image = nil
        imageInfo = nil
        isLoading = false
        isProgressive = false
        isAnimation = false
        animationFrames = []
        currentFrameIndex = 0
        progressPct = 0
        completedPasses = 0
        showGallery = true
        showInfo = false
    }

    private func reloadImage() {
        guard let data = lastLoadedData, let name = lastLoadedName else { return }
        startDecode(data: data, name: name)
    }

    private func loadFile(url: URL) {
        isLoading = true
        errorMessage = nil
        resetZoom()

        DispatchQueue.global(qos: .userInitiated).async {
            let accessing = url.startAccessingSecurityScopedResource()
            defer { if accessing { url.stopAccessingSecurityScopedResource() } }

            do {
                let data = try Data(contentsOf: url)
                startDecode(data: data, name: url.lastPathComponent)
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
            startDecode(data: data, name: name)
        }
    }

    private func startDecode(data: Data, name: String) {
        let generation = decodeGeneration + 1
        DispatchQueue.main.async {
            self.decodeGeneration = generation
            self.stopAnimationPlayback()
            self.lastLoadedData = data
            self.lastLoadedName = name
            self.image = nil
            self.imageInfo = nil
            self.errorMessage = nil
            self.isLoading = false
            self.isProgressive = true
            self.isAnimation = false
            self.animationFrames = []
            self.currentFrameIndex = 0
            self.progressPct = 0
            self.completedPasses = 0
        }

        let decodeSettings = settings

        DispatchQueue.global(qos: .userInitiated).async {
            if JxlDecoder.isAnimation(data), let anim = JxlDecoder.decodeAnimation(data) {
                DispatchQueue.main.async {
                    guard self.decodeGeneration == generation else { return }
                    self.isAnimation = true
                    self.animationFrames = anim.frames
                    self.currentFrameIndex = 0
                    self.image = anim.frames.first?.image
                    self.imageInfo = ImageInfo(
                        width: anim.width,
                        height: anim.height,
                        decodeTimeMs: anim.decodeTimeMs,
                        fileSizeBytes: data.count,
                        fileName: name
                    )
                    self.progressPct = 100
                    self.isProgressive = false
                    self.isLoading = false
                    self.startAnimationPlayback(generation: generation)
                }
                return
            }

            let result = JxlDecoder.decodeProgressive(data, settings: decodeSettings) { update in
                DispatchQueue.main.async {
                    guard self.decodeGeneration == generation else { return }
                    if let img = update.image {
                        self.image = img
                    }
                    self.progressPct = update.progressPct
                    self.completedPasses = update.completedPasses
                    self.isProgressive = !update.isFinal
                    self.isLoading = false
                }
            }

            DispatchQueue.main.async {
                guard self.decodeGeneration == generation else { return }
                guard let decoded = result else {
                    self.errorMessage = "Failed to decode JXL image"
                    self.isLoading = false
                    self.isProgressive = false
                    return
                }

                self.image = decoded.image
                self.imageInfo = ImageInfo(
                    width: decoded.width,
                    height: decoded.height,
                    decodeTimeMs: decoded.decodeTimeMs,
                    fileSizeBytes: data.count,
                    fileName: name
                )
                self.progressPct = 100
                self.isProgressive = false
                self.isLoading = false
            }
        }
    }

    private func updateSettings(_ newSettings: JxlDecoder.DecoderSettings) {
        let formatChanged = newSettings.colorType != settings.colorType
            || newSettings.dataType != settings.dataType
            || newSettings.premultiplyAlpha != settings.premultiplyAlpha
            || newSettings.linearOutput != settings.linearOutput
            || newSettings.highPrecision != settings.highPrecision

        settings = newSettings

        if image != nil && !isAnimation && formatChanged {
            reloadImage()
        }
    }

    private func stopAnimationPlayback() {
        animationTimer?.invalidate()
        animationTimer = nil
    }

    private func startAnimationPlayback(generation: Int) {
        stopAnimationPlayback()
        guard animationFrames.count > 1 else { return }

        func scheduleNext() {
            guard decodeGeneration == generation else { return }
            let frame = animationFrames[currentFrameIndex]
            let delay = max(16, frame.durationMs)
            animationTimer = Timer.scheduledTimer(withTimeInterval: Double(delay) / 1000.0, repeats: false) { _ in
                guard decodeGeneration == generation else { return }
                guard !animationFrames.isEmpty else { return }
                currentFrameIndex = (currentFrameIndex + 1) % animationFrames.count
                image = animationFrames[currentFrameIndex].image
                scheduleNext()
            }
        }

        scheduleNext()
    }

    private func resetZoom() {
        scale = 1
        lastScale = 1
        offset = .zero
        lastOffset = .zero
    }

    private func colorLabel(_ value: UInt8) -> String {
        switch value {
        case 1: return "RGB"
        case 2: return "RGBA"
        case 3: return "BGR"
        case 4: return "BGRA"
        case 5: return "Grayscale"
        case 6: return "Grayscale + Alpha"
        default: return "Auto"
        }
    }

    private func dataLabel(_ value: UInt8) -> String {
        switch value {
        case 1: return "Unsigned 8-bit (u8)"
        case 2: return "Unsigned 16-bit (u16)"
        case 3: return "Float16 (f16)"
        default: return "Float32 (f32)"
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
                .font(.system(size: 12, design: .monospaced))
            Spacer()
            Text(value)
                .foregroundColor(.textSecondary)
                .font(.system(size: 12, design: .monospaced))
        }
    }
}
