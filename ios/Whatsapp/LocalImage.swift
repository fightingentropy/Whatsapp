import ImageIO
import AVFoundation
import SwiftUI

enum Thumbnails {
    struct Request: Hashable {
        let url: URL
        let maximumSize: Int
        let revision: String
        init(_ url: URL, maximumSize: Int, revision: String? = nil) {
            self.url = url
            self.maximumSize = maximumSize
            self.revision = revision ?? Thumbnails.revision(url)
        }
        var key: NSString { "\(url.path)-\(maximumSize)-\(revision)" as NSString }
    }

    static let cache: NSCache<NSString, UIImage> = {
        let cache = NSCache<NSString, UIImage>()
        cache.totalCostLimit = 24 * 1024 * 1024
        return cache
    }()

    static func clear() { cache.removeAllObjects() }

    static func revision(_ url: URL) -> String {
        var fresh = url
        fresh.removeAllCachedResourceValues()
        guard let values = try? fresh.resourceValues(forKeys: [.contentModificationDateKey, .fileSizeKey]) else { return "missing" }
        return "\(values.contentModificationDate?.timeIntervalSince1970 ?? 0)-\(values.fileSize ?? 0)"
    }

    static func cached(_ request: Request) -> UIImage? { cache.object(forKey: request.key) }
    static let loader = ThumbnailLoader()

    static func keep(_ image: UIImage, for request: Request) {
        let cost = image.cgImage.map { $0.bytesPerRow * $0.height } ?? 0
        cache.setObject(image, forKey: request.key, cost: cost)
    }

    // Used on the engine queue before publishing locally restored rows. Only
    // small, downsampled stills are decoded; videos remain asynchronous/on demand.
    @discardableResult static func prepareStill(_ url: URL, maximumSize: Int) -> UIImage? {
        let request = Request(url, maximumSize: maximumSize, revision: MediaFiles.inspect(url))
        guard let image = cached(request) ?? decodeStill(request) else { return nil }
        keep(image, for: request)
        return image
    }

    private static func decodeStill(_ request: Request) -> UIImage? {
        let url = request.url
        let maximumSize = request.maximumSize
        if let image = cached(request) { return image }
        guard !["mp4", "mov", "m4v"].contains(url.pathExtension.lowercased()),
              let source = CGImageSourceCreateWithURL(url as CFURL, [kCGImageSourceShouldCache: false] as CFDictionary),
              let cg = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: maximumSize,
                kCGImageSourceShouldCacheImmediately: true
              ] as CFDictionary) else { return nil }
        return UIImage(cgImage: cg)
    }

    static func load(_ url: URL, maximumSize: Int) async -> UIImage? {
        await load(Request(url, maximumSize: maximumSize))
    }

    static func load(_ request: Request) async -> UIImage? {
        await loader.load(request)
    }

    static func decode(_ request: Request) async -> UIImage? {
        guard !Task.isCancelled else { return nil }
        if let image = cached(request) { return image }
        let url = request.url
        let maximumSize = request.maximumSize
        let cg: CGImage
        if ["mp4", "mov", "m4v"].contains(url.pathExtension.lowercased()) {
            let generator = AVAssetImageGenerator(asset: AVURLAsset(url: url))
            generator.maximumSize = CGSize(width: maximumSize, height: maximumSize)
            generator.appliesPreferredTrackTransform = true
            let frame = try? await withTaskCancellationHandler {
                try await generator.image(at: .zero)
            } onCancel: { generator.cancelAllCGImageGeneration() }
            guard !Task.isCancelled, let frame else { return nil }
            cg = frame.image
        } else {
            return autoreleasepool { decodeStill(request) }
        }
        let image = UIImage(cgImage: cg)
        return image
    }
}

struct LocalImage: View {
    private let request: Thumbnails.Request
    @State private var loaded: Loaded?
    private struct Loaded {
        let request: Thumbnails.Request
        let image: UIImage
    }

    init(url: URL, maximumSize: Int) {
        let request = Thumbnails.Request(url, maximumSize: maximumSize, revision: MediaFiles.revision(url) ?? "uninspected")
        self.request = request
        _loaded = State(initialValue: Thumbnails.cached(request).map { Loaded(request: request, image: $0) })
    }

    // SwiftUI can recreate a row with empty @State while its image is warm.
    // Read the memory cache during the first render, before .task is scheduled.
    private var image: UIImage? {
        Thumbnails.cached(request) ?? (loaded?.request.url == request.url && loaded?.request.maximumSize == request.maximumSize ? loaded?.image : nil)
    }

    var body: some View {
        Group {
            if let image { Image(uiImage: image).resizable() }
            else { Image(systemName: "photo").resizable().foregroundStyle(.secondary).padding(16) }
        }
        .task(id: request) {
            if let image = Thumbnails.cached(request) {
                loaded = Loaded(request: request, image: image)
                return
            }
            let image = await Thumbnails.load(request)
            if !Task.isCancelled, let image { loaded = Loaded(request: request, image: image) }
        }
    }
}
