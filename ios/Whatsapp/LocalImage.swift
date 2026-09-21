import ImageIO
import SwiftUI

private enum Thumbnails {
    static let cache: NSCache<NSString, UIImage> = {
        let cache = NSCache<NSString, UIImage>()
        cache.totalCostLimit = 24 * 1024 * 1024
        return cache
    }()

    static func load(_ url: URL, maximumSize: Int) -> UIImage? {
        let key = "\(url.path)-\(maximumSize)" as NSString
        if let image = cache.object(forKey: key) { return image }
        guard let source = CGImageSourceCreateWithURL(url as CFURL, [kCGImageSourceShouldCache: false] as CFDictionary),
              let cg = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: maximumSize,
                kCGImageSourceShouldCacheImmediately: true
              ] as CFDictionary) else { return nil }
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
            let image = await Task.detached(priority: .utility) { Thumbnails.load(url, maximumSize: maximumSize) }.value
            if !Task.isCancelled { self.image = image }
        }
    }
}
