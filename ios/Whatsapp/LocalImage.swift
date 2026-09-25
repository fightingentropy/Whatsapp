import ImageIO
import AVFoundation
import SwiftUI

enum Thumbnails {
    static let cache: NSCache<NSString, UIImage> = {
        let cache = NSCache<NSString, UIImage>()
        cache.totalCostLimit = 24 * 1024 * 1024
        return cache
    }()

    static func clear() { cache.removeAllObjects() }

    static func load(_ url: URL, maximumSize: Int) async -> UIImage? {
        let key = "\(url.path)-\(maximumSize)" as NSString
        if let image = cache.object(forKey: key) { return image }
        let cg: CGImage
        if ["mp4", "mov", "m4v"].contains(url.pathExtension.lowercased()) {
            let generator = AVAssetImageGenerator(asset: AVURLAsset(url: url))
            generator.maximumSize = CGSize(width: maximumSize, height: maximumSize)
            generator.appliesPreferredTrackTransform = true
            guard let frame = try? await generator.image(at: .zero) else { return nil }
            cg = frame.image
        } else {
            guard let source = CGImageSourceCreateWithURL(url as CFURL, [kCGImageSourceShouldCache: false] as CFDictionary),
                  let thumbnail = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                    kCGImageSourceCreateThumbnailFromImageAlways: true,
                    kCGImageSourceCreateThumbnailWithTransform: true,
                    kCGImageSourceThumbnailMaxPixelSize: maximumSize,
                    kCGImageSourceShouldCacheImmediately: true
                  ] as CFDictionary) else { return nil }
            cg = thumbnail
        }
        let image = UIImage(cgImage: cg)
        cache.setObject(image, forKey: key, cost: cg.bytesPerRow * cg.height)
        return image
    }
}

struct LocalImage: View {
    let url: URL
    let maximumSize: Int
    @State private var image: UIImage?

    var body: some View {
        Group {
            if let image { Image(uiImage: image).resizable() }
            else { Image(systemName: "photo").resizable().foregroundStyle(.secondary).padding(16) }
        }
        .task(id: url) {
            let image = await Task.detached(priority: .utility) { await Thumbnails.load(url, maximumSize: maximumSize) }.value
            if !Task.isCancelled { self.image = image }
        }
    }
}
